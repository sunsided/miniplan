//! Bidirectional Dijkstra ([`BiDij`]): uniform-cost search from both ends at once.
//!
//! The forward side runs Dijkstra over complete states from the initial state, the backward
//! side runs Dijkstra over [`Subgoal`]s regressed from the goal. Each step expands the side
//! with the smaller open `g`, and a [`MeetIndex`] records the cheapest forward state that
//! satisfies a backward subgoal.
//!
//! ```mermaid
//! flowchart TD
//!     limits{budget used up?} -->|yes| limit[LimitReached]
//!     limits -->|no| bound{"g_fwd + g_bwd >= best meet?"}
//!     bound -->|yes| plan[Plan]
//!     bound -->|no| done{forward exhausted?}
//!     done -->|"yes, meet"| plan
//!     done -->|"yes, no meet"| unsolvable[Unsolvable]
//!     done -->|no| side{"g_fwd <= g_bwd?"}
//!     side -->|yes| fwd[expand forward]
//!     side -->|no| bwd[expand backward]
//!     fwd --> limits
//!     bwd --> limits
//! ```

// TODO(perf): per-FactId bucket index over closed_other to avoid O(|closed|)
//             meet scans (inherited from bibfs-uc).

use std::collections::BinaryHeap;
use std::hash::Hash;
use std::mem;

use rustc_hash::FxHashMap;

use crate::error::MiniplanError;
use crate::plan::Plan;
use crate::search::meet::MeetIndex;
use crate::search::open_list::{OpenEntry, prefer_forward};
use crate::search::outcome::{Budget, ParentMap, ROOT_OP, reject_conditional_effects};
use crate::search::regression::{Subgoal, can_regress, regress};
use crate::search::{Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats};
use crate::task::{OpId, State, Task};

/// Bidirectional Dijkstra's algorithm (cost-aware).
///
/// Searches forward from the initial state and backward from the goal
/// simultaneously, expanding the side with the lowest `g` value.
/// Guarantees optimal (minimum-cost) plans.
///
/// # Examples
///
/// ```
/// use miniplan::search::{BiDij, Planner};
///
/// let planner = BiDij::new();
/// assert_eq!(planner.name(), "bidij");
/// ```
#[derive(Default)]
pub struct BiDij {}

impl BiDij {
    /// Create a new bidirectional Dijkstra planner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Planner for BiDij {
    fn name(&self) -> &'static str {
        "bidij"
    }

    fn describe(&self) -> &'static str {
        "Bidirectional Dijkstra (cost-aware)"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::ACTION_COSTS
    }

    /// Runs bidirectional Dijkstra on `task`.
    ///
    /// # Errors
    ///
    /// Returns [`MiniplanError::UnsupportedConditionalEffects`] if any operator has a
    /// conditional effect, since regression through them is not supported.
    fn solve(
        &mut self,
        task: &Task,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        let budget = Budget::start(limits);
        reject_conditional_effects(task)?;

        if task.init.satisfies(&task.goal_pos, &task.goal_neg) {
            return Ok(budget.plan(Plan::new(), SearchStats::default()));
        }

        Ok(Search::new(task, budget).run())
    }
}

/// The state of one bidirectional Dijkstra run.
struct Search<'a> {
    /// The task being solved.
    task: &'a Task,
    /// Limits and clock of this run.
    budget: Budget<'a>,
    /// Counters reported with the outcome.
    stats: SearchStats,
    /// Dijkstra over complete states, rooted at the initial state.
    forward: DijkstraSide<State>,
    /// Dijkstra over subgoals, rooted at the goal.
    backward: DijkstraSide<Subgoal>,
    /// Labelled nodes of both sides and the cheapest meet so far.
    meets: MeetIndex,
}

