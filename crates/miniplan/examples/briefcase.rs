use miniplan::ground::ground;
use miniplan::heuristic::{BlindHeuristic, HFF};
use miniplan::pddl_io::{load_domain_str, load_problem_str};
use miniplan::search::{Astar, Bfs, Planner, SearchLimits, SearchOutcome};

const DOMAIN: &str = r#"
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

const PROBLEM: &str = r#"
(define (problem get-paid)
    (:domain briefcase-world)
    (:objects home office - location
              p d - physob)
    (:init (at B home) (at P home) (at D home) (in P))
    (:goal (and (at B office) (at D home) (at P home)))
)
"#;

fn main() {
    let domain = load_domain_str(DOMAIN).expect("domain parses");
    let problem = load_problem_str(PROBLEM).expect("problem parses");

    let task = ground(&domain, &problem).expect("grounding succeeds");
    println!(
        "Briefcase world: {} facts, {} operators",
        task.num_facts(),
        task.operators.len()
    );

    let limits = SearchLimits::default();

    println!("\n--- BFS ---");
    solve_with(&mut Bfs::new(), &task, &limits);

    println!("\n--- A* + hFF ---");
    solve_with(&mut Astar::new(Box::new(HFF)), &task, &limits);

    println!("\n--- A* + blind ---");
    solve_with(&mut Astar::new(Box::new(BlindHeuristic)), &task, &limits);
}

fn solve_with<P: Planner>(planner: &mut P, task: &miniplan::task::Task, limits: &SearchLimits) {
    match planner.solve(task, limits).expect("solve returns") {
        SearchOutcome::Plan(plan, stats) => {
            println!(
                "  Plan found: {} steps, cost {:.2}, expanded {} nodes in {:?}",
                stats.plan_length, stats.plan_cost, stats.nodes_expanded, stats.elapsed
            );
            print!("{}", plan);
        }
        SearchOutcome::Unsolvable(_) => {
            println!("  Unsolvable.");
        }
        SearchOutcome::LimitReached(stats) => {
            println!("  Limit reached (expanded {} nodes).", stats.nodes_expanded);
        }
        _ => println!("  Unknown outcome."),
    }
}
