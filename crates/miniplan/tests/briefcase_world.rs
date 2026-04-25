use miniplan::ground::ground;
use miniplan::heuristic::HFF;
use miniplan::load_combined_str;
use miniplan::pddl_io::{load_domain_str, load_problem_str};
use miniplan::search::astar::Astar;
use miniplan::search::bfs::Bfs;
use miniplan::search::{Planner, SearchLimits};

// Simple gripper-like domain for initial smoke testing
const SIMPLE_DOMAIN: &str = r#"
(define (domain simple-test)
    (:requirements :strips :typing)
    (:types room ball)
    (:predicates (at ?r - room)
                 (at-ball ?b - ball ?r - room)
                 (free)
                 (holding ?b - ball))

    (:action move
        :parameters (?from ?to - room)
        :precondition (at ?from)
        :effect (and (at ?to) (not (at ?from))))

    (:action pick
        :parameters (?b - ball ?r - room)
        :precondition (and (free) (at-ball ?b ?r) (at ?r))
        :effect (and (holding ?b) (not (free)) (not (at-ball ?b ?r))))

    (:action drop
        :parameters (?b - ball ?r - room)
        :precondition (holding ?b)
        :effect (and (free) (at-ball ?b ?r) (not (holding ?b))))
)
"#;

const SIMPLE_PROBLEM: &str = r#"
(define (problem simple-test-1)
    (:domain simple-test)
    (:objects r1 r2 - room
              b1 - ball)
    (:init (at r1) (at-ball b1 r1) (free))
    (:goal (and (at r2) (at-ball b1 r2)))
)
"#;

const BRIEFCASE_DOMAIN: &str = r#"
(define (domain briefcase-world)
    (:requirements :strips :equality :typing :conditional-effects)
    (:types location physob)
    (:constants B - physob)
    (:predicates (at ?x - physob ?y - location)
                 (in ?x - physob))

    (:action mov-B
        :parameters (?m ?l - location)
        :precondition (and (at B ?m) (not (= ?m ?l)))
        :effect (and (at B ?l) (not (at B ?m))))

    (:action put-in
        :parameters (?x - physob ?l - location)
        :precondition (and (at ?x ?l) (at B ?l))
        :effect (in ?x))

    (:action take-out
        :parameters (?x - physob ?l - location)
        :precondition (and (in ?x) (at B ?l))
        :effect (not (in ?x)))
)
"#;

const BRIEFCASE_PROBLEM: &str = r#"
(define (problem get-paid)
    (:domain briefcase-world)
    (:objects home office - location
              p d - physob)
    (:init (at B home) (at P home) (at D home) (in P))
    (:goal (and (at B office) (at D home) (at P home)))
)
"#;

#[test]
fn test_briefcase_parse_and_ground() {
    let domain = load_domain_str(BRIEFCASE_DOMAIN).expect("domain parses");
    let problem = load_problem_str(BRIEFCASE_PROBLEM).expect("problem parses");
    let task = ground(&domain, &problem).expect("grounding succeeds");

    assert!(!task.operators.is_empty(), "should have grounded operators");
    assert!(
        !task.goal_pos.is_empty() || !task.goal_neg.is_empty(),
        "should have goals"
    );
}

#[test]
fn test_simple_parse_and_ground() {
    let domain = load_domain_str(SIMPLE_DOMAIN).expect("domain parses");
    let problem = load_problem_str(SIMPLE_PROBLEM).expect("problem parses");
    let task = ground(&domain, &problem).expect("grounding succeeds");

    assert!(!task.operators.is_empty(), "should have grounded operators");
    eprintln!(
        "Simple: {} facts, {} operators",
        task.num_facts(),
        task.operators.len()
    );
}

#[test]
fn test_simple_solve_bfs() {
    let domain = load_domain_str(SIMPLE_DOMAIN).expect("domain parses");
    let problem = load_problem_str(SIMPLE_PROBLEM).expect("problem parses");
    let task = ground(&domain, &problem).expect("grounding succeeds");

    let mut planner = Bfs::new();
    let limits = SearchLimits {
        time_budget: Some(std::time::Duration::from_secs(30)),
        node_budget: Some(100_000),
        memory_mb: None,
    };

    let outcome = planner.solve(&task, &limits).expect("solve returns");
    match outcome {
        miniplan::SearchOutcome::Plan(plan, _stats) => {
            assert!(!plan.is_empty(), "BFS should find a plan");
            eprintln!("Simple BFS plan ({} steps):", plan.len());
            for step in &plan.steps {
                eprintln!("  {}", step.op_name);
            }
        }
        miniplan::SearchOutcome::Unsolvable(_) => {
            panic!("simple-test should be solvable");
        }
        miniplan::SearchOutcome::LimitReached(_) => {
            panic!("BFS should not hit limits on simple-test");
        }
        _ => panic!("unknown outcome"),
    }
}

