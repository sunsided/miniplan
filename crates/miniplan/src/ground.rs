//! Grounding — converting PDDL to a state-space representation.
//!
//! The [`ground`] function takes a parsed PDDL domain and problem and produces
//! a fully grounded [`Task`](crate::task::Task) with all operators instantiated.
//!
//! # Examples
//!
//! ```
//! use miniplan::ground::ground;
//! use miniplan::pddl_io::{load_domain_str, load_problem_str};
//!
//! let domain = load_domain_str(r#"
//! (define (domain test)
//!   (:requirements :strips)
//!   (:predicates (a) (b))
//!   (:action go
//!     :parameters ()
//!     :precondition (a)
//!     :effect (and (b) (not (a))))
//! )
//! "#).expect("domain parses");
//!
//! let problem = load_problem_str(r#"
//! (define (problem test-1)
//!   (:domain test)
//!   (:init (a))
//!   (:goal (b)))
//! "#).expect("problem parses");
//!
//! let task = ground(&domain, &problem).expect("grounding succeeds");
//! assert!(!task.operators.is_empty());
//! ```

mod action;
mod cost;
mod effect;
mod formula;
mod types;

pub use action::ground;
