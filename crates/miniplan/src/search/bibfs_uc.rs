// Note: for cost-aware bidirectional search, use the `bidij` planner
// (Bidirectional Dijkstra). NBS is implemented in `search::nbs`; BAE* is
// implemented in `search::bae`.
// TODO(cond-effects): support via regression through conditional effects
// TODO(perf): per-FactId bucket index to avoid O(|F|·|B|) scan

use std::collections::VecDeque;
use std::time::Instant;

use rustc_hash::FxHashMap;

use crate::error::MiniplanError;
use crate::plan::{Plan, PlanStep};
use crate::search::{Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats, Task};
use crate::task::{OpId, Operator, State};

type SubgoalKey = (State, State);

#[derive(Default)]
pub struct BibfsUc {}

impl BibfsUc {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Planner for BibfsUc {
    fn name(&self) -> &str {
        "bibfs-uc"
    }

    fn describe(&self) -> &str {
        "Bidirectional BFS (uniform-cost, not cost-aware)"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL | PlannerCapabilities::NEGATIVE_PRECONDS
    }

    fn solve(
        &mut self,
        task: &Task,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        let start = Instant::now();
        let mut stats = SearchStats::default();

        for op in &task.operators {
            if !op.conditional.is_empty() {
                return Err(MiniplanError::UnsupportedConditionalEffects);
            }
        }

        if task.init.satisfies(&task.goal_pos, &task.goal_neg) {
            stats.elapsed = start.elapsed();
            return Ok(SearchOutcome::Plan(Plan::new(), stats));
        }

        let num_facts = task.num_facts();

        let mut forward_queue: VecDeque<State> = VecDeque::new();
        let mut forward_closed: FxHashMap<State, (Option<State>, OpId)> = FxHashMap::default();

        forward_queue.push_back(task.init.clone());
        forward_closed.insert(task.init.clone(), (None, OpId(usize::MAX)));

        let init_subgoal = (task.goal_pos.clone(), task.goal_neg.clone());
        let mut backward_queue: VecDeque<SubgoalKey> = VecDeque::new();
        let mut backward_closed: FxHashMap<SubgoalKey, (Option<SubgoalKey>, OpId)> =
            FxHashMap::default();

        backward_queue.push_back(init_subgoal.clone());
        backward_closed.insert(init_subgoal, (None, OpId(usize::MAX)));

        loop {
            let forward_empty = forward_queue.is_empty();
            let backward_empty = backward_queue.is_empty();

            if forward_empty && backward_empty {
                stats.elapsed = start.elapsed();
                return Ok(SearchOutcome::Unsolvable(stats));
            }

            if !forward_empty {
                let layer_size = forward_queue.len();
                let mut new_forward: Vec<State> = Vec::new();

                for _ in 0..layer_size {
                    let state = forward_queue.pop_front().unwrap();
                    stats.nodes_expanded += 1;

                    if let Some(max_nodes) = limits.node_budget
                        && stats.nodes_expanded >= max_nodes
                    {
                        stats.elapsed = start.elapsed();
                        return Ok(SearchOutcome::LimitReached(stats));
                    }
                    if let Some(timeout) = limits.time_budget
                        && start.elapsed() >= timeout
                    {
                        stats.elapsed = start.elapsed();
                        return Ok(SearchOutcome::LimitReached(stats));
                    }

                    for op in &task.operators {
                        if state.applicable(op) {
                            let next = state.apply(op);
                            if !forward_closed.contains_key(&next) {
                                forward_closed.insert(next.clone(), (Some(state.clone()), op.id));
                                new_forward.push(next);
                            }
                        }
                    }
                }

                for s in &new_forward {
                    forward_queue.push_back(s.clone());
                    if let Some(plan) =
                        check_meet_from_forward(s, &backward_closed, &forward_closed, task)
                    {
                        stats.plan_cost = plan.cost;
                        stats.plan_length = plan.len();
                        stats.elapsed = start.elapsed();
                        return Ok(SearchOutcome::Plan(plan, stats));
                    }
                }
            }

            if !backward_empty {
                let layer_size = backward_queue.len();
                let mut new_backward: Vec<SubgoalKey> = Vec::new();

                for _ in 0..layer_size {
                    let (g_pos, g_neg) = backward_queue.pop_front().unwrap();
                    stats.nodes_expanded += 1;

                    if let Some(max_nodes) = limits.node_budget
                        && stats.nodes_expanded >= max_nodes
                    {
                        stats.elapsed = start.elapsed();
                        return Ok(SearchOutcome::LimitReached(stats));
                    }
                    if let Some(timeout) = limits.time_budget
                        && start.elapsed() >= timeout
                    {
                        stats.elapsed = start.elapsed();
                        return Ok(SearchOutcome::LimitReached(stats));
                    }

                    for op in &task.operators {
                        if is_relevant(op, &g_pos, &g_neg) && is_consistent(op, &g_pos, &g_neg) {
                            let mut g_pos_new = State::new(num_facts);
                            let mut g_neg_new = State::new(num_facts);
                            for bit in g_pos.0.ones() {
                                if !op.add.0.contains(bit) {
                                    g_pos_new.0.set(bit, true);
                                }
                            }
                            for bit in op.pre_pos.0.ones() {
                                g_pos_new.0.set(bit, true);
                            }
                            for bit in g_neg.0.ones() {
                                if !op.del.0.contains(bit) {
                                    g_neg_new.0.set(bit, true);
                                }
                            }
                            for bit in op.pre_neg.0.ones() {
                                g_neg_new.0.set(bit, true);
                            }
                            let new_sg = (g_pos_new, g_neg_new);
                            if !backward_closed.contains_key(&new_sg) {
                                backward_closed.insert(
                                    new_sg.clone(),
                                    (Some((g_pos.clone(), g_neg.clone())), op.id),
                                );
                                new_backward.push(new_sg);
                            }
                        }
                    }
                }

                for sg in &new_backward {
                    backward_queue.push_back(sg.clone());
                    if let Some(plan) =
                        check_meet_from_backward(sg, &forward_closed, &backward_closed, task)
                    {
                        stats.plan_cost = plan.cost;
                        stats.plan_length = plan.len();
                        stats.elapsed = start.elapsed();
                        return Ok(SearchOutcome::Plan(plan, stats));
                    }
                }
            }
        }
    }
}

fn is_relevant(op: &Operator, g_pos: &State, g_neg: &State) -> bool {
    op.add.0.ones().any(|b| g_pos.0.contains(b)) || op.del.0.ones().any(|b| g_neg.0.contains(b))
}

fn is_consistent(op: &Operator, g_pos: &State, g_neg: &State) -> bool {
    !op.del.0.ones().any(|b| g_pos.0.contains(b)) && !op.add.0.ones().any(|b| g_neg.0.contains(b))
}

fn build_plan_from_ops(ops: &[OpId], task: &Task) -> Plan {
    let mut plan = Plan::new();
    let mut total_cost = 0.0;
    for &op_id in ops {
        if let Some(op) = task.operators.get(op_id.0) {
            plan.steps.push(PlanStep {
                op_id,
                op_name: op.name.clone(),
            });
            total_cost += op.cost as f64;
        }
    }
    plan.cost = total_cost;
    plan
}

fn check_meet_from_forward(
    s: &State,
    backward_closed: &FxHashMap<SubgoalKey, (Option<SubgoalKey>, OpId)>,
    forward_closed: &FxHashMap<State, (Option<State>, OpId)>,
    task: &Task,
) -> Option<Plan> {
    for (g_pos, g_neg) in backward_closed.keys() {
        if s.satisfies(g_pos, g_neg) {
            return reconstruct_plan(
                s,
                (g_pos.clone(), g_neg.clone()),
                forward_closed,
                backward_closed,
                task,
            );
        }
    }
    None
}

fn check_meet_from_backward(
    sg: &SubgoalKey,
    forward_closed: &FxHashMap<State, (Option<State>, OpId)>,
    backward_closed: &FxHashMap<SubgoalKey, (Option<SubgoalKey>, OpId)>,
    task: &Task,
) -> Option<Plan> {
    for (f_state, (_parent, _op)) in forward_closed {
        if f_state.satisfies(&sg.0, &sg.1) {
            return reconstruct_plan(f_state, sg.clone(), forward_closed, backward_closed, task);
        }
    }
    None
}

fn reconstruct_plan(
    meet_state: &State,
    meet_subgoal: SubgoalKey,
    forward_closed: &FxHashMap<State, (Option<State>, OpId)>,
    backward_closed: &FxHashMap<SubgoalKey, (Option<SubgoalKey>, OpId)>,
    task: &Task,
) -> Option<Plan> {
    let mut forward_ops: Vec<OpId> = Vec::new();
    let mut current = meet_state.clone();
    loop {
        if let Some((parent, op)) = forward_closed.get(&current) {
            if op.0 == usize::MAX {
                break;
            }
            forward_ops.push(*op);
            current = parent.clone().unwrap();
        } else {
            break;
        }
    }
    forward_ops.reverse();

    let mut backward_ops: Vec<OpId> = Vec::new();
    let mut sg_current = meet_subgoal.clone();
    loop {
        if let Some((parent, op)) = backward_closed.get(&sg_current) {
            if op.0 == usize::MAX {
                break;
            }
            backward_ops.push(*op);
            sg_current = parent.clone().unwrap();
        } else {
            break;
        }
    }

    let mut all_ops = forward_ops;
    all_ops.extend(backward_ops);
    Some(build_plan_from_ops(&all_ops, task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::bfs::Bfs;
    use crate::task::{CondEffect, Fact, FactId, Task, TaskMeta, TypeHierarchy};
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
            cost: 1,
        }
    }

    fn run_both(task: &Task) -> (Plan, Plan) {
        let mut bibfs = BibfsUc::new();
        let mut bfs = Bfs::new();
        let limits = SearchLimits::default();
        let bibfs_outcome = bibfs.solve(task, &limits).unwrap();
        let bfs_outcome = bfs.solve(task, &limits).unwrap();
        let (bibfs_plan, bfs_plan) = match (bibfs_outcome, bfs_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        (bibfs_plan, bfs_plan)
    }

    #[test]
    fn test_simple_plan() {
        let task = make_test_task(
            4,
            &[0],
            &[3],
            &[],
            vec![
                make_op(0, "op0", &[0], &[], &[1, 2], &[]),
                make_op(1, "op1", &[1, 2], &[], &[3], &[]),
            ],
        );

        let (bibfs_plan, bfs_plan) = run_both(&task);
        assert_eq!(bibfs_plan.len(), bfs_plan.len());
        assert_eq!(bibfs_plan.steps[0].op_id, OpId(0));
        assert_eq!(bibfs_plan.steps[1].op_id, OpId(1));
    }

    #[test]
    fn test_unsolvable() {
        let task = make_test_task(
            3,
            &[0],
            &[2],
            &[],
            vec![make_op(0, "op0", &[0], &[], &[1], &[])],
        );

        let mut bibfs = BibfsUc::new();
        let limits = SearchLimits::default();
        let outcome = bibfs.solve(&task, &limits).unwrap();
        assert!(matches!(outcome, SearchOutcome::Unsolvable(_)));
    }

    #[test]
    fn test_conditional_effect_rejected() {
        let s = |bits: &[usize], size: usize| -> State {
            let mut state = State::new(size);
            for &b in bits {
                state.0.set(b, true);
            }
            state
        };
        let mut op = make_op(0, "op_cond", &[0], &[], &[1], &[]);
        op.conditional.push(CondEffect {
            cond_pos: s(&[0], 3),
            cond_neg: State::new(3),
            add: s(&[2], 3),
            del: State::new(3),
        });

        let task = make_test_task(3, &[0], &[2], &[], vec![op]);

        let mut bibfs = BibfsUc::new();
        let limits = SearchLimits::default();
        let result = bibfs.solve(&task, &limits);
        assert!(matches!(
            result,
            Err(MiniplanError::UnsupportedConditionalEffects)
        ));
    }

    #[test]
    fn test_init_satisfies_goal() {
        let task = make_test_task(
            3,
            &[0, 1],
            &[1],
            &[],
            vec![make_op(0, "op0", &[0], &[], &[1], &[])],
        );

        let mut bibfs = BibfsUc::new();
        let limits = SearchLimits::default();
        let outcome = bibfs.solve(&task, &limits).unwrap();
        match outcome {
            SearchOutcome::Plan(plan, _) => {
                assert!(plan.is_empty());
                assert_eq!(plan.cost, 0.0);
            }
            _ => panic!("expected empty plan"),
        }
    }
}
