//! Bidirectional breadth-first search ([`BibfsUc`]), uniform-cost by step count.
//!
//! Each round expands one full layer forward (complete states from the initial state), then
//! one full layer backward ([`Subgoal`]s regressed from the goal). Every newly generated node
//! is checked against all closed nodes of the other side, and the first match ends the search.

// Note: for cost-aware bidirectional search, use the `bidij` planner
// (Bidirectional Dijkstra). NBS is implemented in `search::nbs`; BAE* is
// implemented in `search::bae`.
// TODO(cond-effects): support via regression through conditional effects
// TODO(perf): per-FactId bucket index to avoid O(|F|·|B|) scan

use std::collections::VecDeque;
use std::mem;

use crate::error::MiniplanError;
use crate::plan::Plan;
use crate::search::outcome::{
    Budget, ParentMap, ROOT_OP, plan_through, reject_conditional_effects,
};
use crate::search::regression::{Subgoal, can_regress, regress};
use crate::search::{Planner, PlannerCapabilities, SearchLimits, SearchOutcome, SearchStats, Task};
use crate::task::State;

/// Bidirectional breadth-first search (uniform-cost).
///
/// Searches forward from the initial state and backward from the goal
/// simultaneously, meeting in the middle. Faster than unidirectional BFS
/// but does not account for action costs.
///
/// # Examples
///
/// ```
/// use miniplan::search::{BibfsUc, Planner};
///
/// let planner = BibfsUc::new();
/// assert_eq!(planner.name(), "bibfs-uc");
/// ```
#[derive(Default)]
pub struct BibfsUc {}

impl BibfsUc {
    /// Create a new bidirectional BFS planner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Planner for BibfsUc {
    fn name(&self) -> &'static str {
        "bibfs-uc"
    }

    fn describe(&self) -> &'static str {
        "Bidirectional BFS (uniform-cost, not cost-aware)"
    }

    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL | PlannerCapabilities::NEGATIVE_PRECONDS
    }

    /// Runs bidirectional breadth-first search on `task`.
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

/// The state of one bidirectional breadth-first run.
struct Search<'a> {
    /// The task being solved.
    task: &'a Task,
    /// Limits and clock of this run.
    budget: Budget<'a>,
    /// Counters reported with the outcome.
    stats: SearchStats,
    /// Forward states of the next layer to expand, in generation order.
    forward_queue: VecDeque<State>,
    /// Every forward state generated so far, with its parent link.
    forward_closed: ParentMap<State>,
    /// Backward subgoals of the next layer to expand, in generation order.
    backward_queue: VecDeque<Subgoal>,
    /// Every backward subgoal generated so far, with its parent link.
    backward_closed: ParentMap<Subgoal>,
}

impl<'a> Search<'a> {
    /// Opens both search roots: the initial state forward and the goal backward.
    fn new(task: &'a Task, budget: Budget<'a>) -> Self {
        let goal = (task.goal_pos.clone(), task.goal_neg.clone());
        let mut search = Self {
            task,
            budget,
            stats: SearchStats::default(),
            forward_queue: VecDeque::new(),
            forward_closed: ParentMap::default(),
            backward_queue: VecDeque::new(),
            backward_closed: ParentMap::default(),
        };

        search.forward_queue.push_back(task.init.clone());
        search
            .forward_closed
            .insert(task.init.clone(), (None, ROOT_OP));
        search.backward_queue.push_back(goal.clone());
        search.backward_closed.insert(goal, (None, ROOT_OP));

        search
    }

    /// Alternates forward and backward layers until a meet, a limit or forward exhaustion
    /// ends the search.
    fn run(mut self) -> SearchOutcome {
        loop {
            // Every forward state is checked against all backward subgoals, the goal itself
            // included, when it is generated. Once forward runs dry no reachable state
            // satisfies the goal. Backward regression alone may never run dry.
            if self.forward_queue.is_empty() {
                return self.budget.unsolvable(self.stats);
            }

            if let Some(outcome) = self.expand_forward_layer() {
                return outcome;
            }

            if !self.backward_queue.is_empty()
                && let Some(outcome) = self.expand_backward_layer()
            {
                return outcome;
            }
        }
    }

    /// Expands every queued forward state, then checks the new layer for a meet.
    ///
    /// Returns the outcome if a limit is hit or a meet is found.
    fn expand_forward_layer(&mut self) -> Option<SearchOutcome> {
        let mut layer = Vec::new();
        for _ in 0..self.forward_queue.len() {
            let state = self
                .forward_queue
                .pop_front()
                .expect("loop runs once per queued state");
            self.stats.nodes_expanded += 1;
            if self.budget.exhausted(&self.stats) {
                return Some(self.budget.limit_reached(mem::take(&mut self.stats)));
            }

            self.generate_forward(&state, &mut layer);
        }

        for state in &layer {
            self.forward_queue.push_back(state.clone());
            if let Some(plan) = self.meet_from_forward(state) {
                return Some(self.budget.plan(plan, mem::take(&mut self.stats)));
            }
        }

        None
    }

