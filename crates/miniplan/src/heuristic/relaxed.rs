//! Delete-relaxation heuristics: h^add, h^max and h^FF.
//!
//! All three explore the same relaxed planning graph (RPG), in which
//! operators never delete facts. The forward pass assigns every fact a
//! relaxed cost (`0` in the evaluated state, `f64::INFINITY` when unreachable)
//! and remembers the operator that achieved it most cheaply.

use fixedbitset::FixedBitSet;

use crate::search::HValue;
use crate::search::Heuristic;
use crate::task::{FactId, OpId, Operator, State, Task};

/// The h^add heuristic (Bonet & Geffner, 2001).
///
/// Sums the relaxed costs of all goal facts, assuming subgoals are independent.
/// Not admissible (can overestimate), but often more informed than h^max.
///
/// # Examples
///
/// ```
/// use miniplan::heuristic::HAdd;
/// use miniplan::search::Heuristic;
///
/// assert_eq!(HAdd.name(), "hadd");
/// ```
pub struct HAdd;

/// The h^max heuristic (Bonet & Geffner, 2001).
///
/// Takes the maximum relaxed cost among all goal facts.
/// Admissible but often weak (underestimates significantly).
///
/// # Examples
///
/// ```
/// use miniplan::heuristic::HMax;
/// use miniplan::search::Heuristic;
///
/// assert_eq!(HMax.name(), "hmax");
/// ```
pub struct HMax;

/// The FF heuristic (Hoffmann & Nebel, 2001).
///
/// Extracts a relaxed plan from the RPG and sums its operator costs.
/// Not admissible but highly informative in practice.
///
/// # Examples
///
/// ```
/// use miniplan::heuristic::HFF;
/// use miniplan::search::Heuristic;
///
/// assert_eq!(HFF.name(), "hff");
/// ```
pub struct HFF;

impl Heuristic for HAdd {
    fn name(&self) -> &'static str {
        "hadd"
    }

    fn estimate(&self, task: &Task, state: &State) -> HValue {
        let rpg = build_rpg(task, state);
        let mut cost = 0.0;
        for bit in task.goal_pos.0.ones() {
            cost += rpg.fact_cost[bit];
        }

        HValue(cost)
    }
}

impl Heuristic for HMax {
    fn name(&self) -> &'static str {
        "hmax"
    }

    fn estimate(&self, task: &Task, state: &State) -> HValue {
        let rpg = build_rpg(task, state);
        let mut max_cost: f64 = 0.0;
        for bit in task.goal_pos.0.ones() {
            max_cost = max_cost.max(rpg.fact_cost[bit]);
        }

        HValue(max_cost)
    }
}

impl Heuristic for HFF {
    fn name(&self) -> &'static str {
        "hff"
    }

    fn estimate(&self, task: &Task, state: &State) -> HValue {
        let rpg = build_rpg(task, state);
        let cost = extract_relaxed_plan_cost(task, &rpg);
        HValue(cost)
    }

    fn preferred_ops(&self, _task: &Task, _state: &State) -> &[OpId] {
        /// h^FF computes no preferred operators yet, so every state gets this empty list.
        static EMPTY: [OpId; 0] = [];
        &EMPTY
    }
}

/// Outcome of the forward relaxed exploration from one state.
struct RpgResult {
    /// Relaxed cost per fact index: `0.0` for facts of the start state,
    /// `f64::INFINITY` for facts the relaxation cannot reach.
    fact_cost: Vec<f64>,
    /// Index into `task.operators` of the operator that set each fact's
    /// current cost; `None` for start facts and unreachable facts.
    achiever: Vec<Option<usize>>,
}

/// Facts newly reached during one sweep over all operators.
#[derive(Debug, Default)]
struct Sweep {
    /// Whether some fact was reached for the first time.
    reached_new: bool,
    /// Whether at least one operator was applicable in the relaxed state.
    any_applicable: bool,
}

