//! Tests that grounding rejects PDDL+ processes and events instead of dropping them.

use miniplan::error::MiniplanError;
use miniplan::ground::ground;
use miniplan::pddl_io::{load_domain_str, load_problem_str};

/// A problem for the domains below: one ball, which must end up on the floor.
const PROBLEM: &str = "(define (problem p) (:domain d) (:init) (:goal (on-floor)))";

/// Grounds `domain` against [`PROBLEM`] and returns the error message.
///
/// # Panics
///
/// Panics if either file fails to parse, or if grounding succeeds or fails
/// with anything but [`MiniplanError::Unsupported`].
fn unsupported_message(domain: &str) -> String {
    let domain = load_domain_str(domain).expect("domain should parse");
    let problem = load_problem_str(PROBLEM).expect("problem should parse");

    match ground(&domain, &problem) {
        Err(MiniplanError::Unsupported(message)) => message,
        other => panic!("expected an Unsupported error, got {other:?}"),
    }
}

/// An event makes the goal reachable on its own, so ignoring it would report a
/// solvable task as unsolvable.
#[test]
fn event_is_unsupported() {
    let domain = "(define (domain d) (:requirements :time) (:predicates (on-floor))
        (:event land :parameters () :precondition () :effect (on-floor)))";

    assert_eq!(unsupported_message(domain), "PDDL+ event `land`");
}

/// A process changes fluents continuously, which a classical task cannot model.
#[test]
fn process_is_unsupported() {
    let domain = "(define (domain d) (:requirements :time) (:predicates (on-floor))
        (:functions (height))
        (:process fall :parameters () :precondition ()
            :effect (decrease (height) (* #t 1))))";

    assert_eq!(unsupported_message(domain), "PDDL+ process `fall`");
}
