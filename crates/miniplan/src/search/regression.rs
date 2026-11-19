//! Backward regression over subgoals, shared by the bidirectional planners.
//!
//! A backward search does not walk over complete states. It walks over [`Subgoal`]s: partial
//! states that list the facts that must hold and the facts that must not hold. Regressing a
//! subgoal through an operator yields the weakest subgoal from which applying that operator
//! reaches the original one.

use crate::task::{Operator, State};

/// A partial state as `(must_hold, must_not_hold)`, both over the task's fact indices.
///
/// A complete state `s` satisfies it when `s` contains every fact of `must_hold` and none of
/// `must_not_hold` (see [`State::satisfies`]).
pub(super) type Subgoal = (State, State);

/// Returns `true` if `op` can be the last step towards `subgoal`: it adds a required fact
/// or deletes a forbidden one.
fn is_relevant(op: &Operator, subgoal: &Subgoal) -> bool {
    let (must_hold, must_not_hold) = subgoal;

    op.add.0.ones().any(|b| must_hold.0.contains(b))
        || op.del.0.ones().any(|b| must_not_hold.0.contains(b))
}

/// Returns `true` if applying `op` cannot break `subgoal`: it deletes no required fact and
/// adds no forbidden one.
fn is_consistent(op: &Operator, subgoal: &Subgoal) -> bool {
    let (must_hold, must_not_hold) = subgoal;

    !op.del.0.ones().any(|b| must_hold.0.contains(b))
        && !op.add.0.ones().any(|b| must_not_hold.0.contains(b))
}

/// Returns `true` if `subgoal` can be regressed through `op`: the operator is both relevant
/// to and consistent with it.
pub(super) fn can_regress(op: &Operator, subgoal: &Subgoal) -> bool {
    is_relevant(op, subgoal) && is_consistent(op, subgoal)
}

/// Regresses `subgoal` through `op`.
///
/// The result requires `op`'s preconditions plus every part of `subgoal` that `op` does not
/// establish itself. Both halves are sized for `num_facts` facts. Only meaningful when
/// [`can_regress`] holds.
pub(super) fn regress(op: &Operator, subgoal: &Subgoal, num_facts: usize) -> Subgoal {
    let (must_hold, must_not_hold) = subgoal;
    let mut new_hold = State::new(num_facts);
    let mut new_not_hold = State::new(num_facts);

    for bit in must_hold.0.ones() {
        if !op.add.0.contains(bit) {
            new_hold.0.set(bit, true);
        }
    }

    for bit in op.pre_pos.0.ones() {
        new_hold.0.set(bit, true);
    }

    for bit in must_not_hold.0.ones() {
        if !op.del.0.contains(bit) {
            new_not_hold.0.set(bit, true);
        }
    }

    for bit in op.pre_neg.0.ones() {
        new_not_hold.0.set(bit, true);
    }

    (new_hold, new_not_hold)
}

/// The largest per-fact cost among the facts set in `facts`, or `0.0` if none is set.
///
/// `fact_costs` is indexed by fact and holds relaxed reachability costs, with
/// `f64::INFINITY` for unreachable facts. Returns `f64::INFINITY` as soon as one set fact is
/// unreachable, which the planners use to prune dead subgoals.
pub(super) fn max_fact_cost(facts: &State, fact_costs: &[f64]) -> f64 {
    let mut max_cost = 0.0;
    for b in facts.0.ones() {
        let c = fact_costs[b];
        if c == f64::INFINITY {
            return f64::INFINITY;
        }

        if c > max_cost {
            max_cost = c;
        }
    }

    max_cost
}
