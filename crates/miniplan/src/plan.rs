use std::fmt;

use crate::task::OpId;

#[derive(Debug, Clone)]
pub struct PlanStep {
    pub op_id: OpId,
    pub op_name: String,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
    pub cost: f64,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            cost: 0.0,
        }
    }
}

impl Plan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "; cost = {}", self.cost)?;
        writeln!(f, "; length = {}", self.len())?;
        for step in &self.steps {
            writeln!(f, "({})", step.op_name)?;
        }
        Ok(())
    }
}
