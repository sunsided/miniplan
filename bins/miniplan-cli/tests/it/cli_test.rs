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

/// The plain plan `solve` prints for `gripper-small.pddl` with the default A* planner.
const GRIPPER_PLAIN_PLAN: &str = "; cost = 7\n; length = 7\n\
    (pick(ball2,rooma,left))\n(move(rooma,roomb))\n(drop(ball2,roomb,left))\n\
    (move(roomb,rooma))\n(pick(ball1,rooma,left))\n(move(rooma,roomb))\n\
    (drop(ball1,roomb,left))\n";

/// A domain and problem in one file whose goal no action can reach.
const UNSOLVABLE_PDDL: &str = r"
(define (domain stuck)
  (:requirements :strips)
  (:predicates (ready) (done))
  (:action finish
    :parameters ()
    :precondition (ready)
    :effect (done)))
(define (problem stuck-1)
  (:domain stuck)
  (:init)
  (:goal (done)))
";

/// Path of `name` inside the repository's `examples/pddl` directory.
fn example(name: &str) -> String {
    format!("{}/../../examples/pddl/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// A path in the system temp directory unique to this process and `tag`.
fn temp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("miniplan-cli-test-{}-{tag}", std::process::id()))
}

/// Runs the `miniplan` binary with `args` and returns its output.
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_miniplan"))
        .args(args)
        .output()
        .expect("CLI should run")
}

/// Stdout of `output` as text.
fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Stderr of `output` as text.
fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `solve` prints the plain plan to stdout by default and exits 0.
#[test]
fn solve_prints_plain_plan() {
    let output = run(&["solve", &example("gripper-small.pddl")]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), GRIPPER_PLAIN_PLAN);
}

/// `--format ipc` currently prints the same text as plain.
#[test]
fn solve_ipc_format() {
    let output = run(&["solve", &example("gripper-small.pddl"), "--format", "ipc"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), GRIPPER_PLAIN_PLAN);
}

/// `--format json` prints a JSON object with cost, length and steps.
#[test]
fn solve_json_format() {
    let output = run(&["solve", &example("gripper-small.pddl"), "--format", "json"]);

    assert_eq!(output.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");
    assert_eq!(json["cost"], 7.0);
    assert_eq!(json["length"], 7);
    assert_eq!(json["steps"][0], "pick(ball2,rooma,left)");
    assert_eq!(json["steps"].as_array().map(Vec::len), Some(7));
}

/// An unknown `--format` is a usage error (exit code 2) that lists the valid formats.
#[test]
fn solve_rejects_unknown_format() {
    let output = run(&["solve", &example("gripper-small.pddl"), "--format", "bogus"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("[possible values: plain, ipc, json]"));
}

/// `--output` writes the plan to a file and leaves stdout empty.
#[test]
fn solve_writes_output_file() {
    let path = temp_path("solve-output.plan");
    let output = run(&[
        "solve",
        &example("gripper-small.pddl"),
        "--output",
        path.to_str().unwrap(),
    ]);

    let written = std::fs::read_to_string(&path).expect("plan file written");
    let _ = std::fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "");
    assert_eq!(written, GRIPPER_PLAIN_PLAN);
}

/// An `--output` path in a missing directory fails with a write error.
#[test]
fn solve_reports_unwritable_output() {
    let path = temp_path("missing-dir").join("plan.txt");
    let output = run(&[
        "solve",
        &example("gripper-small.pddl"),
        "-o",
        path.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("Failed to write plan file"));
}

/// `--stats` adds the full search statistics to stderr after a plan was found.
#[test]
fn solve_prints_full_stats() {
    let output = run(&["solve", &example("gripper-small.pddl"), "--stats"]);

    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(0));
    for line in [
        "Search stats:",
        "Nodes expanded: ",
        "Nodes generated: ",
        "Plan cost: 7.00",
        "Plan length: 7",
    ] {
        assert!(stderr.contains(line), "missing {line:?} in {stderr}");
    }
}

/// A one-node budget stops the search: exit code 2 and short statistics.
#[test]
fn solve_limit_reached_exits_2() {
    let output = run(&[
        "solve",
        &example("gripper.pddl"),
        "--max-nodes",
        "1",
        "--stats",
    ]);

    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("Search limit reached."), "{stderr}");
    assert!(stderr.contains("Nodes expanded: 1"), "{stderr}");
    assert!(!stderr.contains("Nodes generated"), "{stderr}");
}