    /// Appends the not yet seen successors of `state` to `layer`, closing them.
    fn generate_forward(&mut self, state: &State, layer: &mut Vec<State>) {
        for op in &self.task.operators {
            if state.applicable(op) {
                let next = state.apply(op);
                if !self.forward_closed.contains_key(&next) {
                    self.stats.nodes_generated += 1;
                    self.forward_closed
                        .insert(next.clone(), (Some(state.clone()), op.id));
                    layer.push(next);
                }
            }
        }
    }

    /// Expands every queued backward subgoal, then checks the new layer for a meet.
    ///
    /// Returns the outcome if a limit is hit or a meet is found.
    fn expand_backward_layer(&mut self) -> Option<SearchOutcome> {
        let mut layer = Vec::new();
        for _ in 0..self.backward_queue.len() {
            let subgoal = self
                .backward_queue
                .pop_front()
                .expect("loop runs once per queued subgoal");
            self.stats.nodes_expanded += 1;
            if self.budget.exhausted(&self.stats) {
                return Some(self.budget.limit_reached(mem::take(&mut self.stats)));
            }

            self.generate_backward(&subgoal, &mut layer);
        }

        for subgoal in &layer {
            self.backward_queue.push_back(subgoal.clone());
            if let Some(plan) = self.meet_from_backward(subgoal) {
                return Some(self.budget.plan(plan, mem::take(&mut self.stats)));
            }
        }

        None
    }

    /// Appends the not yet seen regressions of `subgoal` to `layer`, closing them.
    fn generate_backward(&mut self, subgoal: &Subgoal, layer: &mut Vec<Subgoal>) {
        let num_facts = self.task.num_facts();
        for op in &self.task.operators {
            if can_regress(op, subgoal) {
                let previous = regress(op, subgoal, num_facts);
                if !self.backward_closed.contains_key(&previous) {
                    self.stats.nodes_generated += 1;
                    self.backward_closed
                        .insert(previous.clone(), (Some(subgoal.clone()), op.id));
                    layer.push(previous);
                }
            }
        }
    }

    /// The plan through the first closed backward subgoal that `state` satisfies, if any.
    fn meet_from_forward(&self, state: &State) -> Option<Plan> {
        self.backward_closed
            .keys()
            .find(|subgoal| state.satisfies(&subgoal.0, &subgoal.1))
            .map(|subgoal| self.plan_through(state, subgoal))
    }

    /// The plan through the first closed forward state that satisfies `subgoal`, if any.
    fn meet_from_backward(&self, subgoal: &Subgoal) -> Option<Plan> {
        self.forward_closed
            .keys()
            .find(|state| state.satisfies(&subgoal.0, &subgoal.1))
            .map(|state| self.plan_through(state, subgoal))
    }

    /// Joins the forward path to `state` and the backward path from `subgoal`.
    fn plan_through(&self, state: &State, subgoal: &Subgoal) -> Plan {
        plan_through(
            &self.forward_closed,
            state,
            &self.backward_closed,
            subgoal,
            self.task,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::Bfs;
    use crate::search::test_support::{expect_plan, make_op, make_test_task, state_of};
    use crate::task::{CondEffect, OpId};

    /// Solves `task` with [`BibfsUc`] and [`Bfs`] and returns both plans.
    ///
    /// # Panics
    ///
    /// Panics if either planner fails to find a plan.
    fn run_both(task: &Task) -> (Plan, Plan) {
        let mut bibfs = BibfsUc::new();
        let mut bfs = Bfs::new();
        let limits = SearchLimits::default();
        let bibfs_outcome = bibfs.solve(task, &limits).unwrap();
        let bfs_outcome = bfs.solve(task, &limits).unwrap();
        let (bibfs_plan, _) = expect_plan(bibfs_outcome);
        let (bfs_plan, _) = expect_plan(bfs_outcome);
        (bibfs_plan, bfs_plan)
    }

    #[test]
    fn test_simple_plan() {
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

        let (bibfs_plan, bfs_plan) = run_both(&task);
        assert_eq!(bibfs_plan.len(), bfs_plan.len());
        assert_eq!(bibfs_plan.steps[0].op_id, OpId(0));
        assert_eq!(bibfs_plan.steps[1].op_id, OpId(1));
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

        let mut bibfs = BibfsUc::new();
        let limits = SearchLimits::default();
        let outcome = bibfs.solve(&task, &limits).unwrap();
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

        let mut bibfs = BibfsUc::new();
        let limits = SearchLimits::default();
        let result = bibfs.solve(&task, &limits);
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

        let mut bibfs = BibfsUc::new();
        let limits = SearchLimits::default();
        let outcome = bibfs.solve(&task, &limits).unwrap();
        match outcome {
            SearchOutcome::Plan(plan, _) => {
                assert!(plan.is_empty());
                assert_eq!(plan.cost, 0.0);
            }
            _ => panic!("expected empty plan"),
        }
    }
}
