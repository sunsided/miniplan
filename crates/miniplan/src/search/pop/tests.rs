//! Unit tests for [`Pop`] on small hand-written tasks.

use super::*;
use crate::error::MiniplanError;
use crate::plan::{PartialOrder, Plan};
use crate::search::test_support::{expect_plan, make_op, make_test_task};
use crate::task::CondEffect;

/// Solves `task` with the default ranking and returns the plan.
///
/// # Panics
///
/// Panics if no plan is found.
fn solve(task: &Task) -> Plan {
    let outcome = Pop::default()
        .solve(task, &SearchLimits::default())
        .expect("pop accepts the task");

    expect_plan(outcome).0
}

/// The step names of `plan`, in order.
fn names(plan: &Plan) -> Vec<&str> {
    plan.steps.iter().map(|s| s.op_name.as_str()).collect()
}

/// The ordering pairs of `plan`.
fn order(plan: &Plan) -> &[(usize, usize)] {
    &plan
        .partial_order
        .as_ref()
        .expect("pop reports the partial order")
        .before
}

/// A chain `a -> b -> c` needs both steps, in order.
#[test]
fn chain_is_totally_ordered() {
    let task = make_test_task(
        3,
        &[0],
        &[2],
        &[],
        vec![
            make_op(0, "ab", &[0], &[], &[1], &[0], 1),
            make_op(1, "bc", &[1], &[], &[2], &[1], 1),
        ],
    );

    let plan = solve(&task);

    assert_eq!(names(&plan), ["ab", "bc"]);
    assert_eq!(order(&plan), [(0, 1)]);
    assert!((plan.cost - 2.0).abs() < f64::EPSILON);
}

/// Two goals with unrelated achievers leave the steps unordered.
#[test]
fn independent_goals_stay_unordered() {
    let task = make_test_task(
        2,
        &[],
        &[0, 1],
        &[],
        vec![
            make_op(0, "make-0", &[], &[], &[0], &[], 1),
            make_op(1, "make-1", &[], &[], &[1], &[], 1),
        ],
    );

    let plan = solve(&task);

    assert_eq!(plan.len(), 2);
    assert_eq!(order(&plan), []);
}

/// A step that deletes the precondition of another must come after it (promotion or
/// demotion of a threat).
#[test]
fn threat_forces_an_order() {
    let task = make_test_task(
        4,
        &[3],
        &[1, 2],
        &[],
        vec![
            make_op(0, "spoil", &[], &[], &[1], &[3], 1),
            make_op(1, "use", &[3], &[], &[2], &[], 1),
        ],
    );

    let plan = solve(&task);

    assert_eq!(names(&plan), ["use", "spoil"]);
    assert_eq!(order(&plan), [(0, 1)]);
}

/// An operator that adds and deletes a fact ends with it false, so it cannot achieve it.
#[test]
fn delete_wins_over_add() {
    let task = make_test_task(
        2,
        &[],
        &[1],
        &[],
        vec![
            make_op(0, "flicker", &[], &[], &[1], &[1], 1),
            make_op(1, "make", &[], &[], &[1], &[], 5),
        ],
    );

    let plan = solve(&task);

    assert_eq!(names(&plan), ["make"]);
}

/// A negative goal is achieved by deleting the fact.
#[test]
fn negative_goal_is_achieved_by_a_delete() {
    let task = make_test_task(
        1,
        &[0],
        &[],
        &[0],
        vec![make_op(0, "drop", &[], &[], &[], &[0], 1)],
    );

    let plan = solve(&task);

    assert_eq!(names(&plan), ["drop"]);
}

/// The initial state already satisfying the goal yields the empty plan.
#[test]
fn satisfied_goal_gives_empty_plan() {
    let task = make_test_task(1, &[0], &[0], &[], vec![]);

    let plan = solve(&task);

    assert!(plan.is_empty());
    assert_eq!(plan.partial_order, Some(PartialOrder::default()));
}

/// A goal no relaxed plan reaches is reported as unsolvable.
#[test]
fn relaxed_unreachable_goal_is_unsolvable() {
    let task = make_test_task(
        2,
        &[],
        &[1],
        &[],
        vec![make_op(0, "never", &[0], &[], &[1], &[], 1)],
    );

    let outcome = Pop::default()
        .solve(&task, &SearchLimits::default())
        .expect("pop accepts the task");

    assert!(
        matches!(outcome, SearchOutcome::Unsolvable(_)),
        "{outcome:?}"
    );
}

/// Conditional effects are rejected.
#[test]
fn conditional_effects_are_rejected() {
    let mut op = make_op(0, "cond", &[], &[], &[], &[], 1);
    op.conditional.push(CondEffect {
        cond_pos: op.add.clone(),
        cond_neg: op.add.clone(),
        add: op.add.clone(),
        del: op.add.clone(),
    });
    let task = make_test_task(1, &[], &[0], &[], vec![op]);

    let result = Pop::default().solve(&task, &SearchLimits::default());

    assert!(matches!(
        result,
        Err(MiniplanError::UnsupportedConditionalEffects)
    ));
}

/// Every ranking finds a plan for the threat task.
#[test]
fn every_ranking_solves() {
    let task = make_test_task(
        4,
        &[3],
        &[1, 2],
        &[],
        vec![
            make_op(0, "spoil", &[], &[], &[1], &[3], 1),
            make_op(1, "use", &[3], &[], &[2], &[], 1),
        ],
    );

    for name in ["hadd", "hff", "hmax", "goal-count", "blind", "zero"] {
        let ranking = PopRanking::from_name(name).expect("built-in name");
        let outcome = Pop::new(ranking)
            .solve(&task, &SearchLimits::default())
            .expect("pop accepts the task");

        assert_eq!(names(&expect_plan(outcome).0), ["use", "spoil"], "{name}");
    }
}

/// Zero-cost operators still yield a plan, and the plan reports their real cost of 0.
#[test]
fn zero_cost_steps_are_planned_at_their_real_cost() {
    let task = make_test_task(
        3,
        &[0],
        &[2],
        &[],
        vec![
            make_op(0, "free-a", &[0], &[], &[1], &[], 0),
            make_op(1, "free-b", &[1], &[], &[2], &[], 0),
        ],
    );

    let plan = solve(&task);

    assert_eq!(names(&plan), ["free-a", "free-b"]);
    assert!(plan.cost.abs() < f64::EPSILON);
}
