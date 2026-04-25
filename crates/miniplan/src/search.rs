//! Search algorithms for automated planning.
//!
//! This module provides a collection of search algorithms (planners) for solving
//! PDDL planning tasks. It includes unidirectional planners like BFS and A*,
//! as well as bidirectional planners like BiDij, NBS, and BAE*.
//!
//! # Entry points
//!
//! - [`Registry`] — a plugin-style registry of planners and heuristics.
//!   Use [`Registry::with_builtins`] to get all built-in algorithms.
//! - [`Solver`] — high-level solver that uses a [`Registry`] to build and run planners.
//!
//! # Using the registry
//!
//! ```
//! use miniplan::search::Registry;
//!
//! let registry = Registry::with_builtins();
//! let planners: Vec<&str> = registry.planners().map(|r| r.name).collect();
//! assert!(planners.contains(&"bfs"));
//! assert!(planners.contains(&"astar"));
//! ```
//!
//! # Planner traits
//!
//! The [`Planner`] trait defines the interface all planners implement.
//! Each planner reports its [`PlannerCapabilities`] and can solve a [`Task`].
//! within given [`SearchLimits`], returning a [`SearchOutcome`].

mod astar;
mod bae;
mod bfs;
mod bibfs_uc;
mod bidij;
mod gbfs;
mod nbs;

use std::time::Duration;

use bitflags::bitflags;

use crate::error::MiniplanError;
use crate::plan::Plan;
use crate::task::{State, Task};

pub use crate::task::OpId;
pub use astar::Astar;
pub use bae::Bae;
pub use bfs::Bfs;
pub use bibfs_uc::BibfsUc;
pub use bidij::BiDij;
pub use gbfs::Gbfs;
pub use nbs::Nbs;

/// A heuristic value returned by [`Heuristic::estimate`].
///
/// Wraps an `f64` and supports comparison, hashing, and an `INFINITY` constant.
///
/// # Examples
///
/// ```
/// use miniplan::search::HValue;
///
/// let h = HValue(3.5);
/// assert!(h.is_finite());
/// assert!(HValue::INFINITY.is_finite() == false);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct HValue(pub f64);

impl HValue {
    /// An infinite heuristic value.
    pub const INFINITY: HValue = HValue(f64::INFINITY);

    /// Returns `true` if this value is finite (not infinity or NaN).
    pub fn is_finite(&self) -> bool {
        self.0.is_finite()
    }
}

impl PartialEq for HValue {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialOrd for HValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl std::hash::Hash for HValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// Capability flags describing what features a planner supports.
    ///
    /// Use bitwise operations to combine flags when checking if a planner
    /// can handle a given task.
    ///
    /// # Examples
    ///
    /// ```
    /// use miniplan::search::PlannerCapabilities;
    ///
    /// let caps = PlannerCapabilities::CLASSICAL | PlannerCapabilities::ACTION_COSTS;
    /// assert!(caps.contains(PlannerCapabilities::CLASSICAL));
    /// ```
    pub struct PlannerCapabilities: u32 {
        /// Classical STRIPS planning (positive preconditions, add/delete effects).
        const CLASSICAL           = 1 << 0;
        /// Negative preconditions (NOT predicates).
        const NEGATIVE_PRECONDS   = 1 << 1;
        /// Disjunctive preconditions or effects.
        const DISJUNCTIVE         = 1 << 2;
        /// Quantified preconditions (forall, exists).
        const QUANTIFIED_PRECONS  = 1 << 3;
        /// Conditional effects (when ... then ...).
        const CONDITIONAL_EFFECTS = 1 << 4;
        /// Action costs (non-uniform cost planning).
        const ACTION_COSTS        = 1 << 5;
        /// Guarantees optimal (minimum-cost) plans.
        const OPTIMAL             = 1 << 6;
    }
}

/// Limits that control search termination.
///
/// Any limit that is `None` is unbounded. When a limit is reached,
/// the planner returns [`SearchOutcome::LimitReached`].
///
/// # Examples
///
/// ```
/// use miniplan::search::SearchLimits;
/// use std::time::Duration;
///
/// let limits = SearchLimits {
///     time_budget: Some(Duration::from_secs(60)),
///     node_budget: Some(1_000_000),
///     memory_mb: None,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct SearchLimits {
    /// Maximum wall-clock time for the search.
    pub time_budget: Option<Duration>,
    /// Maximum number of nodes to expand.
    pub node_budget: Option<u64>,
    /// Maximum memory usage in megabytes.
    pub memory_mb: Option<u64>,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            time_budget: Some(Duration::from_secs(300)),
            node_budget: None,
            memory_mb: None,
        }
    }
}

