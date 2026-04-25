pub mod astar;
pub mod bfs;
pub mod bibae;
pub mod bibfs_uc;
pub mod bidij;
pub mod binbs;
pub mod gbfs;

use std::time::Duration;

use bitflags::bitflags;

use crate::error::MiniplanError;
use crate::plan::Plan;
use crate::task::{State, Task};

pub use crate::task::OpId;

#[derive(Debug, Clone, Copy)]
pub struct HValue(pub f64);

impl HValue {
    pub const INFINITY: HValue = HValue(f64::INFINITY);

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
    pub struct PlannerCapabilities: u32 {
        const CLASSICAL           = 1 << 0;
        const NEGATIVE_PRECONDS   = 1 << 1;
        const DISJUNCTIVE         = 1 << 2;
        const QUANTIFIED_PRECONS  = 1 << 3;
        const CONDITIONAL_EFFECTS = 1 << 4;
        const ACTION_COSTS        = 1 << 5;
        const OPTIMAL             = 1 << 6;
    }
}

#[derive(Debug, Clone)]
pub struct SearchLimits {
    pub time_budget: Option<Duration>,
    pub node_budget: Option<u64>,
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

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SearchOutcome {
    Plan(Plan, SearchStats),
    Unsolvable(SearchStats),
    LimitReached(SearchStats),
}

#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub plan_cost: f64,
    pub plan_length: usize,
    pub elapsed: Duration,
}

pub trait Heuristic: Send + Sync {
    fn name(&self) -> &str;
    fn estimate(&self, task: &Task, state: &State) -> HValue;
    fn preferred_ops(&self, _task: &Task, _state: &State) -> &[OpId] {
        &[]
    }
}

pub trait Planner: Send {
    fn name(&self) -> &str;
    fn describe(&self) -> &str {
        ""
    }
    fn capabilities(&self) -> PlannerCapabilities {
        PlannerCapabilities::CLASSICAL
    }
    fn solve(&mut self, task: &Task, limits: &SearchLimits)
    -> Result<SearchOutcome, MiniplanError>;
}

#[derive(Default, Clone)]
pub struct PlannerConfig {
    pub opts: rustc_hash::FxHashMap<String, String>,
}

pub struct RegisteredPlanner {
    pub name: &'static str,
    pub description: &'static str,
    pub capabilities: PlannerCapabilities,
    pub factory: PlannerFactory,
}

pub struct RegisteredHeuristic {
    pub name: &'static str,
    pub factory: HeuristicFactory,
}

pub type PlannerFactory =
    std::sync::Arc<dyn Fn(&PlannerConfig) -> Result<Box<dyn Planner>, MiniplanError> + Send + Sync>;

pub type HeuristicFactory = std::sync::Arc<
    dyn Fn(&PlannerConfig) -> Result<Box<dyn Heuristic>, MiniplanError> + Send + Sync,
>;

pub struct Registry {
    planners: rustc_hash::FxHashMap<String, RegisteredPlanner>,
    heuristics: rustc_hash::FxHashMap<String, RegisteredHeuristic>,
}

impl Registry {
    pub fn with_builtins() -> Self {
        let mut r = Self {
            planners: rustc_hash::FxHashMap::default(),
            heuristics: rustc_hash::FxHashMap::default(),
        };
        r.register_builtins();
        r
    }

    fn register_builtins(&mut self) {
        use crate::heuristic::blind::BlindHeuristic;
        use crate::heuristic::goal_count::GoalCountHeuristic;
        use crate::heuristic::relaxed::{HAdd, HFF, HMax};
        use crate::heuristic::zero::HZero;
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
            name: "binbs",
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
                Ok(Box::new(binbs::Nbs::new(h)))
            }),
        });

        self.register_planner(RegisteredPlanner {
            name: "bibae",
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
                Ok(Box::new(bibae::BiBae::new(h)))
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

    pub fn register_planner(&mut self, r: RegisteredPlanner) {
        self.planners.insert(r.name.to_owned(), r);
    }

    pub fn register_heuristic(&mut self, r: RegisteredHeuristic) {
        self.heuristics.insert(r.name.to_owned(), r);
    }

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

    pub fn planners(&self) -> impl Iterator<Item = &RegisteredPlanner> {
        self.planners.values()
    }

    pub fn heuristics(&self) -> impl Iterator<Item = &RegisteredHeuristic> {
        self.heuristics.values()
    }
}

pub struct Solver {
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
    pub fn new() -> Self {
        Self::default()
    }

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

pub struct PlannerChoice {
    pub planner: String,
    pub heuristic: Option<String>,
    pub config: PlannerConfig,
}
