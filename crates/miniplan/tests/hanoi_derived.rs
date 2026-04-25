use miniplan::ground::ground;
use miniplan::heuristic::HFF;
use miniplan::pddl_io::load_combined_str_named;
use miniplan::search::{Astar, Planner, SearchLimits};
use pddl::Parser;

const HANOI_DERIVED_SRC: &str = include_str!("../../../examples/pddl/hanoi-derived.pddl");

#[test]
fn test_hanoi_derived_plan_length() {
    let parsed =
        pddl::PddlFile::from_str(HANOI_DERIVED_SRC).expect("hanoi-derived.pddl should parse");
    assert_eq!(parsed.domain_count(), 1, "should have exactly 1 domain");
    assert_eq!(parsed.problem_count(), 1, "should have exactly 1 problem");

    let (domain, problem) =
        load_combined_str_named(HANOI_DERIVED_SRC, None, None).expect("should load");
    let task = ground(&domain, &problem).expect("grounding should succeed");

    let mut planner = Astar::new(Box::new(HFF));
    let limits = SearchLimits {
        time_budget: Some(std::time::Duration::from_secs(60)),
        node_budget: Some(1_000_000),
        memory_mb: None,
    };

    let outcome = planner.solve(&task, &limits).expect("solve should return");
    match outcome {
        miniplan::search::SearchOutcome::Plan(plan, _stats) => {
            assert_eq!(
                plan.len(),
                7,
                "hanoi-derived optimal plan length should be 7"
            );
        }
        miniplan::search::SearchOutcome::Unsolvable(_) => {
            panic!("hanoi-derived should be solvable")
        }
        miniplan::search::SearchOutcome::LimitReached(_) => {
            panic!("hanoi-derived hit search limits")
        }
        _ => panic!("unknown outcome for hanoi-derived"),
    }
}
