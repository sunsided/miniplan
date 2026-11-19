//! Search budgets, plan assembly and the terminal outcomes every planner reports.
//!
//! Planners create one [`Budget`] when a search starts, ask it after each expansion whether
//! the [`SearchLimits`] are used up, and end through one of its outcome constructors, so
//! `elapsed`, `plan_cost` and `plan_length` in [`SearchStats`] are filled in the same way
//! everywhere.

use std::hash::Hash;
use std::time::Instant;

use rustc_hash::FxHashMap;

use crate::error::MiniplanError;
use crate::plan::{Plan, PlanStep};
use crate::search::regression::Subgoal;
use crate::search::{SearchLimits, SearchOutcome, SearchStats};
use crate::task::{OpId, State, Task};

/// Parent links of a search tree: each node maps to its parent and the operator that led
/// from the parent to it.
///
/// The root maps to `(None, ROOT_OP)`. Every other node has `Some(parent)`.
pub(super) type ParentMap<T> = FxHashMap<T, (Option<T>, OpId)>;

/// Sentinel operator stored with the root of a [`ParentMap`]. Never a real operator index.
pub(super) const ROOT_OP: OpId = OpId(usize::MAX);

/// The limits of one search run, together with the instant the run started.
pub(super) struct Budget<'l> {
    /// The caller's limits. `None` fields are unbounded.
    limits: &'l SearchLimits,
    /// When the search started; the reference point for the time budget and `elapsed`.
    start: Instant,
}

impl<'l> Budget<'l> {
    /// Starts the clock for a search bounded by `limits`.
    pub(super) fn start(limits: &'l SearchLimits) -> Self {
        Self {
            limits,
            start: Instant::now(),
        }
    }

    /// Returns `true` once the node budget or the time budget is used up.
    ///
    /// The node budget is checked first and compares against
    /// [`SearchStats::nodes_expanded`] (`>=`), so a budget of `n` stops after the `n`-th
    /// expansion.
    pub(super) fn exhausted(&self, stats: &SearchStats) -> bool {
        let over_nodes = self
            .limits
            .node_budget
            .is_some_and(|max_nodes| stats.nodes_expanded >= max_nodes);

        over_nodes
            || self
                .limits
                .time_budget
                .is_some_and(|timeout| self.start.elapsed() >= timeout)
    }

    /// Ends the search with [`SearchOutcome::LimitReached`].
    pub(super) fn limit_reached(&self, stats: SearchStats) -> SearchOutcome {
        SearchOutcome::LimitReached(self.stamp(stats))
    }

    /// Ends the search with [`SearchOutcome::Unsolvable`].
    pub(super) fn unsolvable(&self, stats: SearchStats) -> SearchOutcome {
        SearchOutcome::Unsolvable(self.stamp(stats))
    }

    /// Ends the search with `plan`, recording its cost and length in the stats.
    pub(super) fn plan(&self, plan: Plan, mut stats: SearchStats) -> SearchOutcome {
        stats.plan_cost = plan.cost;
        stats.plan_length = plan.len();

        SearchOutcome::Plan(plan, self.stamp(stats))
    }

    /// Records the time spent so far in `stats`.
    fn stamp(&self, mut stats: SearchStats) -> SearchStats {
        stats.elapsed = self.start.elapsed();
        stats
    }
}

/// Builds a [`Plan`] that applies `ops` in order, summing their costs.
///
/// Operator ids not present in `task` are skipped.
pub(super) fn plan_from_ops(ops: &[OpId], task: &Task) -> Plan {
    let mut plan = Plan::new();
    let mut total_cost = 0.0;
    for &op_id in ops {
        if let Some(op) = task.operators.get(op_id.0) {
            plan.steps.push(PlanStep {
                op_id,
                op_name: op.name.clone(),
            });
            total_cost += f64::from(op.cost);
        }
    }

    plan.cost = total_cost;
    plan
}

/// Collects the operators on the path from `node` up to the root of `parents`.
///
/// The result is ordered from `node` towards the root: reverse it for a forward search tree,
/// use it as is for a backward (regression) tree. A node missing from `parents` yields an
/// empty path.
///
/// # Panics
///
/// Panics if a non-root entry has no parent, which [`ParentMap`] rules out.
pub(super) fn ops_to_root<T: Clone + Eq + Hash>(parents: &ParentMap<T>, node: &T) -> Vec<OpId> {
    let mut ops = Vec::new();
    let mut current = node.clone();
    while let Some((parent, op)) = parents.get(&current) {
        if *op == ROOT_OP {
            break;
        }

        ops.push(*op);
        current = parent.clone().expect("only the root has no parent");
    }

    ops
}

/// Joins the forward path to `state` and the backward path from `subgoal` into one plan.
///
/// `state` must satisfy `subgoal`, so the forward operators followed by the backward ones
/// lead from the initial state to the goal.
pub(super) fn plan_through(
    forward_parents: &ParentMap<State>,
    state: &State,
    backward_parents: &ParentMap<Subgoal>,
    subgoal: &Subgoal,
    task: &Task,
) -> Plan {
    let mut ops = ops_to_root(forward_parents, state);
    ops.reverse();
    ops.extend(ops_to_root(backward_parents, subgoal));

    plan_from_ops(&ops, task)
}

/// Rejects tasks with conditional effects, which regression-based planners cannot handle.
///
/// # Errors
///
/// Returns [`MiniplanError::UnsupportedConditionalEffects`] if any operator of `task` has a
/// conditional effect.
pub(super) fn reject_conditional_effects(task: &Task) -> Result<(), MiniplanError> {
    if task.operators.iter().any(|op| !op.conditional.is_empty()) {
        return Err(MiniplanError::UnsupportedConditionalEffects);
    }

    Ok(())
}
