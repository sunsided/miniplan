//! Background solver thread for the TUI, so a long search never blocks the event loop.

use std::any::Any;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use miniplan::error::MiniplanError;
use miniplan::search::{PlannerChoice, SearchLimits, SearchOutcome, Solver};
use miniplan::task::Task;

/// A message from the solver thread to the UI.
pub enum SolverEvent {
    /// The search finished; a planner panic is reported as [`MiniplanError::SearchLimit`].
    Done(Result<SearchOutcome, MiniplanError>),
}

/// Starts a search for `task` on a new thread and returns the channel its result arrives on.
///
/// Exactly one [`SolverEvent::Done`] is sent. Dropping the receiver cancels nothing: the
/// thread runs to completion and its send fails silently.
pub fn spawn_solver(
    task: Task,
    choice: PlannerChoice,
    limits: SearchLimits,
) -> Receiver<SolverEvent> {
    let (tx, rx): (Sender<SolverEvent>, Receiver<SolverEvent>) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let solver = Solver::new();
            solver.solve_task(&task, &choice, &limits)
        }));
        let outcome = result.unwrap_or_else(|panic| Err(panic_error(panic.as_ref())));
        let _ = tx.send(SolverEvent::Done(outcome));
    });

    rx
}

/// Turns a caught panic payload into an error that carries the panic message.
fn panic_error(panic: &(dyn Any + Send)) -> MiniplanError {
    let msg = if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "solver panicked with unknown reason".to_string()
    };

    MiniplanError::SearchLimit(msg)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use miniplan::ground::ground;
    use miniplan::search::PlannerKind;

    use super::*;
    use crate::tui::test_support::bundle;

    /// How long a test waits for the solver thread before failing instead of hanging.
    const RECV_TIMEOUT: Duration = Duration::from_secs(20);

    /// The grounded `gripper-2` task from `gripper-small.pddl`.
    fn gripper_task() -> Task {
        let bundle = bundle(&["gripper-small.pddl"]);
        ground(&bundle.domains[0].1, &bundle.problems[0].1).expect("gripper grounds")
    }

    /// Spawns a BFS search for the gripper task with `limits` and waits for its one result.
    fn solve_in_background(limits: SearchLimits) -> Result<SearchOutcome, MiniplanError> {
        let rx = spawn_solver(gripper_task(), PlannerChoice::new(PlannerKind::Bfs), limits);
        let SolverEvent::Done(result) = rx.recv_timeout(RECV_TIMEOUT).expect("solver answers");

        assert!(rx.recv().is_err(), "the channel closes after one event");
        result
    }

    /// An unlimited search on the background thread sends the optimal seven-step plan.
    #[test]
    fn spawned_search_sends_plan() {
        let result = solve_in_background(SearchLimits::default());

        let Ok(SearchOutcome::Plan(plan, _)) = result else {
            panic!("expected a plan");
        };
        assert_eq!(plan.len(), 7);
    }

    /// A node budget of one ends the search with [`SearchOutcome::LimitReached`].
    #[test]
    fn spawned_search_reports_limit() {
        let limits = SearchLimits {
            node_budget: Some(1),
            ..SearchLimits::default()
        };

        let result = solve_in_background(limits);

        assert!(matches!(result, Ok(SearchOutcome::LimitReached(_))));
    }

    /// Panic payloads of type `&str` and `String` keep their message; others get a fallback.
    #[test]
    fn panic_error_keeps_message() {
        let cases: [(Box<dyn Any + Send>, &str); 3] = [
            (Box::new("static message"), "static message"),
            (Box::new(String::from("owned message")), "owned message"),
            (Box::new(42_u8), "solver panicked with unknown reason"),
        ];

        for (payload, expected) in cases {
            let MiniplanError::SearchLimit(msg) = panic_error(payload.as_ref()) else {
                panic!("expected a SearchLimit error");
            };

            assert_eq!(msg, expected);
        }
    }
}
