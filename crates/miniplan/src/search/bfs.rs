//! Breadth-first search ([`Bfs`]) over complete states.

use std::collections::VecDeque;

use rustc_hash::FxHashSet;

use crate::error::MiniplanError;
use crate::search::outcome::{Budget, plan_from_ops};
use crate::search::{Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats, Task};

/// Breadth-first search planner.
///
/// Explores the state space level by level, guaranteeing the shortest
/// (fewest-step) plan. Does not support action costs.
///
/// # Examples
///
/// ```
/// use miniplan::search::{Bfs, Planner};
///
/// let planner = Bfs::new();
/// assert_eq!(planner.name(), "bfs");
/// ```
#[derive(Default)]
pub struct Bfs {}

impl Bfs {
    /// Create a new BFS planner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Planner for Bfs {
    fn name(&self) -> &'static str {
        "bfs"
    }

    fn describe(&self) -> &'static str {
        "Breadth-first search planner"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL | PlannerCapabilities::NEGATIVE_PRECONDS
    }

    /// Runs breadth-first search on `task`. Never fails.
    fn solve(
        &mut self,
        task: &Task,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        let budget = Budget::start(limits);
        let mut stats = SearchStats::default();
        let mut queue = VecDeque::new();
        let mut closed = FxHashSet::default();

        queue.push_back((task.init.clone(), Vec::new()));
        closed.insert(task.init.clone());

        while let Some((state, path)) = queue.pop_front() {
            stats.nodes_expanded += 1;
            if state.satisfies(&task.goal_pos, &task.goal_neg) {
                return Ok(budget.plan(plan_from_ops(&path, task), stats));
            }

            if budget.exhausted(&stats) {
                return Ok(budget.limit_reached(stats));
            }

            for op in &task.operators {
                if state.applicable(op) {
                    let next = state.apply(op);
                    if closed.insert(next.clone()) {
                        stats.nodes_generated += 1;
                        let mut new_path = path.clone();
                        new_path.push(op.id);
                        queue.push_back((next, new_path));
                    }
                }
            }
        }

        Ok(budget.unsolvable(stats))
    }
}
