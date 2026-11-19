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
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("input.pddl");
    std::fs::write(&file, content).expect("write fixture");

    let assert = miniplan().arg("tui").arg(&file).assert().code(1);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(stderr.contains("no problem definition found"), "{stderr}");
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
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("input.pddl");
    std::fs::write(&file, content).expect("write fixture");

    let assert = miniplan().arg("tui").arg(&file).assert().code(1);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(stderr.contains("no domain definition found"), "{stderr}");
}
