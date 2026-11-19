//! Smoke tests for the `tui` subcommand's argument and input validation.
//!
//! Only failure paths are tested: they exit before the terminal is taken over.

use assert_cmd::Command;

/// A command for the `miniplan` binary built by this package.
fn miniplan() -> Command {
    Command::new(env!("CARGO_BIN_EXE_miniplan"))
}

/// `tui` without input files is a usage error (exit code 2).
#[test]
fn tui_requires_files() {
    let mut cmd = miniplan();
    cmd.arg("tui").assert().failure().code(2);
}

/// `tui` refuses a bundle that has a domain but no problem.
#[test]
fn tui_exits_on_no_problem() {
    let content = r"
(define (domain test-domain)
  (:requirements :strips)
  (:predicates (at ?x))
)
";
    let file = std::env::temp_dir().join("miniplan_test_no_problem.pddl");
    std::fs::write(&file, content).unwrap();

    let mut cmd = miniplan();
    cmd.arg("tui").arg(&file).assert().failure();

    let _ = std::fs::remove_file(&file);
}

/// `tui` refuses a bundle that has a problem but no domain.
#[test]
fn tui_exits_on_no_domain() {
    let content = r"
(define (problem test-problem)
  (:domain test-domain)
  (:objects)
  (:init)
  (:goal (at obj1))
)
";
    let file = std::env::temp_dir().join("miniplan_test_no_domain.pddl");
    std::fs::write(&file, content).unwrap();

    let mut cmd = miniplan();
    cmd.arg("tui").arg(&file).assert().failure();

    let _ = std::fs::remove_file(&file);
}
