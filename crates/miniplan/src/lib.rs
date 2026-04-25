#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! A PDDL planning library with multiple search algorithms.
//!
//! `miniplan` is a Rust library for parsing, grounding, and solving PDDL planning
//! tasks. It provides a collection of search algorithms (planners) and heuristic
//! functions, accessible through a registry-based API.
//!
//! # Modules
//!
//! - [`error`] — error types for parsing, grounding, and search.
//! - [`ground`] — grounding functions that convert PDDL to a state-space representation.
//! - [`heuristic`] — heuristic functions (h^add, h^max, h^FF, etc.).
//! - [`pddl_io`] — PDDL file loading utilities.
//! - [`plan`] — plan representation.
//! - [`search`] — search algorithms and the planner registry.
//! - [`task`] — grounded task representation (facts, operators, states).
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
