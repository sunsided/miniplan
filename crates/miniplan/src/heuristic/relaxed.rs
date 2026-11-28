//! Delete-relaxation heuristics: h^add, h^max and h^FF.
//!
//! All three explore the same relaxed planning graph (RPG), in which
//! operators never delete facts and negative conditions are ignored. The
//! forward pass assigns every fact a relaxed cost (`0` in the evaluated state,
//! `f64::INFINITY` when unreachable) and remembers the operator that achieved
//! it most cheaply. h^add and h^FF combine precondition costs by sum, h^max by
//! maximum.

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
        let rpg = build_rpg(task, state, Aggregate::Sum);
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
        let rpg = build_rpg(task, state, Aggregate::Max);
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
        let rpg = build_rpg(task, state, Aggregate::Sum);
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

/// How the relaxed exploration combines the costs of an operator's preconditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aggregate {
    /// Sum of the precondition costs (h^add, h^FF).
    Sum,
    /// Largest precondition cost (h^max). Gives admissible, consistent per-fact costs.
    Max,
}

/// Outcome of one sweep over all operators.
#[derive(Debug, Default)]
struct Sweep {
    /// Whether some fact got a lower cost, including being reached for the first time.
    lowered: bool,
    /// Whether at least one operator was applicable in the relaxed state.
    any_applicable: bool,
}

/// Mutable state of the forward relaxed exploration.
///
/// An operator costs its own cost plus the [`Aggregate`] of its positive
/// preconditions' costs. Negative preconditions are ignored, as the delete
/// relaxation intends: a relaxed state never loses a fact, so it cannot tell
/// whether a fact could have been deleted again.
struct RpgBuilder<'a> {
    /// The task whose operators are relaxed.
    task: &'a Task,
    /// How precondition costs are combined.
    aggregate: Aggregate,
    /// Current relaxed cost per fact index; see [`RpgResult::fact_cost`].
    fact_cost: Vec<f64>,
    /// Current cheapest achiever per fact index; see [`RpgResult::achiever`].
    achiever: Vec<Option<usize>>,
    /// Facts reached so far; only grows.
    reached: FixedBitSet,
}

impl<'a> RpgBuilder<'a> {
    /// Starts the exploration with every fact of `state` reached at cost 0.
    fn new(task: &'a Task, state: &State, aggregate: Aggregate) -> Self {
        let num_facts = task.num_facts();
        let mut builder = Self {
            task,
            aggregate,
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
            if self.holds(&op.pre_pos) {
                sweep.any_applicable = true;
                sweep.lowered |= self.relax_operator(op_idx, op);
            }
        }

        sweep
    }

    /// Lowers the costs of the facts `op` adds, including the adds of its
    /// conditional effects whose condition holds in the relaxed state.
    ///
    /// Returns whether some fact got a lower cost.
    fn relax_operator(&mut self, op_idx: usize, op: &Operator) -> bool {
        let pre_cost = self.combined_cost(&op.pre_pos);
        let cost = f64::from(op.cost) + pre_cost;
        let mut lowered = self.lower_costs(&op.add, cost, op_idx);

        for cond in &op.conditional {
            if self.holds(&cond.cond_pos) {
                let cond_cost = f64::from(op.cost)
                    + match self.aggregate {
                        Aggregate::Sum => pre_cost + self.combined_cost(&cond.cond_pos),
                        Aggregate::Max => pre_cost.max(self.combined_cost(&cond.cond_pos)),
                    };
                lowered |= self.lower_costs(&cond.add, cond_cost, op_idx);
            }
        }

        lowered
    }

    /// Whether every fact of `pos` is reached.
    fn holds(&self, pos: &State) -> bool {
        pos.0.ones().all(|b| self.reached.contains(b))
    }

    /// The current relaxed costs of the facts in `facts`, combined by [`Self::aggregate`].
    fn combined_cost(&self, facts: &State) -> f64 {
        let costs = facts.0.ones().map(|b| self.fact_cost[b]);
        match self.aggregate {
            Aggregate::Sum => costs.sum::<f64>(),
            Aggregate::Max => costs.fold(0.0, f64::max),
        }
    }