impl<'a> Search<'a> {
    /// Opens both search roots: the initial state forward and the goal backward.
    fn new(task: &'a Task, budget: Budget<'a>) -> Self {
        let goal = (task.goal_pos.clone(), task.goal_neg.clone());
        let mut meets = MeetIndex::new(task.num_facts());
        meets.label_forward(&task.init, 0.0);
        meets.label_backward(&goal, 0.0);

        Self {
            task,
            budget,
            stats: SearchStats::default(),
            forward: DijkstraSide::new(task.init.clone()),
            backward: DijkstraSide::new(goal),
            meets,
        }
    }

    /// Expands one side at a time until a limit, the optimal meet or exhaustion ends the
    /// search.
    fn run(mut self) -> SearchOutcome {
        loop {
            if self.budget.exhausted(&self.stats) {
                return self.budget.limit_reached(self.stats);
            }

            if let Some(outcome) = self.check_finished() {
                return outcome;
            }

            if prefer_forward(self.forward.min_g(), self.backward.min_g()) {
                self.expand_forward();
            } else {
                self.expand_backward();
            }
        }
    }

    /// Returns the final outcome once the search is decided, or `None` to keep going.
    ///
    /// The search is decided when the open minima of both sides cannot beat the best meet,
    /// when both sides are empty, or when the forward side is exhausted after the backward
    /// root was expanded.
    fn check_finished(&mut self) -> Option<SearchOutcome> {
        if let (Some(forward_g), Some(backward_g)) = (self.forward.min_g(), self.backward.min_g())
            && let Some(meet) = self.meets.best()
            && forward_g + backward_g >= meet.cost
        {
            return Some(self.finish());
        }

        let forward_empty = self.forward.is_empty();
        let backward_empty = self.backward.is_empty();

        // Forward Dijkstra has expanded every reachable state, and the backward root (the
        // goal itself, g = 0) has been matched against each of them, so the best meet is
        // final. Backward regression alone may never run dry: it keeps producing new,
        // possibly contradictory subgoals.
        let forward_exhausted = forward_empty && self.meets.has_backward();

        if (forward_empty && backward_empty) || forward_exhausted {
            return Some(self.finish());
        }

        None
    }

    /// Ends the search with the best meet's plan, or as unsolvable without one.
    fn finish(&mut self) -> SearchOutcome {
        let stats = mem::take(&mut self.stats);
        match self.meets.best() {
            Some(meet) => {
                let plan = meet.plan(&self.forward.parents, &self.backward.parents, self.task);
                self.budget.plan(plan, stats)
            }
            None => self.budget.unsolvable(stats),
        }
    }

    /// Expands the cheapest open forward state, unless its entry turns out to be outdated.
    fn expand_forward(&mut self) {
        let Some((state, g)) = self.forward.pop_settled() else {
            return;
        };

        self.stats.nodes_expanded += 1;
        self.meets.expand_forward(&state, g);

        let task = self.task;
        for op in &task.operators {
            if state.applicable(op) {
                let next = state.apply(op);
                let new_g = g + f64::from(op.cost);
                if self.forward.relax(next.clone(), new_g, &state, op.id) {
                    self.meets.label_forward(&next, new_g);
                    self.stats.nodes_generated += 1;
                }
            }
        }
    }

    /// Expands the cheapest open backward subgoal, unless its entry turns out to be
    /// outdated.
    fn expand_backward(&mut self) {
        let Some((subgoal, g)) = self.backward.pop_settled() else {
            return;
        };

        self.stats.nodes_expanded += 1;
        self.meets.expand_backward(&subgoal, g);

        let task = self.task;
        for op in &task.operators {
            if can_regress(op, &subgoal) {
                let previous = regress(op, &subgoal, task.num_facts());
                let new_g = g + f64::from(op.cost);
                if self
                    .backward
                    .relax(previous.clone(), new_g, &subgoal, op.id)
                {
                    self.meets.label_backward(&previous, new_g);
                    self.stats.nodes_generated += 1;
                }
            }
        }
    }
}