/// The result of a search operation.
///
/// This enum is `#[non_exhaustive]` — use a wildcard arm (`_`) when matching.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SearchOutcome {
    /// A valid plan was found.
    Plan(Plan, SearchStats),
    /// The task is provably unsolvable.
    Unsolvable(SearchStats),
    /// A search limit was reached before a conclusion could be drawn.
    LimitReached(SearchStats),
}

/// Statistics collected during a search.
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    /// Number of states expanded (popped from the open list).
    pub nodes_expanded: u64,
    /// Number of states generated (successors created).
    pub nodes_generated: u64,
    /// Total cost of the found plan (0.0 if no plan).
    pub plan_cost: f64,
    /// Number of steps in the found plan (0 if no plan).
    pub plan_length: usize,
    /// Wall-clock time spent searching.
    pub elapsed: Duration,
}

/// A heuristic function for estimating the cost to reach the goal.
///
/// Implementors must be `Send + Sync` for use in multi-threaded contexts.
pub trait Heuristic: Send + Sync {
    /// A human-readable name for this heuristic.
    fn name(&self) -> &str;

    /// Estimate the cost from `state` to the goal in `task`.
    fn estimate(&self, task: &Task, state: &State) -> HValue;

    /// Return preferred operators for the current state (used by some planners).
    /// The default implementation returns an empty slice.
    fn preferred_ops(&self, _task: &Task, _state: &State) -> &[OpId] {
        &[]
    }
}

/// A search algorithm that can solve a planning task.
pub trait Planner: Send {
    /// A human-readable name for this planner.
    fn name(&self) -> &str;

    /// A short description of the algorithm.
    fn describe(&self) -> &str {
        ""
    }

    /// The capabilities this planner supports.
    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
    }

    /// Solve the given `task` within the specified `limits`.
    fn solve(&mut self, task: &Task, limits: &SearchLimits)
    -> Result<SearchOutcome, MiniplanError>;
}

/// Configuration options passed to planner/heuristic factories.
#[derive(Default, Clone)]
pub struct PlannerConfig {
    /// Key-value options. Interpretation depends on the planner.
    pub opts: rustc_hash::FxHashMap<String, String>,
}

/// A planner registered in the [`Registry`].
pub struct RegisteredPlanner {
    /// Unique name used to look up this planner.
    pub name: &'static str,
    /// Short description of the algorithm.
    pub description: &'static str,
    /// Capability flags.
    pub capabilities: PlannerCapabilities,
    /// Factory function that constructs a [`Planner`] instance.
    pub factory: PlannerFactory,
}

/// A heuristic registered in the [`Registry`].
pub struct RegisteredHeuristic {
    /// Unique name used to look up this heuristic.
    pub name: &'static str,
    /// Factory function that constructs a [`Heuristic`] instance.
    pub factory: HeuristicFactory,
}

/// A factory function type for constructing a [`Planner`].
///
/// Takes a [`PlannerConfig`] and returns a boxed planner or an error.
pub type PlannerFactory =
    std::sync::Arc<dyn Fn(&PlannerConfig) -> Result<Box<dyn Planner>, MiniplanError> + Send + Sync>;

/// A factory function type for constructing a [`Heuristic`].
///
/// Takes a [`PlannerConfig`] and returns a boxed heuristic or an error.
pub type HeuristicFactory = std::sync::Arc<
    dyn Fn(&PlannerConfig) -> Result<Box<dyn Heuristic>, MiniplanError> + Send + Sync,
>;

/// A registry of planners and heuristics.
///
/// Use [`Registry::with_builtins`] to create a registry with all built-in
/// algorithms, then add custom ones with [`register_planner`](Self::register_planner)
/// and [`register_heuristic`](Self::register_heuristic).
pub struct Registry {
    planners: rustc_hash::FxHashMap<String, RegisteredPlanner>,
    heuristics: rustc_hash::FxHashMap<String, RegisteredHeuristic>,
}

