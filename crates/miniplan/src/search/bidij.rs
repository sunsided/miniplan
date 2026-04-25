// TODO(shared): factor regression helpers (is_relevant, is_consistent,
//               regress_subgoal) into search/regression.rs so all three
//               bidirectional planners can reuse them.
// TODO(perf): per-FactId bucket index over closed_other to avoid O(|closed|)
//             meet scans (inherited from bibfs-uc).

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::Instant;

use fixedbitset::FixedBitSet;
use rustc_hash::FxHashMap;

use crate::error::MiniplanError;
use crate::plan::{Plan, PlanStep};
use crate::search::{Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats};
use crate::task::{OpId, Operator, State, Task};

type SubgoalKey = (State, State);

#[derive(Clone)]
struct ForwardNode {
    g: f64,
    state: State,
}

impl PartialEq for ForwardNode {
    fn eq(&self, other: &Self) -> bool {
        self.g == other.g
    }
}

impl Eq for ForwardNode {}

impl PartialOrd for ForwardNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ForwardNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.g.partial_cmp(&self.g).unwrap_or(Ordering::Equal)
    }
}

#[derive(Clone)]
struct BackwardNode {
    g: f64,
    subgoal: SubgoalKey,
}

impl PartialEq for BackwardNode {
    fn eq(&self, other: &Self) -> bool {
        self.g == other.g
    }
}

impl Eq for BackwardNode {}

impl PartialOrd for BackwardNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BackwardNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.g.partial_cmp(&self.g).unwrap_or(Ordering::Equal)
    }
}

#[derive(Default)]
pub struct BiDij {}

impl BiDij {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Planner for BiDij {
    fn name(&self) -> &str {
        "bidij"
    }