/// One direction of the search: a Dijkstra open list over nodes of type `T`.
///
/// Entries are never removed when a node's cost improves; outdated ones are skipped when
/// popped.
struct DijkstraSide<T> {
    /// Open entries ordered by `g`.
    open: BinaryHeap<OpenEntry<T>>,
    /// Best known path cost per node.
    g: FxHashMap<T, f64>,
    /// Parent links of the search tree, for plan reconstruction.
    parents: ParentMap<T>,
}

impl<T: Clone + Eq + Hash> DijkstraSide<T> {
    /// Opens `root` at cost `0`.
    fn new(root: T) -> Self {
        let mut side = Self {
            open: BinaryHeap::new(),
            g: FxHashMap::default(),
            parents: ParentMap::default(),
        };
        side.push(root.clone(), 0.0);
        side.g.insert(root.clone(), 0.0);
        side.parents.insert(root, (None, ROOT_OP));

        side
    }

    /// The smallest open `g`, outdated entries included.
    fn min_g(&self) -> Option<f64> {
        self.open.peek().map(|entry| entry.g)
    }

    /// Returns `true` if no entry is open.
    fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// Pops the cheapest entry and returns its node and `g`.
    ///
    /// Returns `None` if the list is empty or the entry is outdated, because a cheaper path
    /// to its node was found after it was pushed.
    fn pop_settled(&mut self) -> Option<(T, f64)> {
        let entry = self.open.pop()?;
        if entry.g > self.best_g(&entry.node) {
            return None;
        }

        Some((entry.node, entry.g))
    }

    /// Opens `node` at cost `g`, reached from `parent` through `op`, if that is cheaper
    /// than its best known cost. Returns `true` if it was.
    fn relax(&mut self, node: T, g: f64, parent: &T, op: OpId) -> bool {
        if g < self.best_g(&node) {
            self.g.insert(node.clone(), g);
            self.parents
                .insert(node.clone(), (Some(parent.clone()), op));
            self.push(node, g);

            true
        } else {
            false
        }
    }

    /// Best known path cost of `node`, or infinity if it was never opened.
    fn best_g(&self, node: &T) -> f64 {
        self.g.get(node).copied().unwrap_or(f64::INFINITY)
    }

    /// Pushes an open entry for `node` at cost `g`.
    fn push(&mut self, node: T, g: f64) {
        self.open.push(OpenEntry {
            priority: g,
            g,
            node,
            epoch: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heuristic::HFF;
    use crate::search::test_support::{expect_plan, make_op, make_test_task, state_of};
    use crate::search::{Astar, BibfsUc};
    use crate::task::CondEffect;

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
        let (bidij_plan, _) = expect_plan(bidij_outcome);
        let (uc_plan, _) = expect_plan(uc_outcome);
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
        let mut op = make_op(0, "op_cond", &[0], &[], &[1], &[], 1);
        op.conditional.push(CondEffect {
            cond_pos: state_of(&[0], 3),
            cond_neg: State::new(3),
            add: state_of(&[2], 3),
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

        let (bidij_plan, _) = expect_plan(bidij_outcome);
        let (astar_plan, _) = expect_plan(astar_outcome);
        assert_eq!(bidij_plan.cost, astar_plan.cost);
        assert_eq!(bidij_plan.cost, 2.0);
    }

    #[test]
    fn test_stale_pops_skipped() {
        // Two paths to state {1}: op0 (cost 3) and op1 (cost 1).
        // After {1} is settled at g=1, the stale entry at g=3 should be skipped.
        // Chain: 0 -> 1 -> 2 -> 3 -> 4
        // Meets between a labelled and an expanded node are found as soon as both exist,
        // so the cost-4 meet is final after 5 expansions; the stale {1}@g3 entry is never
        // counted as one.
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
                assert_eq!(stats.nodes_expanded, 5);
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

        let (bidij_plan, _) = expect_plan(bidij_outcome);
        let (astar_plan, _) = expect_plan(astar_outcome);
        assert_eq!(bidij_plan.cost, astar_plan.cost);
        assert_eq!(bidij_plan.cost, 3.0);
        assert_eq!(bidij_plan.len(), 3);
    }
}
