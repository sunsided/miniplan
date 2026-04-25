use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::Instant;

use fixedbitset::FixedBitSet;
use rustc_hash::FxHashMap;

use crate::error::MiniplanError;
use crate::heuristic::{rpg_backward_fact_costs, rpg_fact_costs, HFF};
use crate::plan::{Plan, PlanStep};
use crate::search::{
    Heuristic, Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats,
};
use crate::task::{OpId, Operator, State, Task};

type SubgoalKey = (State, State);

#[derive(Clone)]
struct ForwardNodeB {
    b: f64,
    g: f64,
    state: State,
    epoch: u64,
}

impl PartialEq for ForwardNodeB {
    fn eq(&self, other: &Self) -> bool {
        self.b == other.b
    }
}

impl Eq for ForwardNodeB {}

impl PartialOrd for ForwardNodeB {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ForwardNodeB {
    fn cmp(&self, other: &Self) -> Ordering {
        other.b.partial_cmp(&self.b).unwrap_or(Ordering::Equal)
    }
}

#[derive(Clone)]
struct ForwardNodeF {
    f: f64,
    g: f64,
    state: State,
    epoch: u64,
}

impl PartialEq for ForwardNodeF {
    fn eq(&self, other: &Self) -> bool {
        self.f == other.f
    }
}

impl Eq for ForwardNodeF {}

impl PartialOrd for ForwardNodeF {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ForwardNodeF {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}

#[derive(Clone)]
struct ForwardNodeG {
    g: f64,
    state: State,
    epoch: u64,
}

impl PartialEq for ForwardNodeG {
    fn eq(&self, other: &Self) -> bool {
        self.g == other.g
    }
}

impl Eq for ForwardNodeG {}

impl PartialOrd for ForwardNodeG {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ForwardNodeG {
    fn cmp(&self, other: &Self) -> Ordering {
        other.g.partial_cmp(&self.g).unwrap_or(Ordering::Equal)
    }
}

#[derive(Clone)]
struct BackwardNodeB {
    b: f64,
    g: f64,
    subgoal: SubgoalKey,
    epoch: u64,
}

impl PartialEq for BackwardNodeB {
    fn eq(&self, other: &Self) -> bool {
        self.b == other.b
    }
}

impl Eq for BackwardNodeB {}

impl PartialOrd for BackwardNodeB {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BackwardNodeB {
    fn cmp(&self, other: &Self) -> Ordering {
        other.b.partial_cmp(&self.b).unwrap_or(Ordering::Equal)
    }
}

#[derive(Clone)]
struct BackwardNodeF {
    f: f64,
    g: f64,
    subgoal: SubgoalKey,
    epoch: u64,
}

impl PartialEq for BackwardNodeF {
    fn eq(&self, other: &Self) -> bool {
        self.f == other.f
    }
}

impl Eq for BackwardNodeF {}

impl PartialOrd for BackwardNodeF {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BackwardNodeF {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}

#[derive(Clone)]
struct BackwardNodeG {
    g: f64,
    subgoal: SubgoalKey,
    epoch: u64,
}

impl PartialEq for BackwardNodeG {
    fn eq(&self, other: &Self) -> bool {
        self.g == other.g
    }
}

impl Eq for BackwardNodeG {}

impl PartialOrd for BackwardNodeG {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BackwardNodeG {
    fn cmp(&self, other: &Self) -> Ordering {
        other.g.partial_cmp(&self.g).unwrap_or(Ordering::Equal)
    }
}

/// Bidirectional A* with Error (BAE*, Sadhukhan 2013).
///
/// A bidirectional search that uses the B-evaluation function
/// `b = 2g + h_f - h_b` to prioritize expansions, combined with
/// f and g orderings for termination detection. Guarantees optimal
/// plans when using admissible heuristics.
///
/// # Examples
///
/// ```
/// use miniplan::search::{Bae, Planner};
/// use miniplan::heuristic::HFF;
///
/// let planner = Bae::new(Box::new(HFF));
/// assert_eq!(planner.name(), "bae");
///
/// let planner = Bae::with_defaults();
/// ```
pub struct Bae {
    h_forward: Box<dyn Heuristic>,
}

impl Bae {
    /// Create a new BAE* planner with the given forward heuristic.
    pub fn new(h_forward: Box<dyn Heuristic>) -> Self {
        Self { h_forward }
    }

