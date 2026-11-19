//! A* search ([`Astar`]) over complete states with a pluggable [`Heuristic`].

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::MiniplanError;
use crate::search::Heuristic;
use crate::search::outcome::{Budget, plan_from_ops};
use crate::search::{
    HValue, Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats,
};
use crate::task::{OpId, State, Task};

/// An open A* node. Ordered by [`AstarNode::f_score`] only, smallest first.
#[derive(Debug, Clone)]
struct AstarNode {
    /// `g + h`: path cost so far plus the heuristic estimate to the goal.
    f_score: HValue,
    /// Path cost from the initial state to [`AstarNode::state`].
    g_score: f64,
    /// The state this node stands for.
    state: State,
    /// The operators leading from the initial state to [`AstarNode::state`], in order.
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
    /// Estimates the cost from a state to the goal; the `h` in `f = g + h`.
    heuristic: Box<dyn Heuristic>,
}

impl Astar {
    /// Create a new A* planner with the given heuristic.
    #[must_use]
    pub fn new(heuristic: Box<dyn Heuristic>) -> Self {
        Astar { heuristic }
    }

    /// The successors of `node` that are neither closed nor reached more cheaply before,
    /// recording their new `g` in `g_scores`.
    fn successors(
        &self,
        task: &Task,
        node: &AstarNode,
        closed: &FxHashSet<State>,
        g_scores: &mut FxHashMap<State, f64>,
    ) -> Vec<AstarNode> {
        let mut successors = Vec::new();
        for op in &task.operators {
            if !node.state.applicable(op) {
                continue;
            }

            let next = node.state.apply(op);
            if closed.contains(&next) {
                continue;
            }

            let new_g = node.g_score + f64::from(op.cost);
            if g_scores
                .get(&next)
                .is_some_and(|&existing_g| new_g >= existing_g)
            {
                continue;
            }

            g_scores.insert(next.clone(), new_g);
            let h = self.heuristic.estimate(task, &next);
            let mut path = node.path.clone();
            path.push(op.id);
            successors.push(AstarNode {
                f_score: HValue(new_g + h.0),
                g_score: new_g,
                state: next,
                path,
            });
        }

        successors
    }
}

impl Planner for Astar {
    fn name(&self) -> &'static str {
        "astar"
    }

    fn describe(&self) -> &'static str {
        "A* search with pluggable heuristic"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::CONDITIONAL_EFFECTS
            | PlannerCapabilities::ACTION_COSTS
    }

    /// Runs A* on `task`. Never fails.
    fn solve(
        &mut self,
        task: &Task,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        let budget = Budget::start(limits);
        let mut stats = SearchStats::default();
        let mut open = BinaryHeap::new();
        let mut g_scores = FxHashMap::default();
        let mut closed = FxHashSet::default();

        open.push(AstarNode {
            f_score: self.heuristic.estimate(task, &task.init),
            g_score: 0.0,
            state: task.init.clone(),
            path: Vec::new(),
        });
        g_scores.insert(task.init.clone(), 0.0);

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

            for successor in self.successors(task, &node, &closed, &mut g_scores) {
                stats.nodes_generated += 1;
                open.push(successor);
            }
        }

        Ok(budget.unsolvable(stats))
    }
}
