//! Bidirectional A* with Error ([`Bae`], BAE*, Sadhukhan 2013).
//!
//! Each direction keeps three open lists over the same nodes (a three-heap [`Frontier`]):
//! one ordered by BAE*'s `b = 2g + h - h_opposite`, which picks expansions, and one each by
//! `f = g + h` and by `g`, which feed the stopping bound. The search stops once the best meet
//! costs no more than the larger of the [`frontier_bound`] and the mean of both sides'
//! smallest `b`.
//!
//! Forward, `h` is the planner's [`Heuristic`] and `h_opposite` the largest relaxed cost
//! (from the initial state) among the state's facts. Backward, `h` is that relaxed cost
//! over the subgoal's required facts and `h_opposite` the largest relaxed cost from the
//! goal; subgoals for which either is infinite are pruned.
//!
//! ```mermaid
//! flowchart TD
//!     limits{budget used up?} -->|yes| limit[LimitReached]
//!     limits -->|no| bound{"max(bound, mean b) >= best meet?"}
//!     bound -->|yes| plan[Plan]
//!     bound -->|no| done{both sides exhausted?}
//!     done -->|"yes, meet"| plan
//!     done -->|"yes, no meet"| unsolvable[Unsolvable]
//!     done -->|no| side{"b_fwd <= b_bwd?"}
//!     side -->|yes| fwd[expand forward]
//!     side -->|no| bwd[expand backward]
//!     fwd --> limits
//!     bwd --> limits
//! ```

use std::mem;

use crate::error::MiniplanError;
use crate::heuristic::{HFF, rpg_backward_fact_costs, rpg_fact_costs};
use crate::plan::Plan;
use crate::search::meet::MeetIndex;
use crate::search::open_list::{Frontier, frontier_bound, prefer_forward};
use crate::search::outcome::{Budget, reject_conditional_effects};
use crate::search::regression::{Subgoal, can_regress, max_fact_cost, regress};
use crate::search::{
    Heuristic, Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats,
};
use crate::task::{OpId, State, Task};

/// Index of the heap ordered by `b = 2g + h - h_opposite` in a BAE* [`Frontier`]. Picks
/// expansions.
const BY_B: usize = 0;

/// Index of the heap ordered by `f = g + h` in a BAE* [`Frontier`].
const BY_F: usize = 1;

/// Index of the heap ordered by `g` in a BAE* [`Frontier`].
const BY_G: usize = 2;

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
    /// Estimates the cost from a state to the goal for the forward direction.
    h_forward: Box<dyn Heuristic>,
}

impl Bae {
    /// Create a new BAE* planner with the given forward heuristic.
    #[must_use]
    pub fn new(h_forward: Box<dyn Heuristic>) -> Self {
        Self { h_forward }
    }

    /// Create a BAE* planner with default heuristic (HFF).
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(Box::new(HFF))
    }
}

impl Planner for Bae {
    fn name(&self) -> &'static str {
        "bae"
    }

    fn describe(&self) -> &'static str {
        "Bidirectional A* with Error (BAE*, Sadhukhan 2013)"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
            | PlannerCapabilities::NEGATIVE_PRECONDS
            | PlannerCapabilities::ACTION_COSTS
    }

    /// Runs BAE* on `task`.
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

/// The state of one BAE* run.
struct Search<'a> {
    /// The task being solved.
    task: &'a Task,
    /// The forward heuristic.
    heuristic: &'a dyn Heuristic,
    /// Limits and clock of this run.
    budget: Budget<'a>,
    /// Counters reported with the outcome.
    stats: SearchStats,
    /// Relaxed reachability cost of each fact from the initial state.
    fact_costs: Vec<f64>,
    /// Relaxed cost of each fact towards the goal.
    back_costs: Vec<f64>,
    /// Open lists over complete states, rooted at the initial state.
    forward: Frontier<State, 3>,
    /// Open lists over subgoals, rooted at the goal. Stays empty if the goal is pruned.
    backward: Frontier<Subgoal, 3>,
    /// Expanded nodes of both sides and the cheapest meet so far.
    meets: MeetIndex,
}

impl<'a> Search<'a> {
    /// Opens the forward root and, unless the goal is pruned, the backward root.
    fn new(task: &'a Task, heuristic: &'a dyn Heuristic, budget: Budget<'a>) -> Self {
        let mut search = Self {
            task,
            heuristic,
            budget,
            stats: SearchStats::default(),
            fact_costs: rpg_fact_costs(task, &task.init),
            back_costs: rpg_backward_fact_costs(task, &task.goal_pos, &task.goal_neg),
            forward: Frontier::new(),
            backward: Frontier::new(),
            meets: MeetIndex::new(task.num_facts()),
        };

        let init_h = heuristic.estimate(task, &task.init).0;
        let init_h_opposite = max_fact_cost(&task.init, &search.fact_costs);
        search
            .forward
            .push_root(task.init.clone(), priorities(0.0, init_h, init_h_opposite));

        let goal = (task.goal_pos.clone(), task.goal_neg.clone());
        let goal_h = max_fact_cost(&goal.0, &search.fact_costs);
        let goal_h_opposite = max_fact_cost(&goal.0, &search.back_costs);
        if goal_h != f64::INFINITY && goal_h_opposite != f64::INFINITY {
            search
                .backward
                .push_root(goal, priorities(0.0, goal_h, goal_h_opposite));
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

            let forward_b = self.forward.min_priority(BY_B);
            let backward_b = self.backward.min_priority(BY_B);
            if prefer_forward(forward_b, backward_b) {
                self.expand_forward();
            } else {
                self.expand_backward();
            }
        }
    }