/// Mutable state of the forward relaxed exploration.
///
/// Costs follow the additive rule: an operator costs its own cost plus the sum
/// of its positive preconditions' costs. Negative preconditions count as
/// satisfied while their fact has not been reached yet.
struct RpgBuilder<'a> {
    /// The task whose operators are relaxed.
    task: &'a Task,
    /// Current relaxed cost per fact index; see [`RpgResult::fact_cost`].
    fact_cost: Vec<f64>,
    /// Current cheapest achiever per fact index; see [`RpgResult::achiever`].
    achiever: Vec<Option<usize>>,
    /// Facts reached so far; only grows.
    reached: FixedBitSet,
}

impl<'a> RpgBuilder<'a> {
    /// Starts the exploration with every fact of `state` reached at cost 0.
    fn new(task: &'a Task, state: &State) -> Self {
        let num_facts = task.num_facts();
        let mut builder = Self {
            task,
            fact_cost: vec![f64::INFINITY; num_facts],
            achiever: vec![None; num_facts],
            reached: FixedBitSet::with_capacity(num_facts),
        };

        for bit in state.0.ones() {
            builder.fact_cost[bit] = 0.0;
            builder.reached.set(bit, true);
        }

        builder
    }

    /// Applies every relaxed-applicable operator once, in operator order.
    ///
    /// Facts reached earlier in the same sweep already count for later
    /// operators.
    fn sweep(&mut self) -> Sweep {
        let task = self.task;
        let mut sweep = Sweep::default();

        for (op_idx, op) in task.operators.iter().enumerate() {
            if self.holds(&op.pre_pos, &op.pre_neg) {
                sweep.any_applicable = true;
                sweep.reached_new |= self.relax_operator(op_idx, op);
            }
        }

        sweep
    }

    /// Lowers the costs of the facts `op` adds, including the adds of its
    /// conditional effects whose condition holds in the relaxed state.
    ///
    /// Returns whether a fact was reached for the first time.
    fn relax_operator(&mut self, op_idx: usize, op: &Operator) -> bool {
        let cost = f64::from(op.cost) + self.cost_sum(&op.pre_pos);
        let mut reached_new = self.lower_costs(&op.add, cost, op_idx);

        for cond in &op.conditional {
            if self.holds(&cond.cond_pos, &cond.cond_neg) {
                let cond_cost = cost + self.cost_sum(&cond.cond_pos);
                reached_new |= self.lower_costs(&cond.add, cond_cost, op_idx);
            }
        }

        reached_new
    }

    /// Whether every fact of `pos` is reached and no fact of `neg` is.
    fn holds(&self, pos: &State, neg: &State) -> bool {
        pos.0.ones().all(|b| self.reached.contains(b))
            && neg.0.ones().all(|b| !self.reached.contains(b))
    }

    /// Sum of the current relaxed costs of the facts in `facts`.
    fn cost_sum(&self, facts: &State) -> f64 {
        facts.0.ones().map(|b| self.fact_cost[b]).sum::<f64>()
    }

    /// Records `cost` and `op_idx` for every fact of `add` that is currently
    /// more expensive, marking it reached.
    ///
    /// Returns whether a fact was reached for the first time.
    fn lower_costs(&mut self, add: &State, cost: f64, op_idx: usize) -> bool {
        let mut reached_new = false;
        for bit in add.0.ones() {
            if cost < self.fact_cost[bit] {
                self.fact_cost[bit] = cost;
                self.achiever[bit] = Some(op_idx);
                if !self.reached.contains(bit) {
                    self.reached.set(bit, true);
                    reached_new = true;
                }
            }
        }

        reached_new
    }

    /// Ends the exploration and hands out the costs and achievers.
    fn finish(self) -> RpgResult {
        RpgResult {
            fact_cost: self.fact_cost,
            achiever: self.achiever,
        }
    }
}