    /// Create a BAE* planner with default heuristic (HFF).
    pub fn with_defaults() -> Self {
        Self::new(Box::new(HFF))
    }
}

fn is_relevant(op: &Operator, g_pos: &State, g_neg: &State) -> bool {
    op.add.0.ones().any(|b| g_pos.0.contains(b)) || op.del.0.ones().any(|b| g_neg.0.contains(b))
}

fn is_consistent(op: &Operator, g_pos: &State, g_neg: &State) -> bool {
    !op.del.0.ones().any(|b| g_pos.0.contains(b)) && !op.add.0.ones().any(|b| g_neg.0.contains(b))
}

fn h_b_from_costs(state: &State, fact_costs: &[f64]) -> f64 {
    let mut max_cost = 0.0;
    for b in state.0.ones() {
        let c = fact_costs[b];
        if c == f64::INFINITY {
            return f64::INFINITY;
        }
        if c > max_cost {
            max_cost = c;
        }
    }
    max_cost
}

fn h_b_sg(sg: &SubgoalKey, fact_costs: &[f64]) -> f64 {
    let (g_pos, _g_neg) = sg;
    let mut max_cost = 0.0;
    for b in g_pos.0.ones() {
        let c = fact_costs[b];
        if c == f64::INFINITY {
            return f64::INFINITY;
        }
        if c > max_cost {
            max_cost = c;
        }
    }
    max_cost
}

fn h_f_sg(sg: &SubgoalKey, back_costs: &[f64]) -> f64 {
    let (g_pos, _g_neg) = sg;
    let mut max_cost = 0.0;
    for b in g_pos.0.ones() {
        let c = back_costs[b];
        if c == f64::INFINITY {
            return f64::INFINITY;
        }
        if c > max_cost {
            max_cost = c;
        }
    }
    max_cost
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
    while let Some((parent, op)) = forward_parent.get(&current) {
        if op.0 == usize::MAX {
            break;
        }
        forward_ops.push(*op);
        current = parent.clone().unwrap();
    }
    forward_ops.reverse();

    let mut backward_ops: Vec<OpId> = Vec::new();
    let mut sg_current = meet_subgoal.clone();
    while let Some((parent, op)) = backward_parent.get(&sg_current) {
        if op.0 == usize::MAX {
            break;
        }
        backward_ops.push(*op);
        sg_current = parent.clone().unwrap();
    }

    let mut all_ops = forward_ops;
    all_ops.extend(backward_ops);
    build_plan_from_ops(&all_ops, task)
}

impl Planner for Bae {
    fn name(&self) -> &str {
        "bae"
    }

