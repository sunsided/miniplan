//! Property tests: plans are executable, and optimal planners find shortest plans.
//!
//! Each case is a random directed graph rendered as a PDDL "move a token" problem. A plain
//! breadth-first search over the graph itself is the oracle for solvability and plan length.

use std::collections::VecDeque;

use miniplan::ground::ground;
use miniplan::heuristic::{HFF, HMax};
use miniplan::pddl_io::load_combined_str;
use miniplan::plan::Plan;
use miniplan::search::{Astar, Bfs, BiDij, Gbfs, Planner, SearchLimits, SearchOutcome};
use miniplan::task::Task;
use proptest::prelude::*;

/// Largest number of graph nodes a generated case uses.
const MAX_NODES: usize = 7;

/// Domain shared by every case: one token moves along directed edges.
const DOMAIN: &str = "
(define (domain graph-walk)
    (:requirements :strips :typing)
    (:types node)
    (:predicates (at ?n - node) (edge ?from ?to - node))
    (:action move
        :parameters (?from ?to - node)
        :precondition (and (at ?from) (edge ?from ?to))
        :effect (and (at ?to) (not (at ?from)))))
";

/// A generated graph-walk problem: the token starts at node 0 and must reach `goal`.
#[derive(Debug, Clone)]
struct GraphCase {
    /// Number of nodes, named `n0` to `n{nodes - 1}`. At least 2.
    nodes: usize,
    /// Directed edges as `(from, to)` node indices, without self-loops.
    edges: Vec<(usize, usize)>,
    /// Index of the goal node. Never 0, so every plan has at least one step.
    goal: usize,
}

impl GraphCase {
    /// Renders the domain and problem as one PDDL source string.
    fn to_pddl(&self) -> String {
        let objects: Vec<String> = (0..self.nodes).map(|n| format!("n{n}")).collect();
        let edges: String = self
            .edges
            .iter()
            .map(|(from, to)| format!(" (edge n{from} n{to})"))
            .collect();

        format!(
            "{DOMAIN}\n(define (problem walk) (:domain graph-walk)\n  (:objects {} - node)\n  (:init (at n0){edges})\n  (:goal (at n{})))",
            objects.join(" "),
            self.goal
        )
    }

    /// Length of the shortest path from node 0 to the goal, or `None` if unreachable.
    fn shortest_path_len(&self) -> Option<usize> {
        let mut distance = vec![None; self.nodes];
        distance[0] = Some(0);
        let mut queue = VecDeque::from([0]);

        while let Some(node) = queue.pop_front() {
            let next_distance = distance[node].map(|d: usize| d + 1);
            for &(_, to) in self.edges.iter().filter(|(from, _)| *from == node) {
                if distance[to].is_none() {
                    distance[to] = next_distance;
                    queue.push_back(to);
                }
            }
        }

        distance[self.goal]
    }
}

/// Strategy for [`GraphCase`]s with 2 to [`MAX_NODES`] nodes and random edges.
fn graph_case() -> impl Strategy<Value = GraphCase> {
    (2..=MAX_NODES).prop_flat_map(|nodes| {
        let edge = (0..nodes, 0..nodes).prop_filter("no self-loops", |(a, b)| a != b);
        let edges = proptest::collection::vec(edge, 0..nodes * 2);

        (edges, 1..nodes).prop_map(move |(edges, goal)| GraphCase { nodes, edges, goal })
    })
}

/// Grounds a generated case. Generated PDDL is always well-formed, so failures are bugs.
fn ground_case(case: &GraphCase) -> Task {
    let (domain, problem) = load_combined_str(&case.to_pddl()).expect("generated PDDL parses");

    ground(&domain, &problem).expect("generated problem grounds")
}

/// Returns `true` when `plan` is applicable step by step from the initial state and ends in
/// a goal state.
fn plan_reaches_goal(task: &Task, plan: &Plan) -> bool {
    let mut state = task.init.clone();
    for step in &plan.steps {
        let op = &task.operators[step.op_id.0];
        if !state.applicable(op) {
            return false;
        }

        state = state.apply(op);
    }

    state.satisfies(&task.goal_pos, &task.goal_neg)
}

/// Runs `planner` without limits and returns its plan, or `None` if it proves unsolvability.
///
/// # Panics
///
/// Panics when the planner errors or reports a limit, since no limits are set.
fn solve(planner: &mut dyn Planner, task: &Task) -> Option<Plan> {
    match planner.solve(task, &SearchLimits::default()) {
        Ok(SearchOutcome::Plan(plan, _)) => Some(plan),
        Ok(SearchOutcome::Unsolvable(_)) => None,
        other => panic!("{} returned {other:?}", planner.name()),
    }
}

/// The planners under test, paired with whether they guarantee shortest plans.
fn planners() -> Vec<(Box<dyn Planner>, bool)> {
    vec![
        (Box::new(Bfs::new()), true),
        (Box::new(Astar::new(Box::new(HMax))), true),
        (Box::new(BiDij::new()), true),
        (Box::new(Gbfs::new(Box::new(HFF))), false),
    ]
}

proptest! {
    /// Every planner agrees with the oracle on solvability, returns executable plans, and the
    /// optimal planners return plans of the oracle's length.
    #[test]
    fn plans_are_valid_and_optimal_where_promised(case in graph_case()) {
        let task = ground_case(&case);
        let oracle = case.shortest_path_len();

        for (mut planner, optimal) in planners() {
            let plan = solve(planner.as_mut(), &task);
            prop_assert_eq!(plan.is_some(), oracle.is_some(), "{} solvability", planner.name());

            if let (Some(plan), Some(shortest)) = (plan, oracle) {
                prop_assert!(plan_reaches_goal(&task, &plan), "{} plan invalid", planner.name());
                if optimal {
                    prop_assert_eq!(plan.len(), shortest, "{} not optimal", planner.name());
                }
            }
        }
    }
}

/// Regression: `BiDij` looped forever when the goal was unreachable but backward regression
/// kept producing new subgoals (here through the `n3 <-> n5` cycle).
#[test]
fn bidij_reports_unreachable_goal_as_unsolvable() {
    let case = GraphCase {
        nodes: 6,
        edges: vec![(0, 2), (3, 5), (5, 3)],
        goal: 5,
    };

    assert!(solve(&mut BiDij::new(), &ground_case(&case)).is_none());
}
