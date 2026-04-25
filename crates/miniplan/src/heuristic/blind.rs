use crate::Heuristic;
use crate::search::HValue;
use crate::task::{State, Task};

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