    fn describe(&self) -> &str {
        "Bidirectional Dijkstra (cost-aware)"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::ACTION_COSTS
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

        // Forward arena + index
        let mut forward_entries: Vec<(State, f64)> = Vec::new();
        let mut forward_by_fact: Vec<Vec<u32>> = vec![Vec::new(); num_facts];

        // Backward arena + index
        let mut backward_entries: Vec<(SubgoalKey, f64)> = Vec::new();
        let mut backward_by_pos_fact: Vec<Vec<u32>> = vec![Vec::new(); num_facts];
        let mut backward_empty_pos: Vec<u32> = Vec::new();

        // Scratch dedup bitset for forward meet scan
        let mut scratch_back_seen = FixedBitSet::with_capacity(0);

        let mut forward_open: BinaryHeap<ForwardNode> = BinaryHeap::new();
        let mut forward_g: FxHashMap<State, f64> = FxHashMap::default();
        let mut forward_parent: FxHashMap<State, (Option<State>, OpId)> = FxHashMap::default();

        forward_open.push(ForwardNode {
            g: 0.0,
            state: task.init.clone(),
        });
        forward_g.insert(task.init.clone(), 0.0);
        forward_parent.insert(task.init.clone(), (None, OpId(usize::MAX)));

        let init_subgoal = (task.goal_pos.clone(), task.goal_neg.clone());
        let mut backward_open: BinaryHeap<BackwardNode> = BinaryHeap::new();
        let mut backward_g: FxHashMap<SubgoalKey, f64> = FxHashMap::default();
        let mut backward_parent: FxHashMap<SubgoalKey, (Option<SubgoalKey>, OpId)> =
            FxHashMap::default();

        backward_open.push(BackwardNode {
            g: 0.0,
            subgoal: init_subgoal.clone(),
        });
        backward_g.insert(init_subgoal.clone(), 0.0);
        backward_parent.insert(init_subgoal, (None, OpId(usize::MAX)));

        let mut best_meet: Option<(State, SubgoalKey, f64)> = None;

        loop {
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

            let forward_top_g = forward_open.peek().map(|n| n.g);
            let backward_top_g = backward_open.peek().map(|n| n.g);

            // Termination check
            if let (Some(fg), Some(bg)) = (forward_top_g, backward_top_g) {
                if let Some((_, _, meet_cost)) = best_meet {
                    if fg + bg >= meet_cost {
                        stats.elapsed = start.elapsed();
                        let plan = reconstruct_plan_from_meet(
                            &best_meet.unwrap(),
                            &forward_parent,
                            &backward_parent,
                            task,
                        );
                        stats.plan_cost = plan.cost;
                        stats.plan_length = plan.len();
                        return Ok(SearchOutcome::Plan(plan, stats));
                    }
                }
            }

            let forward_empty = forward_open.is_empty();
            let backward_empty = backward_open.is_empty();

            if forward_empty && backward_empty {
                stats.elapsed = start.elapsed();
                if let Some((_, _, _)) = best_meet {
                    let plan = reconstruct_plan_from_meet(
                        &best_meet.unwrap(),
                        &forward_parent,
                        &backward_parent,
                        task,
                    );
                    stats.plan_cost = plan.cost;
                    stats.plan_length = plan.len();
                    return Ok(SearchOutcome::Plan(plan, stats));
                }
                return Ok(SearchOutcome::Unsolvable(stats));
            }

            // Pick side to expand: smaller g wins; if one empty, expand other
            let expand_forward = match (forward_top_g, backward_top_g) {
                (None, _) => false,
                (_, None) => true,
                (Some(fg), Some(bg)) => fg <= bg,
            };

            if expand_forward {
                let node = forward_open.pop().unwrap();
                let state = node.state.clone();
                if node.g > *forward_g.get(&state).unwrap_or(&f64::INFINITY) {
                    continue;
                }
                stats.nodes_expanded += 1;

                let g = node.g;

                // Append to forward arena + index
                let fwd_idx = forward_entries.len() as u32;
                forward_entries.push((state.clone(), g));
                for f in state.0.ones() {
                    forward_by_fact[f].push(fwd_idx);
                }

                // Grow scratch bitset to current backward arena size
                scratch_back_seen.grow(backward_entries.len());
                scratch_back_seen.clear();

                // Check meet with backward arena via per-fact index
                for f in state.0.ones() {
                    for &idx in &backward_by_pos_fact[f] {
                        if !scratch_back_seen.put(idx as usize) {
                            let (sg, sg_g) = &backward_entries[idx as usize];
                            if state.satisfies(&sg.0, &sg.1) {
                                let total = g + sg_g;
                                if best_meet.as_ref().map_or(true, |(_, _, c)| total < *c) {
                                    best_meet = Some((state.clone(), sg.clone(), total));
                                }
                            }
                        }
                    }
                }
                for &idx in &backward_empty_pos {
                    if !scratch_back_seen.put(idx as usize) {
                        let (sg, sg_g) = &backward_entries[idx as usize];
                        if state.satisfies(&sg.0, &sg.1) {
                            let total = g + sg_g;
                            if best_meet.as_ref().map_or(true, |(_, _, c)| total < *c) {
                                best_meet = Some((state.clone(), sg.clone(), total));
                            }
                        }
                    }
                }

                for op in &task.operators {
                    if state.applicable(op) {
                        let next = state.apply(op);
                        let new_g = g + op.cost as f64;
                        if new_g < *forward_g.get(&next).unwrap_or(&f64::INFINITY) {
                            forward_g.insert(next.clone(), new_g);
                            forward_parent.insert(next.clone(), (Some(state.clone()), op.id));
                            stats.nodes_generated += 1;
                            forward_open.push(ForwardNode {
                                g: new_g,
                                state: next,
                            });
                        }
                    }
                }
            } else {
                let node = backward_open.pop().unwrap();
                let subgoal = node.subgoal.clone();
                if node.g > *backward_g.get(&subgoal).unwrap_or(&f64::INFINITY) {
                    continue;
                }
                stats.nodes_expanded += 1;

                let g = node.g;
                let (g_pos, g_neg) = node.subgoal;

                // Append to backward arena + index
                let bwd_idx = backward_entries.len() as u32;
                backward_entries.push(((g_pos.clone(), g_neg.clone()), g));
                let g_pos_ones: Vec<_> = g_pos.0.ones().collect();
                if g_pos_ones.is_empty() {
                    backward_empty_pos.push(bwd_idx);
                } else {
                    for &f in &g_pos_ones {
                        backward_by_pos_fact[f].push(bwd_idx);
                    }
                }

                // Check meet with forward arena via per-fact index
                if g_pos_ones.is_empty() {
                    for (f_state, f_g) in &forward_entries {
                        if f_state.satisfies(&g_pos, &g_neg) {
                            let total = f_g + g;
                            if best_meet.as_ref().map_or(true, |(_, _, c)| total < *c) {
                                best_meet =
                                    Some((f_state.clone(), (g_pos.clone(), g_neg.clone()), total));
                            }
                        }
                    }
                } else {
                    let rarest = g_pos_ones
                        .iter()
                        .min_by_key(|&&f| forward_by_fact[f].len())
                        .copied()
                        .unwrap();
                    for &idx in &forward_by_fact[rarest] {
                        let (f_state, f_g) = &forward_entries[idx as usize];
                        if f_state.satisfies(&g_pos, &g_neg) {
                            let total = f_g + g;
                            if best_meet.as_ref().map_or(true, |(_, _, c)| total < *c) {
                                best_meet =
                                    Some((f_state.clone(), (g_pos.clone(), g_neg.clone()), total));
                            }
                        }
                    }
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
                        let new_g = g + op.cost as f64;
                        if new_g < *backward_g.get(&new_sg).unwrap_or(&f64::INFINITY) {
                            backward_g.insert(new_sg.clone(), new_g);
                            backward_parent.insert(
                                new_sg.clone(),
                                (Some((g_pos.clone(), g_neg.clone())), op.id),
                            );
                            stats.nodes_generated += 1;
                            backward_open.push(BackwardNode {
                                g: new_g,
                                subgoal: new_sg,
                            });
                        }
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

fn reconstruct_plan_from_meet(
    meet: &(State, SubgoalKey, f64),
    forward_parent: &FxHashMap<State, (Option<State>, OpId)>,
    backward_parent: &FxHashMap<SubgoalKey, (Option<SubgoalKey>, OpId)>,
    task: &Task,
) -> Plan {
    let (meet_state, meet_subgoal, _total) = meet;

    let mut forward_ops: Vec<OpId> = Vec::new();
    let mut current = meet_state.clone();
    loop {
        if let Some((parent, op)) = forward_parent.get(&current) {
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
        if let Some((parent, op)) = backward_parent.get(&sg_current) {
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
    build_plan_from_ops(&all_ops, task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heuristic::relaxed::HFF;
    use crate::search::astar::Astar;
    use crate::search::bibfs_uc::BibfsUc;
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
    fn test_simple_plan_uniform_cost() {
        let task = make_test_task(
            4,
            &[0],
            &[3],
            &[],
            vec![
                make_op(0, "op0", &[0], &[], &[1, 2], &[], 1),
                make_op(1, "op1", &[1, 2], &[], &[3], &[], 1),
            ],
        );

        let mut bidij = BiDij::new();
        let mut bibfs_uc = BibfsUc::new();
        let limits = SearchLimits::default();
        let bidij_outcome = bidij.solve(&task, &limits).unwrap();
        let uc_outcome = bibfs_uc.solve(&task, &limits).unwrap();
        let (bidij_plan, uc_plan) = match (bidij_outcome, uc_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        assert_eq!(bidij_plan.len(), uc_plan.len());
        assert_eq!(bidij_plan.steps[0].op_id, OpId(0));
        assert_eq!(bidij_plan.steps[1].op_id, OpId(1));
    }

    #[test]
    fn test_cost_aware_prefers_cheaper_path() {
        // Diamond:
        // init = {a=0}, goal = {d=3}
        // op1: a -> b, cost 10  (0 -> 1)
        // op2: a -> c, cost 1   (0 -> 2)
        // op3: b -> d, cost 1   (1 -> 3)
        // op4: c -> d, cost 1   (2 -> 3)
        // Cheaper path: a -> c -> d, cost 2
        // Expensive path: a -> b -> d, cost 11
        let task = make_test_task(
            4,
            &[0],
            &[3],
            &[],
            vec![
                make_op(0, "a_to_b", &[0], &[], &[1], &[], 10),
                make_op(1, "a_to_c", &[0], &[], &[2], &[], 1),
                make_op(2, "b_to_d", &[1], &[], &[3], &[], 1),
                make_op(3, "c_to_d", &[2], &[], &[3], &[], 1),
            ],
        );

        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let outcome = bidij.solve(&task, &limits).unwrap();
        match outcome {
            SearchOutcome::Plan(plan, _) => {
                assert_eq!(plan.cost, 2.0);
                assert_eq!(plan.len(), 2);
                assert_eq!(plan.steps[0].op_id, OpId(1)); // a_to_c
                assert_eq!(plan.steps[1].op_id, OpId(3)); // c_to_d
            }
            _ => panic!("expected a plan"),
        }
    }

    #[test]
    fn test_unsolvable() {
        let task = make_test_task(
            3,
            &[0],
            &[2],
            &[],
            vec![make_op(0, "op0", &[0], &[], &[1], &[], 1)],
        );

        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let outcome = bidij.solve(&task, &limits).unwrap();
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
        let mut op = make_op(0, "op_cond", &[0], &[], &[1], &[], 1);
        op.conditional.push(CondEffect {
            cond_pos: s(&[0], 3),
            cond_neg: State::new(3),
            add: s(&[2], 3),
            del: State::new(3),
        });

        let task = make_test_task(3, &[0], &[2], &[], vec![op]);

        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let result = bidij.solve(&task, &limits);
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
            vec![make_op(0, "op0", &[0], &[], &[1], &[], 1)],
        );

        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let outcome = bidij.solve(&task, &limits).unwrap();
        match outcome {
            SearchOutcome::Plan(plan, _) => {
                assert!(plan.is_empty());
                assert_eq!(plan.cost, 0.0);
            }
            _ => panic!("expected empty plan"),
        }
    }

    #[test]
    fn test_matches_astar_cost() {
        // Non-uniform cost diamond to cross-check optimality against A*+HFF
        let task = make_test_task(
            4,
            &[0],
            &[3],
            &[],
            vec![
                make_op(0, "a_to_b", &[0], &[], &[1], &[], 10),
                make_op(1, "a_to_c", &[0], &[], &[2], &[], 1),
                make_op(2, "b_to_d", &[1], &[], &[3], &[], 1),
                make_op(3, "c_to_d", &[2], &[], &[3], &[], 1),
            ],
        );

        let mut bidij = BiDij::new();
        let mut astar = Astar::new(Box::new(HFF));
        let limits = SearchLimits::default();
        let bidij_outcome = bidij.solve(&task, &limits).unwrap();
        let astar_outcome = astar.solve(&task, &limits).unwrap();

        let (bidij_plan, astar_plan) = match (bidij_outcome, astar_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        assert_eq!(bidij_plan.cost, astar_plan.cost);
        assert_eq!(bidij_plan.cost, 2.0);
    }

    #[test]
    fn test_stale_pops_skipped() {
        // Two paths to state {1}: op0 (cost 3) and op1 (cost 1).
        // After {1} is settled at g=1, the stale entry at g=3 should be skipped.
        // Chain: 0 -> 1 -> 2 -> 3 -> 4
        // With arena-based meet scan: nodes_expanded = 6 (meet detected one step later
        // than HashMap-based scan since backward subgoals must be canonical first).
        // Without stale guard: would be 7 (stale {1}@g3 would be "expanded").
        let task = make_test_task(
            5,
            &[0],
            &[4],
            &[],
            vec![
                make_op(0, "op0_0to1_exp", &[0], &[], &[1], &[], 3),
                make_op(1, "op1_0to1_cheap", &[0], &[], &[1], &[], 1),
                make_op(2, "op2_1to2", &[1], &[], &[2], &[], 1),
                make_op(3, "op3_2to3", &[2], &[], &[3], &[], 1),
                make_op(4, "op4_3to4", &[3], &[], &[4], &[], 1),
            ],
        );

        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let outcome = bidij.solve(&task, &limits).unwrap();
        match outcome {
            SearchOutcome::Plan(plan, stats) => {
                assert_eq!(plan.cost, 4.0);
                assert_eq!(stats.nodes_expanded, 6);
            }
            _ => panic!("expected a plan"),
        }
    }

    #[test]
    fn test_bucket_index_correctness() {
        // Task requiring multi-fact g_pos meet via bucket index:
        // init = {0}, goal = {3, 4}
        // op0: {0} -> {1, 2}, cost 1
        // op1: {1, 2} -> {3}, cost 1
        // op2: {1, 2} -> {4}, cost 1
        // Optimal: op0, op1, op2 (cost 3) — forward state {0,1,2,3,4} meets backward subgoal ({3,4}, {})
        // The backward subgoal ({3,4}, {}) has g_pos with 2 facts; bucket index must find it.
        let task = make_test_task(
            5,
            &[0],
            &[3, 4],
            &[],
            vec![
                make_op(0, "op0", &[0], &[], &[1, 2], &[], 1),
                make_op(1, "op1", &[1, 2], &[], &[3], &[], 1),
                make_op(2, "op2", &[1, 2], &[], &[4], &[], 1),
            ],
        );

        let mut bidij = BiDij::new();
        let mut astar = Astar::new(Box::new(HFF));
        let limits = SearchLimits::default();
        let bidij_outcome = bidij.solve(&task, &limits).unwrap();
        let astar_outcome = astar.solve(&task, &limits).unwrap();

        let (bidij_plan, astar_plan) = match (bidij_outcome, astar_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        assert_eq!(bidij_plan.cost, astar_plan.cost);
        assert_eq!(bidij_plan.cost, 3.0);
        assert_eq!(bidij_plan.len(), 3);
    }
}
