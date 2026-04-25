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
struct AstarNode {
    f_score: HValue,
    g_score: f64,
    state: State,
    path: Vec<OpId>,
}

impl PartialEq for AstarNode {
    fn eq(&self, other: &Self) -> bool {
        self.f_score == other.f_score
    }
}

impl Eq for AstarNode {}

impl PartialOrd for AstarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AstarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap
        other
            .f_score
            .0
            .partial_cmp(&self.f_score.0)
            .unwrap_or(Ordering::Equal)
    }
}

/// A* search with a pluggable heuristic.
///
/// A* expands nodes in order of `f = g + h`, where `g` is the cost from the
/// start and `h` is the heuristic estimate to the goal. With an admissible
/// heuristic, A* guarantees optimal solutions.
///
/// # Examples
///
/// ```
/// use miniplan::search::{Astar, Planner};
/// use miniplan::heuristic::HFF;
///
/// let planner = Astar::new(Box::new(HFF));
/// assert_eq!(planner.name(), "astar");
/// ```
pub struct Astar {
    heuristic: Box<dyn Heuristic>,
}

impl Astar {
    /// Create a new A* planner with the given heuristic.
    pub fn new(heuristic: Box<dyn Heuristic>) -> Self {
        Astar { heuristic }
    }
}

impl Planner for Astar {
    fn name(&self) -> &str {
        "astar"
    }

    fn describe(&self) -> &str {
        "A* search with pluggable heuristic"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::CONDITIONAL_EFFECTS
            | PlannerCapabilities::ACTION_COSTS
    }

    fn solve(
        &mut self,
        task: &Task,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        let start = Instant::now();
        let mut stats = SearchStats::default();

        let mut open = BinaryHeap::new();
        let mut g_scores = rustc_hash::FxHashMap::default();
        let mut closed = rustc_hash::FxHashSet::default();

        let h = self.heuristic.estimate(task, &task.init);
        open.push(AstarNode {
            f_score: h,
            g_score: 0.0,
            state: task.init.clone(),
            path: Vec::new(),
        });
        g_scores.insert(task.init.clone(), 0.0);

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
                    if closed.contains(&next) {
                        continue;
                    }

                    let new_g = node.g_score + op.cost as f64;
                    let dominated = if let Some(&existing_g) = g_scores.get(&next) {
                        new_g >= existing_g
                    } else {
                        false
                    };

                    if !dominated {
                        g_scores.insert(next.clone(), new_g);
                        stats.nodes_generated += 1;
                        let h = self.heuristic.estimate(task, &next);
                        let f = HValue(new_g + h.0);
                        let mut new_path = node.path.clone();
                        new_path.push(op.id);
                        open.push(AstarNode {
                            f_score: f,
                            g_score: new_g,
                            state: next,
                            path: new_path,
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
