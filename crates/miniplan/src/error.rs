//! Error types for parsing, grounding, and search operations.

use thiserror::Error;

/// Errors that can occur during planning operations.
#[derive(Debug, Error)]
pub enum MiniplanError {
    /// Failed to parse PDDL input.
    #[error("failed to parse PDDL: {0}")]
    Parse(String),

    /// A PDDL requirement is not supported.
    #[error("unsupported PDDL requirement: {0}")]
    Unsupported(String),

    /// A type mismatch was detected.
    #[error("type mismatch: {0}")]
    TypeMismatch(String),

    /// An error occurred during grounding.
    #[error("grounding error: {0}")]
    Ground(String),

    /// A search limit was reached.
    #[error("search limit reached: {0}")]
    SearchLimit(String),

    /// No plan was found.
    #[error("no plan found")]
    NoPlan,

    /// The planner cannot handle the given task.
    #[error("planner '{planner}' cannot handle this task; missing capabilities: {missing:?}")]
    IncapablePlanner {
        /// Name of the planner.
        planner: String,
        /// Description of missing capabilities.
        missing: String,
    },

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The planner name is not registered.
    #[error("invalid planner name: {0}")]
    InvalidPlanner(String),

    /// The heuristic name is not registered.
    #[error("invalid heuristic name: {0}")]
    InvalidHeuristic(String),

    /// Bidirectional search doesn't support conditional effects.
    #[error("bidirectional search does not support operators with conditional effects")]
    UnsupportedConditionalEffects,
}