    /// Records `cost` and `op_idx` for every fact of `add` that is currently
    /// more expensive, marking it reached.
    ///
    /// Returns whether some fact got a lower cost.
    fn lower_costs(&mut self, add: &State, cost: f64, op_idx: usize) -> bool {
        let mut lowered = false;
        for bit in add.0.ones() {
            if cost < self.fact_cost[bit] {
                self.fact_cost[bit] = cost;
                self.achiever[bit] = Some(op_idx);
                self.reached.set(bit, true);
                lowered = true;
            }
        }

        lowered
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
/// Sweeps repeat until a sweep lowers no cost, so the costs are the fixpoint
/// of the [`Aggregate`] rule.
///
/// With non-negative operator costs every fact's fixpoint cost has a
/// derivation in which each fact's cheapest achiever depends only on facts
/// derived before it, so its depth is at most the number of facts. A sweep
/// applies every operator, so after `k` sweeps every fact of depth `k` or
/// less has its final cost, however many times costs dropped on the way.
/// The fixpoint is therefore reached within `num_facts` sweeps and confirmed
/// by one more. The cap of twice that many sweeps is only a safety net; it is
/// asserted never to cut the fixpoint short in debug builds.
fn build_rpg(task: &Task, state: &State, aggregate: Aggregate) -> RpgResult {
    let max_levels = task.num_facts() * 2;
    let mut builder = RpgBuilder::new(task, state, aggregate);
    let mut num_levels = 0usize;

    loop {
        let sweep = builder.sweep();
        if sweep.lowered || sweep.any_applicable {
            num_levels += 1;
        }

        if !sweep.lowered {
            break;
        }

        if num_levels > max_levels {
            debug_assert!(false, "relaxed exploration exceeded {max_levels} sweeps");
            break;
        }
    }

    builder.finish()
}

/// Forward h^max costs of all facts from `state`, indexed by fact id.
///
/// `0.0` marks facts of `state`, `f64::INFINITY` facts the relaxation cannot
/// reach. The largest cost among a set of facts is an admissible and
/// consistent estimate of the cost from `state` to a state containing them,
/// which the bidirectional planners use as their backward heuristic.
pub(crate) fn rpg_fact_costs(task: &Task, state: &State) -> Vec<f64> {
    build_rpg(task, state, Aggregate::Max).fact_cost
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

/// Backward relaxed costs, per fact id, for the backward search over subgoals.
///
/// The largest cost among a subgoal's required facts is a consistent lower
/// bound on the cost of regressing from the goal to that subgoal, and is
/// finite for every subgoal the regression can reach.
///
/// Goal facts cost `0.0`. Every positive precondition of an operator costs at
/// most the operator cost plus the cheapest subgoal the operator can be
/// regressed through: the cheapest of its add effects, or `0.0` when it
/// deletes a fact some subgoal may forbid (one of `goal_neg` or of any
/// operator's negative preconditions), since regression then needs no add
/// effect. This is iterated to a fixpoint. Facts that never get a cost stay at
/// `f64::INFINITY`.
pub(crate) fn rpg_backward_fact_costs(task: &Task, goal_pos: &State, goal_neg: &State) -> Vec<f64> {
    let num_facts = task.num_facts();
    let mut back_cost = vec![f64::INFINITY; num_facts];

    for b in goal_pos.0.ones() {
        back_cost[b] = 0.0;
    }

    let mut forbiddable = goal_neg.0.clone();
    for op in &task.operators {
        forbiddable.union_with(&op.pre_neg.0);
    }

    let mut changed = true;
    while changed {
        changed = false;
        for op in &task.operators {
            let Some(after_cost) = min_regression_cost(op, &back_cost, &forbiddable) else {
                continue;
            };

            let op_total = after_cost + f64::from(op.cost);
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

/// Lowest backward cost of a subgoal that `op` can be regressed through.
///
/// That is `0.0` when `op` deletes a fact of `forbiddable`, otherwise the
/// cheapest backward cost among its add effects. Returns `None` when no such
/// subgoal has a finite cost yet.
fn min_regression_cost(op: &Operator, back_cost: &[f64], forbiddable: &FixedBitSet) -> Option<f64> {
    if !op.del.0.is_disjoint(forbiddable) {
        return Some(0.0);
    }

    op.add
        .0
        .ones()
        .map(|b| back_cost[b])
        .filter(|c| c.is_finite())
        .reduce(f64::min)
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

    /// Operators listed against the chain order need one sweep per chain link,
    /// and an expensive shortcut found first is lowered again in the last
    /// sweep; the exploration still ends at the exact h^max fixpoint.
    #[test]
    fn rpg_reaches_fixpoint_on_reversed_chain() {
        // Chain f0 -> f1 -> ... -> f9 at cost 1 per link, listed last link first,
        // plus a direct f0 -> f9 shortcut at cost 100 listed before everything.
        let mut ops = vec![make_op(0, "shortcut", &[0], &[], &[9], &[], 100)];
        for link in (0..9).rev() {
            ops.push(make_op(
                ops.len(),
                "link",
                &[link],
                &[],
                &[link + 1],
                &[],
                1,
            ));
        }
        let task = make_test_task(10, &[0], &[9], &[], ops);

        let costs = rpg_fact_costs(&task, &task.init);

        let expected: Vec<f64> = (0..10).map(f64::from).collect();
        assert_eq!(costs, expected);
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
