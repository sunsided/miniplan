use miniplan::ground::ground;
use miniplan::heuristic::HFF;
use miniplan::pddl_io::{PddlFile, load_combined_str, load_combined_str_named};
use miniplan::search::{Astar, Planner, SearchLimits};
use pddl::Parser;

const AIR_CARGO_SRC: &str = include_str!("../../../examples/pddl/air-cargo.pddl");

const TWO_DOMAIN_FIXTURE: &str = r#"
(define (domain domain-a)
    (:requirements :strips)
    (:predicates (p))
)

(define (domain domain-b)
    (:requirements :strips)
    (:predicates (q))
)

(define (problem problem-a)
    (:domain domain-a)
    (:init (p))
    (:goal (p))
)
"#;

#[test]
fn test_aind_multi_problem_parse() {
    let parsed = PddlFile::from_str(AIR_CARGO_SRC).expect("air-cargo.pddl should parse");

    assert_eq!(parsed.domain_count(), 1, "should have exactly 1 domain");
    assert_eq!(parsed.problem_count(), 3, "should have exactly 3 problems");

    let names: Vec<String> = parsed
        .problems
        .iter()
        .map(|p| p.name().to_string())
        .collect();
    assert!(names.contains(&"air-cargo-p1".to_string()));
    assert!(names.contains(&"air-cargo-p2".to_string()));
    assert!(names.contains(&"air-cargo-p3".to_string()));
}

#[test]
fn test_aind_load_combined_errors_on_multiple_problems() {
    let err = load_combined_str(AIR_CARGO_SRC).expect_err("should error on multiple problems");
    let msg = err.to_string();
    assert!(msg.contains("air-cargo-p1"), "error should list p1");
    assert!(msg.contains("air-cargo-p2"), "error should list p2");
    assert!(msg.contains("air-cargo-p3"), "error should list p3");
}

#[test]
fn test_multi_domain_errors() {
    let err = load_combined_str(TWO_DOMAIN_FIXTURE).expect_err("should error on multiple domains");
    let msg = err.to_string();
    assert!(msg.contains("domain-a"), "error should list domain-a");
    assert!(msg.contains("domain-b"), "error should list domain-b");

    let (domain, problem) = load_combined_str_named(TWO_DOMAIN_FIXTURE, Some("domain-a"), None)
        .expect("named selection should succeed");
    assert_eq!(domain.name().to_string(), "domain-a");
    assert_eq!(problem.name().to_string(), "problem-a");
}

fn solve_with_ff(src: &str, problem_name: &str) -> usize {
    let (domain, problem) = load_combined_str_named(src, None, Some(problem_name))
        .unwrap_or_else(|e| panic!("{} should load: {}", problem_name, e));
    let task = ground(&domain, &problem).expect("grounding should succeed");

    let mut planner = Astar::new(Box::new(HFF));
    let limits = SearchLimits {
        time_budget: Some(std::time::Duration::from_secs(60)),
        node_budget: Some(1_000_000),
        memory_mb: None,
    };

    let outcome = planner.solve(&task, &limits).expect("solve should return");
    match outcome {
        miniplan::search::SearchOutcome::Plan(plan, _stats) => plan.len(),
        miniplan::search::SearchOutcome::Unsolvable(_) => {
            panic!("{} should be solvable", problem_name)
        }
        miniplan::search::SearchOutcome::LimitReached(_) => {
            panic!("{} hit search limits", problem_name)
        }
        _ => panic!("unknown outcome for {}", problem_name),
    }
}

#[test]
fn test_aind_p1_astar_ff() {
    let len = solve_with_ff(AIR_CARGO_SRC, "air-cargo-p1");
    assert_eq!(len, 6, "p1 optimal plan length should be 6");
}

#[test]
fn test_aind_p2_astar_ff() {
    let len = solve_with_ff(AIR_CARGO_SRC, "air-cargo-p2");
    assert_eq!(len, 9, "p2 optimal plan length should be 9");
}

#[test]
#[ignore = "A*+FF with non-admissible FF heuristic does not guarantee optimal plan length; see heuristic_analysis.tex timing caveat"]
fn test_aind_p3_astar_ff() {
    let len = solve_with_ff(AIR_CARGO_SRC, "air-cargo-p3");
    assert_eq!(len, 12, "p3 optimal plan length should be 12");
}

#[test]
fn test_aind_named_problem_missing() {
    let err = load_combined_str_named(AIR_CARGO_SRC, None, Some("does-not-exist"))
        .expect_err("should error");
    let msg = err.to_string();
    assert!(
        msg.contains("does-not-exist"),
        "error should mention the missing name"
    );
    assert!(msg.contains("air-cargo-p1"), "error should list p1");
    assert!(msg.contains("air-cargo-p2"), "error should list p2");
    assert!(msg.contains("air-cargo-p3"), "error should list p3");
}
