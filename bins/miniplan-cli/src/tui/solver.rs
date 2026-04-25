use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use miniplan::MiniplanError;
use miniplan::search::{PlannerChoice, SearchLimits, SearchOutcome, Solver};
use miniplan::task::Task;

pub enum SolverEvent {
    Done(Result<SearchOutcome, MiniplanError>),
}

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
        let outcome = match result {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(e)) => Err(e),
            Err(panic) => {
                let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "solver panicked with unknown reason".to_string()
                };
                Err(MiniplanError::SearchLimit(msg))
            }
        };
        let _ = tx.send(SolverEvent::Done(outcome));
    });
    rx
}
