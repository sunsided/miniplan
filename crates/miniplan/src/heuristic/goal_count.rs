use crate::Heuristic;
use crate::search::HValue;
use crate::task::{State, Task};

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
