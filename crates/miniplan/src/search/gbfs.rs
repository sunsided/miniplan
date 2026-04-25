use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::Instant;

use crate::error::MiniplanError;
use crate::plan::{Plan, PlanStep};
use crate::search::Heuristic;
use crate::search::{
    HValue, Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats,
};
use crate::task::{OpId, State, Task};

#[derive(Debug, Clone)]
struct GbfsNode {
    h_score: HValue,
    state: State,
    path: Vec<OpId>,
    g_score: f64,
}

impl PartialEq for GbfsNode {
    fn eq(&self, other: &Self) -> bool {
        self.h_score == other.h_score
    }
}

impl Eq for GbfsNode {}

impl PartialOrd for GbfsNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GbfsNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .h_score
            .0
            .partial_cmp(&self.h_score.0)
            .unwrap_or(Ordering::Equal)
    }
}

/// Greedy best-first search with a pluggable heuristic.
///
/// Expands the node with the lowest heuristic value `h`, ignoring path cost.
/// Fast but does not guarantee optimality.
///
/// # Examples
///
/// ```
/// use miniplan::search::{Gbfs, Planner};
/// use miniplan::heuristic::HFF;
///
/// let planner = Gbfs::new(Box::new(HFF));
/// assert_eq!(planner.name(), "gbfs");
/// ```
pub struct Gbfs {
    heuristic: Box<dyn Heuristic>,
}

impl Gbfs {
    /// Create a new GBFS planner with the given heuristic.
    pub fn new(heuristic: Box<dyn Heuristic>) -> Self {
        Gbfs { heuristic }
    }
}

impl Planner for Gbfs {
    fn name(&self) -> &str {
        "gbfs"
    }

    fn describe(&self) -> &str {
        "Greedy best-first search"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::CONDITIONAL_EFFECTS
    }

    fn solve(
        &mut self,
        task: &Task,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        let start = Instant::now();
        let mut stats = SearchStats::default();

        let mut open = BinaryHeap::new();
        let mut closed = rustc_hash::FxHashSet::default();

        let h = self.heuristic.estimate(task, &task.init);
        open.push(GbfsNode {
            h_score: h,
            state: task.init.clone(),
            path: Vec::new(),
            g_score: 0.0,
        });

        while let Some(node) = open.pop() {
            if closed.contains(&node.state) {
                continue;
            }
            closed.insert(node.state.clone());
            stats.nodes_expanded += 1;

            if node.state.satisfies(&task.goal_pos, &task.goal_neg) {
                let plan = build_plan(&node.path, task);
                stats.plan_cost = plan.cost;
                stats.plan_length = plan.len();
                stats.elapsed = start.elapsed();
                return Ok(SearchOutcome::Plan(plan, stats));
            }

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
                if node.state.applicable(op) {
                    let next = node.state.apply(op);
                    if !closed.contains(&next) {
                        stats.nodes_generated += 1;
                        let h = self.heuristic.estimate(task, &next);
                        let mut new_path = node.path.clone();
                        new_path.push(op.id);
                        open.push(GbfsNode {
                            h_score: h,
                            state: next,
                            path: new_path,
                            g_score: node.g_score + op.cost as f64,
                        });
                    }
                }
            }
        }

        stats.elapsed = start.elapsed();
        Ok(SearchOutcome::Unsolvable(stats))
    }
}

fn build_plan(path: &[OpId], task: &Task) -> Plan {
    let mut plan = Plan::new();
    let mut total_cost = 0.0;
    for &op_id in path {
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
