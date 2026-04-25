#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! A PDDL planner library built around the [`pddl`](https://crates.io/crates/pddl) crate.
//!
//! `miniplan` is primarily a **planner** — it takes a parsed PDDL domain and problem,
//! grounds them into a state-space representation, and solves them using a collection
//! of search algorithms (planners) and heuristic functions. PDDL parsing and loading
//! utilities are re-exported from the `pddl` crate for convenience.
//!
//! # Architecture
//!
//! 1. **Parse** — use `pddl::Parser` (re-exported via [`pddl_io`]) to load PDDL.
//! 2. **Ground** — call [`ground::ground`] to convert the parsed domain and problem
//!    into a [`task::Task`] with grounded facts and operators.
//! 3. **Solve** — use the [`search::Solver`] and [`search::Registry`] to pick a
//!    planner and search for a plan.
//!
//! # Modules
//!
//! - [`search`] — search algorithms (BFS, A*, GBFS, bidirectional variants)
//!   and the planner registry.
//! - [`heuristic`] — heuristic functions (h^add, h^max, h^FF, etc.).
//! - [`ground`] — grounding functions that convert PDDL to a state-space representation.
//! - [`task`] — grounded task representation (facts, operators, states).
//! - [`plan`] — plan representation and formatting.
//! - [`pddl_io`] — convenience wrappers around `pddl::Parser` for loading PDDL
//!   from strings, files, or multiple files.
//! - [`error`] — error types for parsing, grounding, and search.
//!
//! # Getting started
//!
//! The simplest way to solve a planning task is to use the [`Solver`](search::Solver)
//! with the built-in registry:
//!
//! ```
//! use miniplan::search::{Solver, PlannerChoice, SearchLimits};
//! use miniplan::pddl_io::{load_domain_str, load_problem_str};
//! use miniplan::ground::ground;
//!
//! const DOMAIN: &str = r#"
//! (define (domain test)
//!   (:requirements :strips)
//!   (:predicates (a) (b))
//!   (:action go
//!     :parameters ()
//!     :precondition (a)
//!     :effect (and (b) (not (a))))
//! )
//! "#;
//!
//! const PROBLEM: &str = r#"
//! (define (problem test-1)
//!   (:domain test)
//!   (:init (a))
//!   (:goal (b)))
//! "#;
//!
//! let domain = load_domain_str(DOMAIN).expect("domain parses");
//! let problem = load_problem_str(PROBLEM).expect("problem parses");
//! let task = ground(&domain, &problem).expect("grounding succeeds");
//!
//! let solver = Solver::new();
//! let choice = PlannerChoice::new("bfs");
//! let limits = SearchLimits::default();
//!
//! match solver.solve_task(&task, &choice, &limits).expect("solve returns") {
//!     miniplan::search::SearchOutcome::Plan(plan, _stats) => {
//!         assert_eq!(plan.len(), 1);
//!     }
//!     _ => panic!("expected a plan"),
//! }
//! ```

pub mod error;
pub mod ground;
pub mod heuristic;
pub mod pddl_io;
pub mod plan;
pub mod search;
pub mod task;

mod util;
