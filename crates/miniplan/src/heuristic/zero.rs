use crate::Heuristic;
use crate::search::HValue;
use crate::task::{State, Task};

pub struct HZero;

impl Heuristic for HZero {
    fn name(&self) -> &str {
        "zero"
    }

    fn estimate(&self, _task: &Task, _state: &State) -> HValue {
        HValue(0.0)
    }
}