/// Runs the forward relaxed exploration from `state`.
///
/// Sweeps repeat until a sweep reaches no new fact. As a safety net, the
/// exploration also stops once more than twice as many non-empty sweeps
/// (levels) as there are facts have run.
fn build_rpg(task: &Task, state: &State) -> RpgResult {
    let max_levels = task.num_facts() * 2;
    let mut builder = RpgBuilder::new(task, state);
    let mut num_levels = 0usize;

    loop {
        let sweep = builder.sweep();
        if sweep.reached_new || sweep.any_applicable {
            num_levels += 1;
        }

        if !sweep.reached_new || num_levels > max_levels {
            break;
        }
    }

    builder.finish()
}

/// Forward relaxed costs of all facts from `state`, indexed by fact id.
///
/// `0.0` marks facts of `state`, `f64::INFINITY` unreachable facts.
pub(crate) fn rpg_fact_costs(task: &Task, state: &State) -> Vec<f64> {
    build_rpg(task, state).fact_cost
}

/// Cost of a relaxed plan for the positive goal, extracted backwards from the
/// achievers of `rpg`.
///
/// Each achiever operator is counted once, however many facts it supports.
/// Unreachable goal facts are skipped, so the result is finite even when the
/// goal is relaxed-unreachable.
fn extract_relaxed_plan_cost(task: &Task, rpg: &RpgResult) -> f64 {
    let mut marked_ops = std::collections::HashSet::new();
    let mut total_cost = 0.0;

    let mut goals_to_achieve: Vec<FactId> = task
        .goal_pos
        .0
        .ones()
        .filter(|&b| rpg.fact_cost[b] < f64::INFINITY)
        .map(FactId)
        .collect();

    let mut visited = std::collections::HashSet::new();

    while let Some(fact) = goals_to_achieve.pop() {
        if !visited.insert(fact) {
            continue;
        }

        if let Some(Some(op_idx)) = rpg.achiever.get(fact.0).copied()
            && marked_ops.insert(op_idx)
            && let Some(op) = task.operators.get(op_idx)
        {
            total_cost += f64::from(op.cost);
            for pre_bit in op.pre_pos.0.ones() {
                if !visited.contains(&FactId(pre_bit)) {
                    goals_to_achieve.push(FactId(pre_bit));
                }
            }
        }
    }

    total_cost
}

/// Backward relaxed costs: an estimate, per fact id, of the cost from a state
/// containing that fact to the goal.
///
/// Goal facts cost `0.0`. An operator whose add effects all have a finite cost
/// gives each of its positive preconditions at most the largest such cost plus
/// the operator cost; this is iterated to a fixpoint. Facts that never get a
/// cost stay at `f64::INFINITY`. `goal_neg` is currently ignored.
pub(crate) fn rpg_backward_fact_costs(task: &Task, goal_pos: &State, goal_neg: &State) -> Vec<f64> {
    let num_facts = task.num_facts();
    let mut back_cost = vec![f64::INFINITY; num_facts];

    for b in goal_pos.0.ones() {
        back_cost[b] = 0.0;
    }

    let _ = goal_neg;

    let mut changed = true;
    while changed {
        changed = false;
        for op in &task.operators {
            let Some(max_add_cost) = max_add_cost(op, &back_cost) else {
                continue;
            };

            let op_total = max_add_cost + f64::from(op.cost);
            for p in op.pre_pos.0.ones() {
                if op_total < back_cost[p] {
                    back_cost[p] = op_total;
                    changed = true;
                }
            }
        }
    }

    back_cost
}

