use crate::search::HValue;
use crate::search::Heuristic;
use crate::task::{State, Task};

/// A blind heuristic that returns 0 at the goal and 1 otherwise.
///
/// This is the simplest admissible heuristic, equivalent to
/// treating the search as uniform-cost with goal checking.
///
/// # Examples
///
/// ```
/// use miniplan::heuristic::BlindHeuristic;
/// use miniplan::search::Heuristic;
///
/// assert_eq!(BlindHeuristic.name(), "blind");
/// ```
pub struct BlindHeuristic;

impl Heuristic for BlindHeuristic {
    fn name(&self) -> &str {
        "blind"
    }

    fn estimate(&self, task: &Task, state: &State) -> HValue {
        if state.satisfies(&task.goal_pos, &task.goal_neg) {
            HValue(0.0)
        } else {
            HValue(1.0)
        }
    }
}
