//! End-to-end tests of the `miniplan` binary's non-interactive subcommands.

use std::process::Command;

/// `--help` succeeds and lists the main subcommands.
#[test]
fn test_cli_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_miniplan"))
        .arg("--help")
        .output()
        .expect("CLI should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("solve"));
    assert!(stdout.contains("check"));
    assert!(stdout.contains("list-planners"));
}

/// `list-planners` succeeds and lists the core planners.
#[test]
fn test_cli_list_planners() {
    let output = Command::new(env!("CARGO_BIN_EXE_miniplan"))
        .arg("list-planners")
        .output()
        .expect("CLI should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("astar"));
    assert!(stdout.contains("bfs"));
    assert!(stdout.contains("gbfs"));
}

/// `check` on a file with several problems fails without `--problem` and names every candidate.
#[test]
fn test_cli_check_multi_problem_errors() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let pddl_path = format!("{manifest_dir}/../../examples/pddl/air-cargo.pddl");

    let output = Command::new(env!("CARGO_BIN_EXE_miniplan"))
        .arg("check")
        .arg(&pddl_path)
        .output()
        .expect("CLI should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("test_cli_check_multi_problem_errors stderr: {stderr}");
    eprintln!("test_cli_check_multi_problem_errors stdout: {stdout}");
    eprintln!(
        "test_cli_check_multi_problem_errors exit: {:?}",
        output.status
    );

    assert!(!output.status.success(), "should fail without --problem");
    assert!(stderr.contains("air-cargo-p1"), "stderr should list p1");
    assert!(stderr.contains("air-cargo-p2"), "stderr should list p2");
    assert!(stderr.contains("air-cargo-p3"), "stderr should list p3");
}

/// `check --problem` picks one problem out of a multi-problem file and prints its size.
#[test]
fn test_cli_check_with_problem_flag() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let pddl_path = format!("{manifest_dir}/../../examples/pddl/air-cargo.pddl");

    let output = Command::new(env!("CARGO_BIN_EXE_miniplan"))
        .arg("check")
        .arg(&pddl_path)
        .arg("--problem")
        .arg("air-cargo-p1")
        .output()
        .expect("CLI should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("test_cli_check_with_problem_flag stderr: {stderr}");
    eprintln!("test_cli_check_with_problem_flag stdout: {stdout}");
    eprintln!("test_cli_check_with_problem_flag exit: {:?}", output.status);

    assert!(output.status.success(), "should succeed with --problem");
    assert!(stdout.contains("Facts:"), "stdout should contain Facts");
    assert!(
        stdout.contains("Operators:"),
        "stdout should contain Operators"
    );
}
