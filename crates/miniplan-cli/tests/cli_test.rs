use std::process::Command;

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
