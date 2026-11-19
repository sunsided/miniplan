//! Near-Optimal Bidirectional Search ([`Nbs`], Chen et al. 2017).
//!
//! Each direction keeps two open lists over the same nodes, one ordered by `f = g + h` and
//! one by `g` (a two-heap [`Frontier`]). The side with the smaller open `f` expands next,
//! and the search stops once the best meet costs no more than the [`frontier_bound`] of the
//! open lists.
//!
//! The forward heuristic is the planner's [`Heuristic`]. The backward one is the largest
//! relaxed reachability cost (from the initial state) among a subgoal's required facts;
//! subgoals with an unreachable fact are pruned.
//!
//! ```mermaid
//! flowchart TD
//!     limits{budget used up?} -->|yes| limit[LimitReached]
//!     limits -->|no| bound{"bound >= best meet?"}
//!     bound -->|yes| plan[Plan]
//!     bound -->|no| done{both sides exhausted?}
//!     done -->|"yes, meet"| plan
//!     done -->|"yes, no meet"| unsolvable[Unsolvable]
//!     done -->|no| side{"f_fwd <= f_bwd?"}
//!     side -->|yes| fwd[expand forward]
//!     side -->|no| bwd[expand backward]
//!     fwd --> limits
//!     bwd --> limits
//! ```

use std::mem;

use crate::error::MiniplanError;
use crate::heuristic::{HFF, rpg_fact_costs};
use crate::plan::Plan;
use crate::search::meet::MeetIndex;
use crate::search::open_list::{Frontier, frontier_bound, prefer_forward};
use crate::search::outcome::{Budget, reject_conditional_effects};
use crate::search::regression::{Subgoal, can_regress, max_fact_cost, regress};
use crate::search::{
    Heuristic, Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats,
};
use crate::task::{OpId, State, Task};

/// Index of the heap ordered by `f = g + h` in an NBS [`Frontier`]. Picks expansions.
const BY_F: usize = 0;

/// Index of the heap ordered by `g` in an NBS [`Frontier`].
const BY_G: usize = 1;

/// Near-Optimal Bidirectional Search (NBS, Chen et al. 2017).
///
/// A bidirectional heuristic search that uses front-to-front evaluation
/// with per-fact arena indexing for efficient meet detection. Guarantees
/// optimal plans when using admissible heuristics.
///
/// # Examples
///
/// ```
/// use miniplan::search::{Nbs, Planner};
/// use miniplan::heuristic::HFF;
///
/// let planner = Nbs::new(Box::new(HFF));
/// assert_eq!(planner.name(), "nbs");
///
/// let planner = Nbs::with_defaults();
/// ```
pub struct Nbs {
    /// Estimates the cost from a state to the goal for the forward direction.
    h_forward: Box<dyn Heuristic>,
}

impl Nbs {
    /// Create a new NBS planner with the given forward heuristic.
    #[must_use]
    pub fn new(h_forward: Box<dyn Heuristic>) -> Self {
        Self { h_forward }
    }

    /// Create an NBS planner with default heuristic (HFF).
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(Box::new(HFF))
    }
}

impl Planner for Nbs {
    fn name(&self) -> &'static str {
        "nbs"
    }

    fn describe(&self) -> &'static str {
        "Near-Optimal Bidirectional Search (Chen et al. 2017)"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::ACTION_COSTS
    }

    /// Runs NBS on `task`.
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

        Ok(Search::new(task, self.h_forward.as_ref(), budget).run())
    }
}

/// The state of one NBS run.
struct Search<'a> {
    /// The task being solved.
    task: &'a Task,
    /// The forward heuristic.
    heuristic: &'a dyn Heuristic,
    /// Limits and clock of this run.
    budget: Budget<'a>,
    /// Counters reported with the outcome.
    stats: SearchStats,
    /// Relaxed reachability cost of each fact from the initial state; the backward
    /// heuristic.
    fact_costs: Vec<f64>,
    /// Open lists over complete states, rooted at the initial state.
    forward: Frontier<State, 2>,
    /// Open lists over subgoals, rooted at the goal. Stays empty if the goal needs an
    /// unreachable fact.
    backward: Frontier<Subgoal, 2>,
    /// Expanded nodes of both sides and the cheapest meet so far.
    meets: MeetIndex,
}

