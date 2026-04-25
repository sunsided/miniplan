//! Heuristic functions for estimating distance to goal.
//!
//! Heuristics are used by planners like A*, GBFS, NBS, and BAE* to guide
//! their search. Each heuristic implements the [`Heuristic`] trait.
//!
//! # Available heuristics
//!
//! - [`BlindHeuristic`] — returns 0 at goal, 1 otherwise.
//! - [`GoalCountHeuristic`] — counts unsatisfied goal facts.
//! - [`HAdd`] — additive heuristic (Bonet & Geffner, 2001).
//! - [`HMax`] — max heuristic (Bonet & Geffner, 2001).
//! - [`HFF`] — FF heuristic (Hoffmann & Nebel, 2001).
//! - [`HZero`] — always returns 0 (uninformed).
//!
//! # Examples
//!
//! ```
//! use miniplan::heuristic::HFF;
//! use miniplan::search::Heuristic;
//!
//! assert_eq!(HFF.name(), "hff");
//! ```

mod blind;
mod goal_count;
mod relaxed;
mod zero;

pub use blind::BlindHeuristic;
pub use goal_count::GoalCountHeuristic;
pub use relaxed::{HAdd, HFF, HMax};
pub use zero::HZero;

pub use crate::search::Heuristic;

pub(crate) use relaxed::{rpg_backward_fact_costs, rpg_fact_costs};
