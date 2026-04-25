use std::collections::VecDeque;
use std::time::Instant;

use crate::error::MiniplanError;
use crate::plan::{Plan, PlanStep};
use crate::search::{Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats, Task};

#[derive(Default)]
pub struct Bfs {}

impl Bfs {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Planner for Bfs {
    fn name(&self) -> &str {
        "bfs"
    }

    fn describe(&self) -> &str {
        "Breadth-first search planner"
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

        let mut queue = VecDeque::new();
        let mut closed = rustc_hash::FxHashSet::default();

        queue.push_back((task.init.clone(), Vec::new()));
        closed.insert(task.init.clone());

        while let Some((state, path)) = queue.pop_front() {
            stats.nodes_expanded += 1;

            if state.satisfies(&task.goal_pos, &task.goal_neg) {
                let plan = build_plan(&path, task);
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
                if state.applicable(op) {
                    let next = state.apply(op);
                    if !closed.contains(&next) {
                        closed.insert(next.clone());
                        stats.nodes_generated += 1;
                        let mut new_path = path.clone();
                        new_path.push(op.id);
                        queue.push_back((next, new_path));
                    }
                }
            }
        }

        stats.elapsed = start.elapsed();
        Ok(SearchOutcome::Unsolvable(stats))
    }
}

fn build_plan(path: &[crate::task::OpId], task: &Task) -> Plan {
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