impl<'a> Search<'a> {
    /// Opens the forward root and, unless the goal is relaxed-unreachable, the backward
    /// root.
    fn new(task: &'a Task, heuristic: &'a dyn Heuristic, budget: Budget<'a>) -> Self {
        let mut search = Self {
            task,
            heuristic,
            budget,
            stats: SearchStats::default(),
            fact_costs: rpg_fact_costs(task, &task.init),
            forward: Frontier::new(),
            backward: Frontier::new(),
            meets: MeetIndex::new(task.num_facts()),
        };

        let init_h = heuristic.estimate(task, &task.init).0;
        search
            .forward
            .push_root(task.init.clone(), [0.0 + init_h, 0.0]);

        let goal = (task.goal_pos.clone(), task.goal_neg.clone());
        let goal_h = max_fact_cost(&goal.0, &search.fact_costs);
        if goal_h != f64::INFINITY {
            search.backward.push_root(goal, [0.0 + goal_h, 0.0]);
        }

        search
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

            let forward_f = self.forward.min_priority(BY_F);
            let backward_f = self.backward.min_priority(BY_F);
            if prefer_forward(forward_f, backward_f) {
                self.expand_forward();
            } else {
                self.expand_backward();
            }
        }
    }

    /// Returns the final outcome once the search is decided, or `None` to keep going.
    ///
    /// The search is decided when the [`frontier_bound`] reaches the best meet's cost, or
    /// when both sides are exhausted.
    fn check_finished(&mut self) -> Option<SearchOutcome> {
        let bound = frontier_bound(
            self.forward.min_priority(BY_F),
            self.backward.min_priority(BY_F),
            self.forward.min_priority(BY_G),
            self.backward.min_priority(BY_G),
        );

        if self.meets.best().is_some_and(|meet| bound >= meet.cost)
            || (self.forward.is_exhausted() && self.backward.is_exhausted())
        {
            return Some(self.finish());
        }

        None
    }

    /// Ends the search with the best meet's plan, or as unsolvable without one.
    fn finish(&mut self) -> SearchOutcome {
        let stats = mem::take(&mut self.stats);
        match self.meets.best() {
            Some(meet) => {
                let plan = meet.plan(self.forward.parents(), self.backward.parents(), self.task);
                self.budget.plan(plan, stats)
            }
            None => self.budget.unsolvable(stats),
        }
    }

    /// Expands the forward state with the smallest open `f`, if both heaps still hold a
    /// live entry.
    fn expand_forward(&mut self) {
        let Some((state, g)) = self.forward.pop_expansion() else {
            return;
        };

        self.stats.nodes_expanded += 1;
        self.meets.add_forward(&state, g);

        let task = self.task;
        for op in &task.operators {
            if state.applicable(op) {
                let next = state.apply(op);
                let new_g = g + f64::from(op.cost);
                if self.forward.improves(&next, new_g) {
                    let h = self.heuristic.estimate(task, &next).0;
                    self.stats.nodes_generated += 1;
                    self.forward
                        .push(next, new_g, [new_g + h, new_g], state.clone(), op.id);
                }
            }
        }
    }

    /// Expands the backward subgoal with the smallest open `f`, if both heaps still hold a
    /// live entry. Regressed subgoals that need an unreachable fact are dropped.
    fn expand_backward(&mut self) {
        let Some((subgoal, g)) = self.backward.pop_expansion() else {
            return;
        };

        self.stats.nodes_expanded += 1;
        self.meets.add_backward(&subgoal, g);

        let task = self.task;
        for op in &task.operators {
            if can_regress(op, &subgoal) {
                let previous = regress(op, &subgoal, task.num_facts());
                let new_g = g + f64::from(op.cost);
                if self.backward.improves(&previous, new_g) {
                    self.open_backward(previous, new_g, &subgoal, op.id);
                }
            }
        }
    }

    /// Opens the regressed subgoal `previous` at cost `g` unless it needs an unreachable
    /// fact.
    fn open_backward(&mut self, previous: Subgoal, g: f64, subgoal: &Subgoal, op: OpId) {
        let h = max_fact_cost(&previous.0, &self.fact_costs);
        if h == f64::INFINITY {
            return;
        }

        self.stats.nodes_generated += 1;
        self.backward
            .push(previous, g, [g + h, g], subgoal.clone(), op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::test_support::{expect_plan, make_op, make_test_task, state_of};
    use crate::search::{Astar, BiDij, BibfsUc};
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

        let mut nbs = Nbs::with_defaults();
        let mut bibfs_uc = BibfsUc::new();
        let limits = SearchLimits::default();
        let nbs_outcome = nbs.solve(&task, &limits).unwrap();
        let uc_outcome = bibfs_uc.solve(&task, &limits).unwrap();
        let (nbs_plan, _) = expect_plan(nbs_outcome);
        let (uc_plan, _) = expect_plan(uc_outcome);
        assert_eq!(nbs_plan.len(), uc_plan.len());
        assert_eq!(nbs_plan.steps[0].op_id, OpId(0));
        assert_eq!(nbs_plan.steps[1].op_id, OpId(1));
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

        let mut nbs = Nbs::with_defaults();
        let limits = SearchLimits::default();
        let outcome = nbs.solve(&task, &limits).unwrap();
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

        let mut nbs = Nbs::with_defaults();
        let limits = SearchLimits::default();
        let outcome = nbs.solve(&task, &limits).unwrap();
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

        let mut nbs = Nbs::with_defaults();
        let limits = SearchLimits::default();
        let result = nbs.solve(&task, &limits);
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

        let mut nbs = Nbs::with_defaults();
        let limits = SearchLimits::default();
        let outcome = nbs.solve(&task, &limits).unwrap();
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

        let mut nbs = Nbs::with_defaults();
        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let nbs_outcome = nbs.solve(&task, &limits).unwrap();
        let bidij_outcome = bidij.solve(&task, &limits).unwrap();

        let (nbs_plan, _) = expect_plan(nbs_outcome);
        let (bidij_plan, _) = expect_plan(bidij_outcome);
        assert_eq!(nbs_plan.cost, bidij_plan.cost);
        assert_eq!(nbs_plan.cost, 2.0);
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

        let mut nbs = Nbs::with_defaults();
        let mut astar = Astar::new(Box::new(HFF));
        let limits = SearchLimits::default();
        let nbs_outcome = nbs.solve(&task, &limits).unwrap();
        let astar_outcome = astar.solve(&task, &limits).unwrap();

        let (nbs_plan, _) = expect_plan(nbs_outcome);
        let (astar_plan, _) = expect_plan(astar_outcome);
        assert_eq!(nbs_plan.cost, astar_plan.cost);
        assert_eq!(nbs_plan.cost, 2.0);
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

        let mut nbs = Nbs::with_defaults();
        let mut astar = Astar::new(Box::new(HFF));
        let limits = SearchLimits::default();
        let nbs_outcome = nbs.solve(&task, &limits).unwrap();
        let astar_outcome = astar.solve(&task, &limits).unwrap();

        let (nbs_plan, _) = expect_plan(nbs_outcome);
        let (astar_plan, _) = expect_plan(astar_outcome);
        assert_eq!(nbs_plan.cost, astar_plan.cost);
        assert_eq!(nbs_plan.cost, 3.0);
        assert_eq!(nbs_plan.len(), 3);
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

        let mut nbs = Nbs::with_defaults();
        let mut bidij = BiDij::new();
        let limits = SearchLimits::default();
        let nbs_outcome = nbs.solve(&task, &limits).unwrap();
        let bidij_outcome = bidij.solve(&task, &limits).unwrap();

        let (nbs_plan, nbs_stats) = expect_plan(nbs_outcome);
        let (bidij_plan, bidij_stats) = expect_plan(bidij_outcome);

        assert_eq!(nbs_plan.cost, bidij_plan.cost);
        assert!(nbs_stats.nodes_expanded <= bidij_stats.nodes_expanded);
    }
}
