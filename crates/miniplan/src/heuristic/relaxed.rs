use crate::search::HValue;
use crate::search::Heuristic;
use crate::task::{FactId, OpId, State, Task};

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
    fn name(&self) -> &str {
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
    fn name(&self) -> &str {
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
    fn name(&self) -> &str {
        "hff"
    }

    fn estimate(&self, task: &Task, state: &State) -> HValue {
        let rpg = build_rpg(task, state);
        let cost = extract_relaxed_plan_cost(task, &rpg);
        HValue(cost)
    }

    fn preferred_ops(&self, _task: &Task, _state: &State) -> &[OpId] {
        static EMPTY: [OpId; 0] = [];
        &EMPTY
    }
}

#[allow(dead_code)]
struct RpgLevel {
    facts: Vec<FactId>,
    applicable_ops: Vec<usize>,
}

#[allow(dead_code)]
struct RpgResult {
    fact_cost: Vec<f64>,
    achiever: Vec<Option<usize>>,
    levels: Vec<RpgLevel>,
}

fn build_rpg(task: &Task, state: &State) -> RpgResult {
    let num_facts = task.num_facts();
    let mut fact_cost = vec![f64::INFINITY; num_facts];
    let mut achiever: Vec<Option<usize>> = vec![None; num_facts];
    let mut levels: Vec<RpgLevel> = Vec::new();

    let mut achieved = fixedbitset::FixedBitSet::with_capacity(num_facts);

    for bit in state.0.ones() {
        fact_cost[bit] = 0.0;
        achieved.set(bit, true);
    }

    let mut changed = true;
    while changed {
        changed = false;
        let mut level_facts = Vec::new();
        let mut level_ops = Vec::new();

        for (op_idx, op) in task.operators.iter().enumerate() {
            let pre_satisfied = op.pre_pos.0.ones().all(|b| achieved.contains(b));
            let neg_satisfied = op.pre_neg.0.ones().all(|b| !achieved.contains(b));

            if pre_satisfied && neg_satisfied {
                let _op_cost = if level_ops.is_empty() {
                    op.pre_pos
                        .0
                        .ones()
                        .map(|b| fact_cost[b])
                        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .unwrap_or(0.0)
                } else {
                    0.0
                };

                let cost = op.cost as f64 + op.pre_pos.0.ones().map(|b| fact_cost[b]).sum::<f64>();

                for bit in op.add.0.ones() {
                    if cost < fact_cost[bit] {
                        fact_cost[bit] = cost;
                        achiever[bit] = Some(op_idx);
                        if !achieved.contains(bit) {
                            achieved.set(bit, true);
                            level_facts.push(FactId(bit));
                            changed = true;
                        }
                    }
                }

                for cond in &op.conditional {
                    let cond_satisfied = cond.cond_pos.0.ones().all(|b| achieved.contains(b))
                        && cond.cond_neg.0.ones().all(|b| !achieved.contains(b));

                    if cond_satisfied {
                        let cond_cost =
                            cost + cond.cond_pos.0.ones().map(|b| fact_cost[b]).sum::<f64>();

                        for bit in cond.add.0.ones() {
                            if cond_cost < fact_cost[bit] {
                                fact_cost[bit] = cond_cost;
                                achiever[bit] = Some(op_idx);
                                if !achieved.contains(bit) {
                                    achieved.set(bit, true);
                                    level_facts.push(FactId(bit));
                                    changed = true;
                                }
                            }
                        }
                    }
                }

                level_ops.push(op_idx);
            }
        }

        if !level_facts.is_empty() || !level_ops.is_empty() {
            levels.push(RpgLevel {
                facts: level_facts,
                applicable_ops: level_ops,
            });
        }

        if levels.len() > num_facts * 2 {
            break;
        }
    }

    RpgResult {
        fact_cost,
        achiever,
        levels,
    }
}

pub(crate) fn rpg_fact_costs(task: &Task, state: &State) -> Vec<f64> {
    build_rpg(task, state).fact_cost
}

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
        if visited.contains(&fact) {
            continue;
        }
        visited.insert(fact);

        if let Some(Some(op_idx)) = rpg.achiever.get(fact.0).copied()
            && !marked_ops.contains(&op_idx)
        {
            marked_ops.insert(op_idx);
            if let Some(op) = task.operators.get(op_idx) {
                total_cost += op.cost as f64;
                for pre_bit in op.pre_pos.0.ones() {
                    if !visited.contains(&FactId(pre_bit)) {
                        goals_to_achieve.push(FactId(pre_bit));
                    }
                }
            }
        }
    }

    total_cost
}

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
            if op.add.0.is_empty() {
                continue;
            }
            let mut max_add_cost = 0.0f64;
            let mut all_finite = true;
            for b in op.add.0.ones() {
                let c = back_cost[b];
                if c == f64::INFINITY {
                    all_finite = false;
                    break;
                }
                if c > max_add_cost {
                    max_add_cost = c;
                }
            }
            if !all_finite {
                continue;
            }
            let op_total = max_add_cost + op.cost as f64;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Fact, FactId, OpId, Operator, Task, TaskMeta, TypeHierarchy};
    use rustc_hash::FxHashMap;

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
                predicate: format!("f{}", i),
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