/// Largest backward cost among the add effects of `op` (at least `0.0`).
///
/// Returns `None` when `op` adds nothing or some add effect has no finite
/// backward cost yet.
fn max_add_cost(op: &Operator, back_cost: &[f64]) -> Option<f64> {
    if op.add.0.is_empty() {
        return None;
    }

    let mut max_cost = 0.0f64;
    for b in op.add.0.ones() {
        let c = back_cost[b];
        if c == f64::INFINITY {
            return None;
        }

        if c > max_cost {
            max_cost = c;
        }
    }

    Some(max_cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Fact, FactId, OpId, Operator, Task, TaskMeta, TypeHierarchy};
    use rustc_hash::FxHashMap;

    /// Builds a task over `num_facts` nullary facts `f0..fN` with the given
    /// initial state, goal bits and operators.
    fn make_test_task(
        num_facts: usize,
        init_bits: &[usize],
        goal_pos_bits: &[usize],
        goal_neg_bits: &[usize],
        operators: Vec<Operator>,
    ) -> Task {
        let mut facts = Vec::new();
        let mut fact_index = FxHashMap::default();
        for i in 0..num_facts {
            let fact = Fact {
                predicate: format!("f{i}"),
                args: vec![],
            };
            let id = FactId(i);
            fact_index.insert(fact.clone(), id);
            facts.push(fact);
        }
        let mut init = State::new(num_facts);
        for &b in init_bits {
            init.0.set(b, true);
        }
        let mut goal_pos = State::new(num_facts);
        for &b in goal_pos_bits {
            goal_pos.0.set(b, true);
        }
        let mut goal_neg = State::new(num_facts);
        for &b in goal_neg_bits {
            goal_neg.0.set(b, true);
        }
        Task {
            facts,
            fact_index,
            operators,
            init,
            goal_pos,
            goal_neg,
            objects: vec![],
            types: TypeHierarchy::new(),
            metadata: TaskMeta {
                domain_name: "test".to_string(),
                problem_name: "test".to_string(),
                requirements: vec![],
            },
        }
    }

    /// Builds an operator over a 10-fact universe from bit lists.
    fn make_op(
        id: usize,
        name: &str,
        pre_pos: &[usize],
        pre_neg: &[usize],
        add: &[usize],
        del: &[usize],
        cost: u32,
    ) -> Operator {
        let s = |bits: &[usize], size: usize| -> State {
            let mut state = State::new(size);
            for &b in bits {
                state.0.set(b, true);
            }
            state
        };
        Operator {
            id: OpId(id),
            name: name.to_string(),
            pre_pos: s(pre_pos, 10),
            pre_neg: s(pre_neg, 10),
            add: s(add, 10),
            del: s(del, 10),
            conditional: vec![],
            cost,
        }
    }

    /// Goal facts have backward cost zero.
    #[test]
    fn test_backward_rpg_goal_facts_cost_zero() {
        let task = make_test_task(
            3,
            &[0],
            &[2],
            &[],
            vec![
                make_op(0, "op0", &[0], &[], &[1], &[], 1),
                make_op(1, "op1", &[1], &[], &[2], &[], 1),
            ],
        );
        let costs = rpg_backward_fact_costs(&task, &task.goal_pos, &task.goal_neg);
        assert_eq!(costs[2], 0.0);
    }

    /// A fact no operator regresses to keeps an infinite backward cost.
    #[test]
    fn test_backward_rpg_unreachable_fact_stays_infinity() {
        let task = make_test_task(
            4,
            &[0],
            &[3],
            &[],
            vec![
                make_op(0, "op0", &[0], &[], &[1], &[], 1),
                make_op(1, "op1", &[1], &[], &[3], &[], 1),
            ],
        );
        let costs = rpg_backward_fact_costs(&task, &task.goal_pos, &task.goal_neg);
        assert_eq!(costs[2], f64::INFINITY);
    }

    /// Backward costs accumulate along a two-operator chain.
    #[test]
    fn test_backward_rpg_two_step_regression() {
        let task = make_test_task(
            4,
            &[0],
            &[3],
            &[],
            vec![
                make_op(0, "op0", &[0], &[], &[1], &[], 1),
                make_op(1, "op1", &[1], &[], &[3], &[], 1),
            ],
        );
        let costs = rpg_backward_fact_costs(&task, &task.goal_pos, &task.goal_neg);
        assert!(costs[1] >= 1.0);
        assert!(costs[0] >= 2.0);
    }
}
