use miniplan::ground::ground;
use miniplan::heuristic::HFF;
use miniplan::pddl_io::{load_domain_str, load_problem_str};
use miniplan::search::astar::Astar;
use miniplan::search::{Planner, SearchLimits, SearchOutcome};

const DOMAIN: &str = r#"
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

const PROBLEM: &str = r#"
(define (problem simple-test-1)
    (:domain simple-test)
    (:objects r1 r2 - room
              b1 - ball)
    (:init (at r1) (at-ball b1 r1) (free))
    (:goal (and (at r2) (at-ball b1 r2)))
)
"#;

fn main() {
    let domain = load_domain_str(DOMAIN).expect("domain parses");
    let problem = load_problem_str(PROBLEM).expect("problem parses");

    let task = ground(&domain, &problem).expect("grounding succeeds");
    println!(
        "Grounded task: {} facts, {} operators",
        task.num_facts(),
        task.operators.len()
    );

    let mut planner = Astar::new(Box::new(HFF));
    let limits = SearchLimits {
        time_budget: None,
        node_budget: None,
        memory_mb: None,
    };

    match planner.solve(&task, &limits).expect("solve returns") {
        SearchOutcome::Plan(plan, stats) => {
            println!("\nPlan found (A* + hFF):");
            println!("  Cost: {:.2}", stats.plan_cost);
            println!("  Length: {}", stats.plan_length);
            println!("  Nodes expanded: {}", stats.nodes_expanded);
            println!("  Time: {:?}", stats.elapsed);
            println!();
            print!("{}", plan);
        }
        SearchOutcome::Unsolvable(_) => {
            println!("Problem is unsolvable.");
        }
        SearchOutcome::LimitReached(_) => {
            println!("Search limit reached.");
        }
        _ => println!("Unknown outcome."),
    }
}