    /// Returns the final outcome once the search is decided, or `None` to keep going.
    ///
    /// The search is decided when the lower bound on the plan cost reaches the best meet's
    /// cost, or when both sides are exhausted.
    fn check_finished(&mut self) -> Option<SearchOutcome> {
        let bound = self.lower_bound();
        if self.meets.best().is_some_and(|meet| bound >= meet.cost)
            || (self.forward.is_exhausted() && self.backward.is_exhausted())
        {
            return Some(self.finish());
        }

        None
    }

    /// Lower bound on the cost of any plan not found yet: the [`frontier_bound`], raised to
    /// the mean of both sides' smallest `b` when both sides are open.
    fn lower_bound(&self) -> f64 {
        let bound = frontier_bound(
            self.forward.min_priority(BY_F),
            self.backward.min_priority(BY_F),
            self.forward.min_priority(BY_G),
            self.backward.min_priority(BY_G),
        );

        match (
            self.forward.min_priority(BY_B),
            self.backward.min_priority(BY_B),
        ) {
            (Some(forward_b), Some(backward_b)) => bound.max(f64::midpoint(forward_b, backward_b)),
            _ => bound,
        }
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

    /// Expands the forward state with the smallest open `b`, if all three heaps still hold
    /// a live entry.
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
                    self.open_forward(next, new_g, &state, op.id);
                }
            }
        }
    }

    /// Opens the successor `next` of `state` at cost `g`.
    fn open_forward(&mut self, next: State, g: f64, state: &State, op: OpId) {
        let h = self.heuristic.estimate(self.task, &next).0;
        let h_opposite = max_fact_cost(&next, &self.fact_costs);

        self.stats.nodes_generated += 1;
        self.forward
            .push(next, g, priorities(g, h, h_opposite), state.clone(), op);
    }

    /// Expands the backward subgoal with the smallest open `b`, if all three heaps still
    /// hold a live entry.
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

    /// Opens the regressed subgoal `previous` at cost `g`, unless either backward estimate
    /// is infinite.
    fn open_backward(&mut self, previous: Subgoal, g: f64, subgoal: &Subgoal, op: OpId) {
        let h = max_fact_cost(&previous.0, &self.fact_costs);
        if h == f64::INFINITY {
            return;
        }

        let h_opposite = max_fact_cost(&previous.0, &self.back_costs);
        if h_opposite == f64::INFINITY {
            return;
        }

        self.stats.nodes_generated += 1;
        self.backward.push(
            previous,
            g,
            priorities(g, h, h_opposite),
            subgoal.clone(),
            op,
        );
    }
}

/// The `[b, f, g]` heap priorities of a node at cost `g` with estimate `h` towards the
/// other end and estimate `h_opposite` back towards its own root.
fn priorities(g: f64, h: f64, h_opposite: f64) -> [f64; 3] {
    let f = g + h;
    let b = 2.0 * g + h - h_opposite;

    [b, f, g]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::test_support::{expect_plan, make_op, make_test_task, state_of};
    use crate::search::{Astar, BiDij, BibfsUc, Nbs};
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

        let mut bae = Bae::with_defaults();
        let mut bibfs_uc = BibfsUc::new();
        let limits = SearchLimits::default();
        let bae_outcome = bae.solve(&task, &limits).unwrap();
        let uc_outcome = bibfs_uc.solve(&task, &limits).unwrap();
        let (bae_plan, _) = expect_plan(bae_outcome);
        let (uc_plan, _) = expect_plan(uc_outcome);
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
        let mut op = make_op(0, "op_cond", &[0], &[], &[1], &[], 1);
        op.conditional.push(CondEffect {
            cond_pos: state_of(&[0], 3),
            cond_neg: State::new(3),
            add: state_of(&[2], 3),
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

        let (bae_plan, _) = expect_plan(bae_outcome);
        let (bidij_plan, _) = expect_plan(bidij_outcome);
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

        let (bae_plan, _) = expect_plan(bae_outcome);
        let (astar_plan, _) = expect_plan(astar_outcome);
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

        let (bae_plan, _) = expect_plan(bae_outcome);
        let (nbs_plan, _) = expect_plan(nbs_outcome);
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

        let (bae_plan, _) = expect_plan(bae_outcome);
        let (astar_plan, _) = expect_plan(astar_outcome);
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

        let (bae_plan, bae_stats) = expect_plan(bae_outcome);
        let (bidij_plan, bidij_stats) = expect_plan(bidij_outcome);

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

        let (bae_plan, bae_stats) = expect_plan(bae_outcome);
        let (nbs_plan, nbs_stats) = expect_plan(nbs_outcome);

        assert_eq!(bae_plan.cost, nbs_plan.cost);
        assert!(
            bae_stats.nodes_expanded <= nbs_stats.nodes_expanded * 2,
            "BAE* expansions ({}) should be within 2x of NBS ({})",
            bae_stats.nodes_expanded,
            nbs_stats.nodes_expanded
        );
    }
}
