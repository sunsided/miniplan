use crate::search::HValue;
use crate::search::Heuristic;
use crate::task::{State, Task};

/// A heuristic that counts the number of unsatisfied positive goal facts.
///
/// This is a simple, fast heuristic that is not admissible in general
/// (it underestimates the true cost when multiple goals interact).
///
/// # Examples
///
/// ```
/// use miniplan::heuristic::GoalCountHeuristic;
/// use miniplan::search::Heuristic;
///
/// assert_eq!(GoalCountHeuristic.name(), "goal-count");
/// ```
pub struct GoalCountHeuristic;

impl Heuristic for GoalCountHeuristic {
    fn name(&self) -> &str {
        "goal-count"
    }

    fn estimate(&self, task: &Task, state: &State) -> HValue {
        let mut count = 0;
        for bit in task.goal_pos.0.ones() {
            if !state.0.contains(bit) {
                count += 1;
            }
        }
        HValue(count as f64)
    }
}
