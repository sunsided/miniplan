//! Greedy best-first search ([`Gbfs`]): always expands the state that looks closest to the
//! goal.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use rustc_hash::FxHashSet;

use crate::error::MiniplanError;
use crate::search::Heuristic;
use crate::search::outcome::{Budget, plan_from_ops};
use crate::search::{
    HValue, Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats,
};
use crate::task::{OpId, State, Task};

/// An open GBFS node. Ordered by [`GbfsNode::h_score`] only, smallest first.
#[derive(Debug, Clone)]
struct GbfsNode {
    /// The heuristic estimate from [`GbfsNode::state`] to the goal.
    h_score: HValue,
    /// The state this node stands for.
    state: State,
    /// The operators leading from the initial state to [`GbfsNode::state`], in order.
    path: Vec<OpId>,
    /// Path cost from the initial state. Tracked but never used for ordering.
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
    /// Estimates the cost from a state to the goal; the only ordering criterion.
    heuristic: Box<dyn Heuristic>,
}

impl Gbfs {
    /// Create a new GBFS planner with the given heuristic.
    #[must_use]
    pub fn new(heuristic: Box<dyn Heuristic>) -> Self {
        Gbfs { heuristic }
    }

    /// The successors of `node` that are not closed yet.
    fn successors(&self, task: &Task, node: &GbfsNode, closed: &FxHashSet<State>) -> Vec<GbfsNode> {
        let mut successors = Vec::new();
        for op in &task.operators {
            if !node.state.applicable(op) {
                continue;
            }

            let next = node.state.apply(op);
            if closed.contains(&next) {
                continue;
            }

            let h = self.heuristic.estimate(task, &next);
            let mut path = node.path.clone();
            path.push(op.id);
            successors.push(GbfsNode {
                h_score: h,
                state: next,
                path,
                g_score: node.g_score + f64::from(op.cost),
            });
        }

        successors
    }
}

impl Planner for Gbfs {
    fn name(&self) -> &'static str {
        "gbfs"
    }

    fn describe(&self) -> &'static str {
        "Greedy best-first search"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::CONDITIONAL_EFFECTS
    }

    /// Runs greedy best-first search on `task`. Never fails.
    fn solve(
        &mut self,
        task: &Task,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        let budget = Budget::start(limits);
        let mut stats = SearchStats::default();
        let mut open = BinaryHeap::new();
        let mut closed = FxHashSet::default();

        open.push(GbfsNode {
            h_score: self.heuristic.estimate(task, &task.init),
            state: task.init.clone(),
            path: Vec::new(),
            g_score: 0.0,
        });

        while let Some(node) = open.pop() {
            if !closed.insert(node.state.clone()) {
                continue;
            }

            stats.nodes_expanded += 1;
            if node.state.satisfies(&task.goal_pos, &task.goal_neg) {
                return Ok(budget.plan(plan_from_ops(&node.path, task), stats));
            }

            if budget.exhausted(&stats) {
                return Ok(budget.limit_reached(stats));
            }

            for successor in self.successors(task, &node, &closed) {
                stats.nodes_generated += 1;
                open.push(successor);
            }
        }

        Ok(budget.unsolvable(stats))
    }
}