impl Registry {
    /// Create a registry populated with all built-in planners and heuristics.
    pub fn with_builtins() -> Self {
        let mut r = Self {
            planners: rustc_hash::FxHashMap::default(),
            heuristics: rustc_hash::FxHashMap::default(),
        };
        r.register_builtins();
        r
    }

    fn register_builtins(&mut self) {
        use crate::heuristic::{BlindHeuristic, GoalCountHeuristic, HAdd, HFF, HMax, HZero};
        use crate::search::astar::Astar;
        use crate::search::bfs::Bfs;
        use crate::search::bibfs_uc::BibfsUc;
        use crate::search::bidij::BiDij;
        use crate::search::gbfs::Gbfs;

        self.register_planner(RegisteredPlanner {
            name: "bfs",
            description: "Breadth-first search",
            capabilities: PlannerCapabilities::CLASSICAL | PlannerCapabilities::NEGATIVE_PRECONDS,
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(Bfs::new()))),
        });

        self.register_planner(RegisteredPlanner {
            name: "astar",
            description: "A* search with pluggable heuristic",
            capabilities: PlannerCapabilities::CLASSICAL
                | PlannerCapabilities::NEGATIVE_PRECONDS
                | PlannerCapabilities::CONDITIONAL_EFFECTS
                | PlannerCapabilities::ACTION_COSTS,
            factory: std::sync::Arc::new(|_cfg| {
                let h = Box::new(HFF);
                Ok(Box::new(Astar::new(h)))
            }),
        });

        self.register_planner(RegisteredPlanner {
            name: "gbfs",
            description: "Greedy best-first search",
            capabilities: PlannerCapabilities::CLASSICAL
                | PlannerCapabilities::NEGATIVE_PRECONDS
                | PlannerCapabilities::CONDITIONAL_EFFECTS,
            factory: std::sync::Arc::new(|_cfg| {
                let h = Box::new(HFF);
                Ok(Box::new(Gbfs::new(h)))
            }),
        });

        self.register_planner(RegisteredPlanner {
            name: "bibfs-uc",
            description: "Bidirectional BFS (uniform-cost, not cost-aware)",
            capabilities: PlannerCapabilities::CLASSICAL | PlannerCapabilities::NEGATIVE_PRECONDS,
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(BibfsUc::new()))),
        });

        self.register_planner(RegisteredPlanner {
            name: "bidij",
            description: "Bidirectional Dijkstra (cost-aware)",
            capabilities: PlannerCapabilities::CLASSICAL
                | PlannerCapabilities::NEGATIVE_PRECONDS
                | PlannerCapabilities::ACTION_COSTS,
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(BiDij::new()))),
        });

        self.register_planner(RegisteredPlanner {
            name: "nbs",
            description: "Near-Optimal Bidirectional Search (Chen et al. 2017)",
            capabilities: PlannerCapabilities::CLASSICAL
                | PlannerCapabilities::NEGATIVE_PRECONDS
                | PlannerCapabilities::ACTION_COSTS,
            factory: std::sync::Arc::new(|cfg| {
                let h_name = cfg
                    .opts
                    .get("heuristic")
                    .map(|s| s.as_str())
                    .unwrap_or("hff");
                let h: Box<dyn crate::search::Heuristic> = match h_name {
                    "hadd" => Box::new(HAdd),
                    "hmax" => Box::new(HMax),
                    "hff" => Box::new(HFF),
                    "blind" => Box::new(crate::heuristic::BlindHeuristic),
                    "zero" => Box::new(HZero),
                    _ => Box::new(HFF),
                };
                Ok(Box::new(nbs::Nbs::new(h)))
            }),
        });

        self.register_planner(RegisteredPlanner {
            name: "bae",
            description: "Bidirectional A* with Error (BAE*, Sadhukhan 2013)",
            capabilities: PlannerCapabilities::CLASSICAL
                | PlannerCapabilities::NEGATIVE_PRECONDS
                | PlannerCapabilities::ACTION_COSTS,
            factory: std::sync::Arc::new(|cfg| {
                let h_name = cfg
                    .opts
                    .get("heuristic")
                    .map(|s| s.as_str())
                    .unwrap_or("hff");
                let h: Box<dyn crate::search::Heuristic> = match h_name {
                    "hadd" => Box::new(HAdd),
                    "hmax" => Box::new(HMax),
                    "hff" => Box::new(HFF),
                    "blind" => Box::new(crate::heuristic::BlindHeuristic),
                    "zero" => Box::new(HZero),
                    _ => Box::new(HFF),
                };
                Ok(Box::new(bae::Bae::new(h)))
            }),
        });

        self.register_heuristic(RegisteredHeuristic {
            name: "blind",
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(BlindHeuristic))),
        });

        self.register_heuristic(RegisteredHeuristic {
            name: "goal-count",
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(GoalCountHeuristic))),
        });

        self.register_heuristic(RegisteredHeuristic {
            name: "hadd",
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(HAdd))),
        });

        self.register_heuristic(RegisteredHeuristic {
            name: "hmax",
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(HMax))),
        });

        self.register_heuristic(RegisteredHeuristic {
            name: "hff",
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(HFF))),
        });

        self.register_heuristic(RegisteredHeuristic {
            name: "zero",
            factory: std::sync::Arc::new(|_cfg| Ok(Box::new(HZero))),
        });
    }

    /// Register a planner in this registry.
    pub fn register_planner(&mut self, r: RegisteredPlanner) {
        self.planners.insert(r.name.to_owned(), r);
    }

    /// Register a heuristic in this registry.
    pub fn register_heuristic(&mut self, r: RegisteredHeuristic) {
        self.heuristics.insert(r.name.to_owned(), r);
    }

    /// Build a planner instance by name.
    pub fn build_planner(
        &self,
        name: &str,
        cfg: &PlannerConfig,
    ) -> Result<Box<dyn Planner>, MiniplanError> {
        let registered = self
            .planners
            .get(name)
            .ok_or_else(|| MiniplanError::InvalidPlanner(name.to_owned()))?;
        (registered.factory)(cfg)
    }

    /// Build a heuristic instance by name.
    pub fn build_heuristic(
        &self,
        name: &str,
        cfg: &PlannerConfig,
    ) -> Result<Box<dyn Heuristic>, MiniplanError> {
        let registered = self
            .heuristics
            .get(name)
            .ok_or_else(|| MiniplanError::InvalidHeuristic(name.to_owned()))?;
        (registered.factory)(cfg)
    }

    /// Iterate over all registered planners.
    pub fn planners(&self) -> impl Iterator<Item = &RegisteredPlanner> {
        self.planners.values()
    }

    /// Iterate over all registered heuristics.
    pub fn heuristics(&self) -> impl Iterator<Item = &RegisteredHeuristic> {
        self.heuristics.values()
    }
}

