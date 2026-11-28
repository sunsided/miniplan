//! Integration tests for the partial-order planner [`Pop`].
//!
//! POP proves unsolvability only when the relaxation shows the goal unreachable. On the
//! generated cases it therefore runs unbounded on solvable tasks (best-first over plan cost
//! with costs of at least 1 is complete) and with a small node budget on unsolvable ones,
//! where it must not return a plan. Every plan it returns is checked twice: as returned,
//! and in a second linearization of its partial order, which must be valid too.

use miniplan::ground::ground;
use miniplan::pddl_io::load_combined_str;
use miniplan::plan::Plan;
use miniplan::search::{Planner, Pop, PopRanking, SearchLimits, SearchOutcome};
use miniplan::task::Task;
use proptest::prelude::*;

use crate::plan_validity::{cheapest_cost, graph_case, ground_case, plan_reaches_goal, prop_case};

/// Node budget for POP on unsolvable generated cases. Small, because POP keeps growing
/// partial plans there until it runs out.
const UNSOLVABLE_NODE_BUDGET: u64 = 200;

/// The blocksworld example.
const BLOCKSWORLD: &str = include_str!("../../../../examples/pddl/blocksworld.pddl");

/// The Sussman anomaly: C on A, A and B on the table; goal A on B on C. Solving either goal
/// first undoes the other, so a planner has to interleave the work for both.
const SUSSMAN: &str = "
(define (domain bw)
    (:requirements :strips)
    (:predicates (on ?x ?y) (ontable ?x) (clear ?x) (handempty) (holding ?x))
    (:action pick-up :parameters (?x)
        :precondition (and (clear ?x) (ontable ?x) (handempty))
        :effect (and (not (ontable ?x)) (not (clear ?x)) (not (handempty)) (holding ?x)))
    (:action put-down :parameters (?x)
        :precondition (holding ?x)
        :effect (and (not (holding ?x)) (clear ?x) (handempty) (ontable ?x)))
    (:action stack :parameters (?x ?y)
        :precondition (and (holding ?x) (clear ?y))
        :effect (and (not (holding ?x)) (not (clear ?y)) (clear ?x) (handempty) (on ?x ?y)))
    (:action unstack :parameters (?x ?y)
        :precondition (and (on ?x ?y) (clear ?x) (handempty))
        :effect (and (holding ?x) (clear ?y) (not (clear ?x)) (not (handempty))
                     (not (on ?x ?y)))))
(define (problem sussman) (:domain bw)
    (:objects a b c)
    (:init (on c a) (ontable a) (ontable b) (clear c) (clear b) (handempty))
    (:goal (and (on a b) (on b c))))
";

/// What POP did on one task.
#[derive(Debug)]
enum PopResult {
    /// A plan.
    Plan(Plan),
    /// Proven unsolvable.
    Unsolvable,
    /// Ran into the node budget.
    LimitReached,
}

/// Runs POP with the default ranking on `task`, within `node_budget` expansions if given.
///
/// # Panics
///
/// Panics if POP rejects the task.
fn run_pop(task: &Task, node_budget: Option<u64>) -> PopResult {
    let limits = SearchLimits {
        node_budget,
        ..SearchLimits::default()
    };

    match Pop::new(PopRanking::HAdd).solve(task, &limits) {
        Ok(SearchOutcome::Plan(plan, _)) => PopResult::Plan(plan),
        Ok(SearchOutcome::Unsolvable(_)) => PopResult::Unsolvable,
        Ok(SearchOutcome::LimitReached(_)) => PopResult::LimitReached,
        other => panic!("pop returned {other:?}"),
    }
}

/// The steps of `plan` reordered by a topological sort of its partial order that prefers
/// the *latest* ready step, so it differs from the returned order wherever the partial
/// order allows.
fn reverse_linearization(plan: &Plan) -> Plan {
    let order = &plan
        .partial_order
        .as_ref()
        .expect("pop reports an order")
        .before;
    let n = plan.len();
    let mut placed = vec![false; n];
    let mut reordered = Plan {
        steps: Vec::with_capacity(n),
        ..plan.clone()
    };

    while reordered.steps.len() < n {
        let ready = (0..n)
            .rev()
            .find(|&j| !placed[j] && order.iter().all(|&(i, k)| k != j || placed[i]))
            .expect("the order is acyclic");
        placed[ready] = true;
        reordered.steps.push(plan.steps[ready].clone());
    }

    reordered
}

/// Whether `plan` reports a well-formed partial order whose two linearizations both reach
/// the goal of `task`.
fn partial_order_is_sound(task: &Task, plan: &Plan) -> bool {
    let Some(order) = &plan.partial_order else {
        return false;
    };

    let well_formed = order.before.iter().all(|&(i, j)| i < j && j < plan.len());

    well_formed
        && plan_reaches_goal(task, plan)
        && plan_reaches_goal(task, &reverse_linearization(plan))
}

/// Checks POP's result on `task` against the oracle's verdict `solvable`.
fn check_pop(task: &Task, solvable: bool) -> Result<(), TestCaseError> {
    let budget = (!solvable).then_some(UNSOLVABLE_NODE_BUDGET);

    match run_pop(task, budget) {
        PopResult::Plan(plan) => {
            prop_assert!(solvable, "pop planned an unsolvable task: {plan:?}");
            prop_assert!(partial_order_is_sound(task, &plan), "unsound plan {plan:?}");
        }
        PopResult::Unsolvable => prop_assert!(!solvable, "pop gave up on a solvable task"),
        PopResult::LimitReached => prop_assert!(!solvable, "pop ran out on a solvable task"),
    }

    Ok(())
}

proptest! {
    /// On graph walks POP plans exactly the solvable cases, with sound partial orders.
    #[test]
    fn pop_agrees_with_oracle_on_graph_walks(case in graph_case()) {
        let task = ground_case(&case);
        check_pop(&task, case.shortest_path_len().is_some())?;
    }

    /// On propositional tasks with negative preconditions and costs, POP plans exactly the
    /// solvable cases, with sound partial orders.
    #[test]
    fn pop_agrees_with_oracle_on_propositional_tasks(case in prop_case()) {
        let (domain, problem) =
            load_combined_str(&case.to_pddl()).expect("generated PDDL parses");
        let task = ground(&domain, &problem).expect("generated problem grounds");
        check_pop(&task, cheapest_cost(&task).is_some())?;
    }
}

/// Grounds the combined domain-and-problem source `src`.
fn ground_src(src: &str) -> Task {
    let (domain, problem) = load_combined_str(src).expect("source parses");

    ground(&domain, &problem).expect("source grounds")
}

/// POP solves the Sussman anomaly, which needs interleaved subgoals.
#[test]
fn pop_solves_sussman_anomaly() {
    let task = ground_src(SUSSMAN);

    let PopResult::Plan(plan) = run_pop(&task, None) else {
        panic!("pop found no plan for the Sussman anomaly");
    };

    assert!(partial_order_is_sound(&task, &plan), "{plan}");
    assert_eq!(plan.len(), 6, "{plan}");
}

/// POP solves the blocksworld example.
#[test]
fn pop_solves_blocksworld_example() {
    let task = ground_src(BLOCKSWORLD);

    let PopResult::Plan(plan) = run_pop(&task, None) else {
        panic!("pop found no plan for blocksworld");
    };

    assert!(partial_order_is_sound(&task, &plan), "{plan}");
}