/// A problem whose goal is unreachable exits with code 1 and says it is unsolvable.
#[test]
fn solve_unsolvable_exits_1() {
    let path = temp_path("unsolvable.pddl");
    std::fs::write(&path, UNSOLVABLE_PDDL).unwrap();

    let output = run(&[
        "solve",
        path.to_str().unwrap(),
        "-p",
        "bfs",
        "-t",
        "10s",
        "--stats",
    ]);

    let _ = std::fs::remove_file(&path);
    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("Problem is unsolvable."), "{stderr}");
    assert!(stderr.contains("Search stats:"), "{stderr}");
}

/// Stdout and stderr of `output` together, for log lines whose stream is not part of the
/// contract.
fn all_output(output: &std::process::Output) -> String {
    stdout(output) + &stderr(output)
}

/// `--domain` and `--problem` select one problem of a multi-problem file.
#[test]
fn solve_selects_problem() {
    let output = run(&[
        "solve",
        &example("air-cargo.pddl"),
        "--domain",
        "air-cargo",
        "--problem",
        "air-cargo-p1",
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).starts_with("; cost = 6\n; length = 6\n"));
}

/// Every `-v` level logs what was loaded; without `-v` nothing is logged.
#[test]
fn verbosity_levels_log_loading() {
    let log_line = "Loaded domain 'gripper' and problem 'gripper-2'";
    let file = example("gripper-small.pddl");

    for flag in ["-v", "-vv", "-vvv"] {
        let output = run(&["solve", &file, flag]);

        assert_eq!(output.status.code(), Some(0), "{flag}");
        assert!(stderr(&output).contains(log_line), "{flag}");
        assert!(
            !stdout(&output).contains(log_line),
            "{flag}: logs on stdout"
        );
    }

    // Logs on stderr keep machine-readable stdout intact.
    let json = run(&["solve", &file, "-vv", "--format", "json"]);
    assert_eq!(json.status.code(), Some(0));
    serde_json::from_slice::<serde_json::Value>(&json.stdout).expect("stdout is valid JSON");

    let quiet = run(&["check", &file]);
    assert!(!all_output(&quiet).contains(log_line));
    let verbose = run(&["check", &file, "-v"]);
    assert!(all_output(&verbose).contains(log_line));
}

/// An unknown planner name fails and lists the valid planners.
#[test]
fn solve_rejects_unknown_planner() {
    let output = run(&["solve", &example("gripper-small.pddl"), "-p", "nope"]);

    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("unknown planner: nope"), "{stderr}");
    assert!(stderr.contains("astar"), "{stderr}");
}

/// A missing input file fails to load.
#[test]
fn solve_reports_unreadable_file() {
    let output = run(&["solve", "/nonexistent/miniplan/input.pddl"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("Failed to load PDDL files"));
}

/// `check` grounds a single-problem file and prints the task size and names.
#[test]
fn check_prints_task_summary() {
    let output = run(&["check", &example("gripper-small.pddl")]);

    let stdout = stdout(&output);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout.starts_with("Task grounded successfully:\n"),
        "{stdout}"
    );
    assert!(stdout.contains("  Objects: 5\n"), "{stdout}");
    assert!(stdout.contains("  Domain: gripper\n"), "{stdout}");
    assert!(stdout.contains("  Problem: gripper-2\n"), "{stdout}");
}

/// `check` with an unknown `--problem` fails and names the available problems.
#[test]
fn check_rejects_unknown_problem() {
    let output = run(&["check", &example("air-cargo.pddl"), "--problem", "nope"]);

    let stderr = stderr(&output);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("problem 'nope' not found"), "{stderr}");
    assert!(stderr.contains("air-cargo-p2"), "{stderr}");
}

/// `list-planners` lists planners with descriptions and the registered heuristics.
#[test]
fn list_planners_lists_heuristics() {
    let output = run(&["list-planners"]);

    let stdout = stdout(&output);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.starts_with("Available planners:\n"), "{stdout}");
    assert!(stdout.contains("\nAvailable heuristics:\n"), "{stdout}");
    assert!(stdout.contains("  bibfs-uc"), "{stdout}");
}