/// High-level solver that combines a [`Registry`] with [`SearchLimits`].
///
/// # Examples
///
/// ```
/// use miniplan::search::{Solver, PlannerChoice, PlannerConfig, SearchLimits};
///
/// let solver = Solver::new();
/// let choice = PlannerChoice::new("bfs");
/// let limits = SearchLimits::default();
/// // solver.solve_task(&task, &choice, &limits); // needs a Task
/// ```
pub struct Solver {
    /// The registry of available planners and heuristics.
    pub registry: Registry,
}

impl Default for Solver {
    fn default() -> Self {
        Self {
            registry: Registry::with_builtins(),
        }
    }
}

impl Solver {
    /// Create a new solver with the built-in registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Solve a task using the specified planner choice.
    pub fn solve_task(
        &self,
        task: &Task,
        choice: &PlannerChoice,
        limits: &SearchLimits,
    ) -> Result<SearchOutcome, MiniplanError> {
        let mut planner = self
            .registry
            .build_planner(&choice.planner, &choice.config)?;
        planner.solve(task, limits)
    }
}

/// Selects which planner and heuristic to use for a solve operation.
///
/// # Examples
///
/// ```
/// use miniplan::search::PlannerChoice;
///
/// let choice = PlannerChoice::new("astar");
/// assert_eq!(choice.planner, "astar");
/// ```
pub struct PlannerChoice {
    /// Name of the planner to use (must be registered).
    pub planner: String,
    /// Optional heuristic name (used by heuristic-driven planners).
    pub heuristic: Option<String>,
    /// Configuration options passed to the planner factory.
    pub config: PlannerConfig,
}

impl PlannerChoice {
    /// Create a new planner choice with just a planner name.
    pub fn new(planner: &str) -> Self {
        Self {
            planner: planner.to_owned(),
            heuristic: None,
            config: PlannerConfig::default(),
        }
    }
}