    fn describe(&self) -> &str {
        "Bidirectional A* with Error (BAE*, Sadhukhan 2013)"
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

        let fact_costs = rpg_fact_costs(task, &task.init);
        let back_costs = rpg_backward_fact_costs(task, &task.goal_pos, &task.goal_neg);

        let mut forward_entries: Vec<(State, f64, u64)> = Vec::new();
        let mut forward_by_fact: Vec<Vec<u32>> = vec![Vec::new(); num_facts];

        let mut backward_entries: Vec<(SubgoalKey, f64, u64)> = Vec::new();
        let mut backward_by_pos_fact: Vec<Vec<u32>> = vec![Vec::new(); num_facts];
        let mut backward_empty_pos: Vec<u32> = Vec::new();

        let mut scratch_back_seen = FixedBitSet::with_capacity(0);

        let mut forward_open_b: BinaryHeap<ForwardNodeB> = BinaryHeap::new();
        let mut forward_open_f: BinaryHeap<ForwardNodeF> = BinaryHeap::new();
        let mut forward_open_g: BinaryHeap<ForwardNodeG> = BinaryHeap::new();
        let mut forward_g: FxHashMap<State, f64> = FxHashMap::default();
        let mut forward_h: FxHashMap<State, f64> = FxHashMap::default();
        let mut forward_hb: FxHashMap<State, f64> = FxHashMap::default();
        let mut forward_parent: FxHashMap<State, (Option<State>, OpId)> = FxHashMap::default();
        let mut forward_epoch: FxHashMap<State, u64> = FxHashMap::default();
        let mut forward_epoch_counter: u64 = 0;

        let init_hf = self.h_forward.estimate(task, &task.init).0;
        let init_hb = h_b_from_costs(&task.init, &fact_costs);
        let init_f = 0.0 + init_hf;
        let init_b = 2.0 * 0.0 + init_hf - init_hb;
        forward_open_b.push(ForwardNodeB {
            b: init_b,
            g: 0.0,
            state: task.init.clone(),
            epoch: 0,
        });
        forward_open_f.push(ForwardNodeF {
            f: init_f,
            g: 0.0,
            state: task.init.clone(),
            epoch: 0,
        });
        forward_open_g.push(ForwardNodeG {
            g: 0.0,
            state: task.init.clone(),
            epoch: 0,
        });
        forward_g.insert(task.init.clone(), 0.0);
        forward_h.insert(task.init.clone(), init_hf);
        forward_hb.insert(task.init.clone(), init_hb);
        forward_parent.insert(task.init.clone(), (None, OpId(usize::MAX)));
        forward_epoch.insert(task.init.clone(), 0);

        let init_subgoal = (task.goal_pos.clone(), task.goal_neg.clone());
        let mut backward_open_b: BinaryHeap<BackwardNodeB> = BinaryHeap::new();
        let mut backward_open_f: BinaryHeap<BackwardNodeF> = BinaryHeap::new();
        let mut backward_open_g: BinaryHeap<BackwardNodeG> = BinaryHeap::new();
        let mut backward_g: FxHashMap<SubgoalKey, f64> = FxHashMap::default();
        let mut backward_h: FxHashMap<SubgoalKey, f64> = FxHashMap::default();
        let mut backward_hf: FxHashMap<SubgoalKey, f64> = FxHashMap::default();
        let mut backward_parent: FxHashMap<SubgoalKey, (Option<SubgoalKey>, OpId)> =
            FxHashMap::default();
        let mut backward_epoch: FxHashMap<SubgoalKey, u64> = FxHashMap::default();
        let mut backward_epoch_counter: u64 = 0;

        let sg_h_b = h_b_sg(&init_subgoal, &fact_costs);
        let sg_h_f = h_f_sg(&init_subgoal, &back_costs);
        if sg_h_b != f64::INFINITY && sg_h_f != f64::INFINITY {
            let sg_f = 0.0 + sg_h_b;
            let sg_b = 2.0 * 0.0 + sg_h_b - sg_h_f;
            backward_open_b.push(BackwardNodeB {
                b: sg_b,
                g: 0.0,
                subgoal: init_subgoal.clone(),
                epoch: 0,
            });
            backward_open_f.push(BackwardNodeF {
                f: sg_f,
                g: 0.0,
                subgoal: init_subgoal.clone(),
                epoch: 0,
            });
            backward_open_g.push(BackwardNodeG {
                g: 0.0,
                subgoal: init_subgoal.clone(),
                epoch: 0,
            });
            backward_g.insert(init_subgoal.clone(), 0.0);
            backward_h.insert(init_subgoal.clone(), sg_h_b);
            backward_hf.insert(init_subgoal.clone(), sg_h_f);
            backward_parent.insert(init_subgoal.clone(), (None, OpId(usize::MAX)));
            backward_epoch.insert(init_subgoal, 0);
        }

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

            let pr_f_min = forward_open_b.peek().map(|n| n.b);
            let pr_b_min = backward_open_b.peek().map(|n| n.b);
            let f_min_f = forward_open_f.peek().map(|n| n.f);
            let f_min_b = backward_open_f.peek().map(|n| n.f);
            let g_min_f = forward_open_g.peek().map(|n| n.g);
            let g_min_b = backward_open_g.peek().map(|n| n.g);

            let gm_bound = match (f_min_f, f_min_b, g_min_f, g_min_b) {
                (Some(ff), Some(fb), Some(gf), Some(gb)) => ff.max(fb).max(gf + gb),
                (Some(ff), None, Some(gf), _) => ff.max(gf),
                (None, Some(fb), _, Some(gb)) => fb.max(gb),
                _ => f64::INFINITY,
            };

            let c_lb = match (pr_f_min, pr_b_min) {
                (Some(pf), Some(pb)) => gm_bound.max((pf + pb) * 0.5),
                _ => gm_bound,
            };

            if let Some((_, _, meet_cost)) = best_meet
                && c_lb >= meet_cost
            {
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

            let forward_empty =
                forward_open_b.is_empty() || forward_open_f.is_empty() || forward_open_g.is_empty();
            let backward_empty = backward_open_b.is_empty()
                || backward_open_f.is_empty()
                || backward_open_g.is_empty();

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

            let expand_forward = match (pr_f_min, pr_b_min) {
                (None, _) => false,
                (_, None) => true,
                (Some(pf), Some(pb)) => pf <= pb,
            };

            if expand_forward {
                let node_b = loop {
                    match forward_open_b.pop() {
                        Some(n) => {
                            let g = *forward_g.get(&n.state).unwrap_or(&f64::INFINITY);
                            let epoch = *forward_epoch.get(&n.state).unwrap_or(&0);
                            if n.g == g && n.epoch == epoch {
                                break Some(n);
                            }
                        }
                        None => break None,
                    }
                };
                let Some(node_b) = node_b else {
                    continue;
                };
                let state = node_b.state.clone();
                let g = node_b.g;

                let node_f = loop {
                    match forward_open_f.pop() {
                        Some(n) => {
                            let gg = *forward_g.get(&n.state).unwrap_or(&f64::INFINITY);
                            let epoch = *forward_epoch.get(&n.state).unwrap_or(&0);
                            if n.g == gg && n.epoch == epoch {
                                break Some(n);
                            }
                        }
                        None => break None,
                    }
                };
                let Some(_node_f) = node_f else {
                    continue;
                };

                let node_g = loop {
                    match forward_open_g.pop() {
                        Some(n) => {
                            let gg = *forward_g.get(&n.state).unwrap_or(&f64::INFINITY);
                            let epoch = *forward_epoch.get(&n.state).unwrap_or(&0);
                            if n.g == gg && n.epoch == epoch {
                                break Some(n);
                            }
                        }
                        None => break None,
                    }
                };
                let Some(_node_g) = node_g else {
                    continue;
                };

                stats.nodes_expanded += 1;

                let fwd_idx = forward_entries.len() as u32;
                forward_entries.push((state.clone(), g, forward_epoch_counter));
                for f in state.0.ones() {
                    forward_by_fact[f].push(fwd_idx);
                }

                scratch_back_seen.grow(backward_entries.len());
                scratch_back_seen.clear();

                for f in state.0.ones() {
                    for &idx in &backward_by_pos_fact[f] {
                        if !scratch_back_seen.put(idx as usize) {
                            let (sg, sg_g, _) = &backward_entries[idx as usize];
                            if state.satisfies(&sg.0, &sg.1) {
                                let total = g + sg_g;
                                if best_meet.as_ref().is_none_or(|(_, _, c)| total < *c) {
                                    best_meet = Some((state.clone(), sg.clone(), total));
                                }
                            }
                        }
                    }
                }
                for &idx in &backward_empty_pos {
                    if !scratch_back_seen.put(idx as usize) {
                        let (sg, sg_g, _) = &backward_entries[idx as usize];
                        if state.satisfies(&sg.0, &sg.1) {
                            let total = g + sg_g;
                            if best_meet.as_ref().is_none_or(|(_, _, c)| total < *c) {
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
                            forward_epoch_counter += 1;
                            let ep = forward_epoch_counter;
                            forward_g.insert(next.clone(), new_g);
                            let hf = self.h_forward.estimate(task, &next).0;
                            let hb = h_b_from_costs(&next, &fact_costs);
                            let f = new_g + hf;
                            let b = 2.0 * new_g + hf - hb;
                            forward_h.insert(next.clone(), hf);
                            forward_hb.insert(next.clone(), hb);
                            forward_parent.insert(next.clone(), (Some(state.clone()), op.id));
                            forward_epoch.insert(next.clone(), ep);
                            stats.nodes_generated += 1;
                            forward_open_b.push(ForwardNodeB {
                                b,
                                g: new_g,
                                state: next.clone(),
                                epoch: ep,
                            });
                            forward_open_f.push(ForwardNodeF {
                                f,
                                g: new_g,
                                state: next.clone(),
                                epoch: ep,
                            });
                            forward_open_g.push(ForwardNodeG {
                                g: new_g,
                                state: next,
                                epoch: ep,
                            });
                        }
                    }
                }
            } else {
                let node_b = loop {
                    match backward_open_b.pop() {
                        Some(n) => {
                            let g = *backward_g.get(&n.subgoal).unwrap_or(&f64::INFINITY);
                            let epoch = *backward_epoch.get(&n.subgoal).unwrap_or(&0);
                            if n.g == g && n.epoch == epoch {
                                break Some(n);
                            }
                        }
                        None => break None,
                    }
                };
                let Some(node_b) = node_b else {
                    continue;
                };
                let _subgoal = node_b.subgoal.clone();
                let g = node_b.g;

                let node_f = loop {
                    match backward_open_f.pop() {
                        Some(n) => {
                            let gg = *backward_g.get(&n.subgoal).unwrap_or(&f64::INFINITY);
                            let epoch = *backward_epoch.get(&n.subgoal).unwrap_or(&0);
                            if n.g == gg && n.epoch == epoch {
                                break Some(n);
                            }
                        }
                        None => break None,
                    }
                };
                let Some(_node_f) = node_f else {
                    continue;
                };

                let node_g = loop {
                    match backward_open_g.pop() {
                        Some(n) => {
                            let gg = *backward_g.get(&n.subgoal).unwrap_or(&f64::INFINITY);
                            let epoch = *backward_epoch.get(&n.subgoal).unwrap_or(&0);
                            if n.g == gg && n.epoch == epoch {
                                break Some(n);
                            }
                        }
                        None => break None,
                    }
                };
                let Some(_node_g) = node_g else {
                    continue;
                };

                stats.nodes_expanded += 1;

                let (g_pos, g_neg) = node_b.subgoal.clone();

                let bwd_idx = backward_entries.len() as u32;
                backward_entries.push(((g_pos.clone(), g_neg.clone()), g, backward_epoch_counter));
                let g_pos_ones: Vec<_> = g_pos.0.ones().collect();
                if g_pos_ones.is_empty() {
                    backward_empty_pos.push(bwd_idx);
                } else {
                    for &f in &g_pos_ones {
                        backward_by_pos_fact[f].push(bwd_idx);
                    }
                }

                if g_pos_ones.is_empty() {
                    for (f_state, f_g, _) in &forward_entries {
                        if f_state.satisfies(&g_pos, &g_neg) {
                            let total = f_g + g;
                            if best_meet.as_ref().is_none_or(|(_, _, c)| total < *c) {
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
                        let (f_state, f_g, _) = &forward_entries[idx as usize];
                        if f_state.satisfies(&g_pos, &g_neg) {
                            let total = f_g + g;
                            if best_meet.as_ref().is_none_or(|(_, _, c)| total < *c) {
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
                            let new_h_b = h_b_sg(&new_sg, &fact_costs);
                            if new_h_b == f64::INFINITY {
                                continue;
                            }
                            let new_h_f = h_f_sg(&new_sg, &back_costs);
                            if new_h_f == f64::INFINITY {
                                continue;
                            }
                            backward_epoch_counter += 1;
                            let ep = backward_epoch_counter;
                            backward_g.insert(new_sg.clone(), new_g);
                            backward_h.insert(new_sg.clone(), new_h_b);
                            backward_hf.insert(new_sg.clone(), new_h_f);
                            backward_parent.insert(
                                new_sg.clone(),
                                (Some((g_pos.clone(), g_neg.clone())), op.id),
                            );
                            backward_epoch.insert(new_sg.clone(), ep);
                            stats.nodes_generated += 1;
                            let f = new_g + new_h_b;
                            let b = 2.0 * new_g + new_h_b - new_h_f;
                            backward_open_b.push(BackwardNodeB {
                                b,
                                g: new_g,
                                subgoal: new_sg.clone(),
                                epoch: ep,
                            });
                            backward_open_f.push(BackwardNodeF {
                                f,
                                g: new_g,
                                subgoal: new_sg.clone(),
                                epoch: ep,
                            });
                            backward_open_g.push(BackwardNodeG {
                                g: new_g,
                                subgoal: new_sg,
                                epoch: ep,
                            });
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heuristic::HFF;
    use crate::search::{Astar, BibfsUc, BiDij, Nbs};
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

        let mut bae = Bae::with_defaults();
        let mut bibfs_uc = BibfsUc::new();
        let limits = SearchLimits::default();
        let bae_outcome = bae.solve(&task, &limits).unwrap();
        let uc_outcome = bibfs_uc.solve(&task, &limits).unwrap();
        let (bae_plan, uc_plan) = match (bae_outcome, uc_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        assert_eq!(bae_plan.len(), uc_plan.len());
        assert_eq!(bae_plan.steps[0].op_id, OpId(0));
        assert_eq!(bae_plan.steps[1].op_id, OpId(1));
    }

    #[test]
    fn test_cost_aware_prefers_cheaper_path() {
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

        let mut bae = Bae::with_defaults();
        let limits = SearchLimits::default();
        let outcome = bae.solve(&task, &limits).unwrap();
        match outcome {
            SearchOutcome::Plan(plan, _) => {
                assert_eq!(plan.cost, 2.0);
                assert_eq!(plan.len(), 2);
                assert_eq!(plan.steps[0].op_id, OpId(1));
                assert_eq!(plan.steps[1].op_id, OpId(3));
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

        let mut bae = Bae::with_defaults();
        let limits = SearchLimits::default();
        let outcome = bae.solve(&task, &limits).unwrap();
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

        let mut bae = Bae::with_defaults();
        let limits = SearchLimits::default();
        let result = bae.solve(&task, &limits);
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

        let mut bae = Bae::with_defaults();
        let limits = SearchLimits::default();
        let outcome = bae.solve(&task, &limits).unwrap();
        match outcome {
            SearchOutcome::Plan(plan, _) => {
                assert!(plan.is_empty());
                assert_eq!(plan.cost, 0.0);
            }
            _ => panic!("expected empty plan"),
        }
    }

    #[test]
    fn test_matches_bidij_cost() {
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

        let mut bae = Bae::with_defaults();
        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let bae_outcome = bae.solve(&task, &limits).unwrap();
        let bidij_outcome = bidij.solve(&task, &limits).unwrap();

        let (bae_plan, bidij_plan) = match (bae_outcome, bidij_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        assert_eq!(bae_plan.cost, bidij_plan.cost);
        assert_eq!(bae_plan.cost, 2.0);
    }

    #[test]
    fn test_matches_astar_cost() {
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

        let mut bae = Bae::with_defaults();
        let mut astar = Astar::new(Box::new(HFF));
        let limits = SearchLimits::default();
        let bae_outcome = bae.solve(&task, &limits).unwrap();
        let astar_outcome = astar.solve(&task, &limits).unwrap();

        let (bae_plan, astar_plan) = match (bae_outcome, astar_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        assert_eq!(bae_plan.cost, astar_plan.cost);
        assert_eq!(bae_plan.cost, 2.0);
    }

    #[test]
    fn test_matches_nbs_cost() {
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

        let mut bae = Bae::with_defaults();
        let mut nbs = Nbs::with_defaults();
        let limits = SearchLimits::default();
        let bae_outcome = bae.solve(&task, &limits).unwrap();
        let nbs_outcome = nbs.solve(&task, &limits).unwrap();

        let (bae_plan, nbs_plan) = match (bae_outcome, nbs_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        assert_eq!(bae_plan.cost, nbs_plan.cost);
        assert_eq!(bae_plan.cost, 2.0);
    }

    #[test]
    fn test_bucket_index_correctness() {
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

        let mut bae = Bae::with_defaults();
        let mut astar = Astar::new(Box::new(HFF));
        let limits = SearchLimits::default();
        let bae_outcome = bae.solve(&task, &limits).unwrap();
        let astar_outcome = astar.solve(&task, &limits).unwrap();

        let (bae_plan, astar_plan) = match (bae_outcome, astar_outcome) {
            (SearchOutcome::Plan(a, _), SearchOutcome::Plan(b, _)) => (a, b),
            _ => panic!("expected plans from both planners"),
        };
        assert_eq!(bae_plan.cost, astar_plan.cost);
        assert_eq!(bae_plan.cost, 3.0);
        assert_eq!(bae_plan.len(), 3);
    }

    #[test]
    fn test_hb_prunes_unreachable_subgoal() {
        let task = make_test_task(
            4,
            &[0],
            &[3],
            &[],
            vec![
                make_op(0, "op0", &[0], &[], &[1], &[], 1),
                make_op(1, "op1", &[1], &[], &[3], &[], 1),
                make_op(2, "op2", &[2], &[], &[3], &[], 1),
            ],
        );

        let mut bae = Bae::with_defaults();
        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let bae_outcome = bae.solve(&task, &limits).unwrap();
        let bidij_outcome = bidij.solve(&task, &limits).unwrap();

        let (bae_plan, bae_stats) = match bae_outcome {
            SearchOutcome::Plan(plan, stats) => (plan, stats),
            _ => panic!("expected a plan"),
        };
        let (bidij_plan, bidij_stats) = match bidij_outcome {
            SearchOutcome::Plan(plan, stats) => (plan, stats),
            _ => panic!("expected a plan"),
        };

        assert_eq!(bae_plan.cost, bidij_plan.cost);
        assert!(bae_stats.nodes_expanded <= bidij_stats.nodes_expanded);
    }

    #[test]
    fn test_error_term_reduces_expansions() {
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

        let mut bae = Bae::with_defaults();
        let mut nbs = Nbs::with_defaults();
        let limits = SearchLimits::default();
        let bae_outcome = bae.solve(&task, &limits).unwrap();
        let nbs_outcome = nbs.solve(&task, &limits).unwrap();

        let (bae_plan, bae_stats) = match bae_outcome {
            SearchOutcome::Plan(plan, stats) => (plan, stats),
            _ => panic!("expected a plan"),
        };
        let (nbs_plan, nbs_stats) = match nbs_outcome {
            SearchOutcome::Plan(plan, stats) => (plan, stats),
            _ => panic!("expected a plan"),
        };

        assert_eq!(bae_plan.cost, nbs_plan.cost);
        assert!(
            bae_stats.nodes_expanded <= nbs_stats.nodes_expanded * 2,
            "BAE* expansions ({}) should be within 2x of NBS ({})",
            bae_stats.nodes_expanded,
            nbs_stats.nodes_expanded
        );
    }
}