#[test]
fn test_briefcase_solve_bfs() {
    let domain = load_domain_str(BRIEFCASE_DOMAIN).expect("domain parses");
    let problem = load_problem_str(BRIEFCASE_PROBLEM).expect("problem parses");
    let task = ground(&domain, &problem).expect("grounding succeeds");

    eprintln!(
        "Briefcase: {} facts, {} operators",
        task.num_facts(),
        task.operators.len()
    );
    eprintln!(
        "Goal pos bits: {:?}",
        task.goal_pos.0.ones().collect::<Vec<_>>()
    );
    eprintln!("Init bits: {:?}", task.init.0.ones().collect::<Vec<_>>());

    for (i, fact) in task.facts.iter().enumerate() {
        if task.goal_pos.0.contains(i) || task.init.0.contains(i) {
            eprintln!("  [{}] {:?}", i, fact);
        }
    }

    let mut planner = Bfs::new();
    let limits = SearchLimits {
        time_budget: Some(std::time::Duration::from_secs(30)),
        node_budget: Some(100_000),
        memory_mb: None,
    };

    let outcome = planner.solve(&task, &limits).expect("solve returns");
    match outcome {
        miniplan::SearchOutcome::Plan(plan, stats) => {
            eprintln!(
                "Briefcase BFS plan ({} steps, expanded {}):",
                plan.len(),
                stats.nodes_expanded
            );
            for step in &plan.steps {
                eprintln!("  {}", step.op_name);
            }
            assert!(!plan.is_empty(), "BFS should find a plan");
        }
        miniplan::SearchOutcome::Unsolvable(stats) => {
            eprintln!("Expanded: {}", stats.nodes_expanded);
            panic!("briefcase-world should be solvable");
        }
        miniplan::SearchOutcome::LimitReached(_) => {
            panic!("BFS should not hit limits on briefcase-world");
        }
        _ => panic!("unknown outcome"),
    }
}

#[test]
fn test_briefcase_solve_astar_ff() {
    let domain = load_domain_str(BRIEFCASE_DOMAIN).expect("domain parses");
    let problem = load_problem_str(BRIEFCASE_PROBLEM).expect("problem parses");
    let task = ground(&domain, &problem).expect("grounding succeeds");

    let mut planner = Astar::new(Box::new(HFF));
    let limits = SearchLimits {
        time_budget: Some(std::time::Duration::from_secs(30)),
        node_budget: Some(100_000),
        memory_mb: None,
    };

    let outcome = planner.solve(&task, &limits).expect("solve returns");
    match outcome {
        miniplan::SearchOutcome::Plan(plan, _stats) => {
            assert!(!plan.is_empty(), "A*+FF should find a plan");
        }
        miniplan::SearchOutcome::Unsolvable(_) => {
            panic!("briefcase-world should be solvable");
        }
        miniplan::SearchOutcome::LimitReached(_) => {
            panic!("A*+FF should not hit limits on briefcase-world");
        }
        _ => panic!("unknown outcome"),
    }
}

const DOMAIN_WITH_COMMENTS: &str = r#"
; This is a test domain with leading comments
; note: uses (either ...) typing syntax
; another comment with unbalanced ( parens here

(define (domain comment-test)
    (:requirements :strips :typing)
    (:types room ball)
    (:predicates (at ?r - room)
                 (at-ball ?b - ball ?r - room)
                 (free)
                 (holding ?b - ball))

    (:action move
        :parameters (?from ?to - room)
        :precondition (at ?from)
        :effect (and (at ?to) (not (at ?from))))

    (:action pick
        :parameters (?b - ball ?r - room)
        :precondition (and (free) (at-ball ?b ?r) (at ?r))
        :effect (and (holding ?b) (not (free)) (not (at-ball ?b ?r))))

    (:action drop
        :parameters (?b - ball ?r - room)
        :precondition (holding ?b)
        :effect (and (free) (at-ball ?b ?r) (not (holding ?b))))
)
"#;

const PROBLEM_WITH_COMMENTS: &str = r#"
; Problem file also supports leading comments
; goal: move things (careful with parens) around

(define (problem comment-test-1)
    (:domain comment-test)
    (:objects r1 r2 - room
              b1 - ball)
    (:init (at r1) (at-ball b1 r1) (free))
    (:goal (and (at r2) (at-ball b1 r2)))
)
"#;

#[test]
fn test_domain_with_leading_comments() {
    let domain =
        load_domain_str(DOMAIN_WITH_COMMENTS).expect("domain with leading comments parses");
    let problem =
        load_problem_str(PROBLEM_WITH_COMMENTS).expect("problem with leading comments parses");
    let task = ground(&domain, &problem).expect("grounding succeeds");
    assert!(!task.operators.is_empty(), "should have grounded operators");
}

const COMBINED_WITH_COMMENTS: &str = r#"
; Combined PDDL file with domain and problem
; handles (either cargo plane) typing

; Domain definition below
(define (domain combined-comment)
    (:requirements :strips :typing)
    (:types location)
    (:predicates (at ?x - location))

    (:action go
        :parameters (?from ?to - location)
        :precondition (at ?from)
        :effect (and (at ?to) (not (at ?from))))
)

; Problem definition follows
; goal must be reachable (no tricky parens here)

(define (problem combined-comment-1)
    (:domain combined-comment)
    (:objects a b - location)
    (:init (at a))
    (:goal (and (at b)))
)
"#;

#[test]
fn test_combined_with_leading_and_interleaved_comments() {
    let (domain, problem) =
        load_combined_str(COMBINED_WITH_COMMENTS).expect("combined file with comments parses");
    let task = ground(&domain, &problem).expect("grounding succeeds");
    assert!(!task.operators.is_empty(), "should have grounded operators");
    assert!(
        !task.goal_pos.is_empty() || !task.goal_neg.is_empty(),
        "should have goals"
    );
}
