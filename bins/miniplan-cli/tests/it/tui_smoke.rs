use assert_cmd::Command;

fn miniplan() -> Command {
    Command::cargo_bin("miniplan").unwrap()
}

#[test]
fn tui_requires_files() {
    let mut cmd = miniplan();
    cmd.arg("tui").assert().failure().code(2);
}

#[test]
fn tui_exits_on_no_problem() {
    let content = r#"
(define (domain test-domain)
  (:requirements :strips)
  (:predicates (at ?x))
)
"#;
    let file = std::env::temp_dir().join("miniplan_test_no_problem.pddl");
    std::fs::write(&file, content).unwrap();

    let mut cmd = miniplan();
    cmd.arg("tui").arg(&file).assert().failure();

    let _ = std::fs::remove_file(&file);
}

#[test]
fn tui_exits_on_no_domain() {
    let content = r#"
(define (problem test-problem)
  (:domain test-domain)
  (:objects)
  (:init)
  (:goal (at obj1))
)
"#;
    let file = std::env::temp_dir().join("miniplan_test_no_domain.pddl");
    std::fs::write(&file, content).unwrap();

    let mut cmd = miniplan();
    cmd.arg("tui").arg(&file).assert().failure();

    let _ = std::fs::remove_file(&file);
}
