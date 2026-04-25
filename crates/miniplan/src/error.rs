use thiserror::Error;

#[derive(Debug, Error)]
pub enum MiniplanError {
    #[error("failed to parse PDDL: {0}")]
    Parse(String),

    #[error("unsupported PDDL requirement: {0}")]
    Unsupported(String),

    #[error("type mismatch: {0}")]
    TypeMismatch(String),

    #[error("grounding error: {0}")]
    Ground(String),

    #[error("search limit reached: {0}")]
    SearchLimit(String),

    #[error("no plan found")]
    NoPlan,

    #[error("planner '{planner}' cannot handle this task; missing capabilities: {missing:?}")]
    IncapablePlanner { planner: String, missing: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid planner name: {0}")]
    InvalidPlanner(String),

    #[error("invalid heuristic name: {0}")]
    InvalidHeuristic(String),
}
