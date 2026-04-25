use crate::search::Heuristic;
use crate::search::HValue;
use crate::task::{State, Task};

/// The h^0 (zero) heuristic — always returns 0.
///
/// This reduces A* to uniform-cost search (Dijkstra's algorithm).
/// Admissible but provides no guidance.
///
/// # Examples
///
/// ```
/// use miniplan::heuristic::HZero;
/// use miniplan::search::Heuristic;
///
/// assert_eq!(HZero.name(), "zero");
/// ```
pub struct HZero;

impl Heuristic for HZero {
    fn name(&self) -> &str {
        "zero"
    }

    fn estimate(&self, _task: &Task, _state: &State) -> HValue {
        HValue(0.0)
    }
}
