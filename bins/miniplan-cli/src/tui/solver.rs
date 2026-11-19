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
