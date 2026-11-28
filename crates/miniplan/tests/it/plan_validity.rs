//! Property tests: plans are executable, and optimal planners find cheapest plans.
//!
//! Two generators feed the planners:
//!
//! - A random directed graph rendered as a PDDL "move a token" problem. A plain breadth-first
//!   search over the graph itself is the oracle for solvability and plan length.
//! - A random propositional task with multi-precondition operators, negative preconditions
//!   (also on initially-true facts) and non-unit costs. A uniform-cost search over the
//!   grounded states is the oracle for solvability and plan cost.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::fmt::Write as _;

use miniplan::ground::ground;
use miniplan::heuristic::{HFF, HMax, HZero};
use miniplan::pddl_io::load_combined_str;
use miniplan::plan::Plan;
use miniplan::search::{
    Astar, Bae, Bfs, BiDij, BibfsUc, Gbfs, Nbs, Planner, SearchLimits, SearchOutcome,
};
use miniplan::task::{State, Task};
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
pub(crate) struct GraphCase {
    /// Number of nodes, named `n0` to `n{nodes - 1}`. At least 2.
    nodes: usize,
    /// Directed edges as `(from, to)` node indices, without self-loops.
    edges: Vec<(usize, usize)>,
    /// Index of the goal node. Never 0, so every plan has at least one step.
    goal: usize,
}

impl GraphCase {
    /// Renders the domain and problem as one PDDL source string.
    pub(crate) fn to_pddl(&self) -> String {
        let objects: Vec<String> = (0..self.nodes).map(|n| format!("n{n}")).collect();
        let edges = self
            .edges
            .iter()
            .fold(String::new(), |mut edges, (from, to)| {
                write!(edges, " (edge n{from} n{to})").expect("writing to a String cannot fail");
                edges
            });

        format!(
            "{DOMAIN}\n(define (problem walk) (:domain graph-walk)\n  (:objects {} - node)\n  (:init (at n0){edges})\n  (:goal (at n{})))",
            objects.join(" "),
            self.goal
        )
    }

    /// Length of the shortest path from node 0 to the goal, or `None` if unreachable.
    pub(crate) fn shortest_path_len(&self) -> Option<usize> {
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
pub(crate) fn graph_case() -> impl Strategy<Value = GraphCase> {
    (2..=MAX_NODES).prop_flat_map(|nodes| {
        let edge = (0..nodes, 0..nodes).prop_filter("no self-loops", |(a, b)| a != b);
        let edges = proptest::collection::vec(edge, 0..nodes * 2);

        (edges, 1..nodes).prop_map(move |(edges, goal)| GraphCase { nodes, edges, goal })
    })
}

/// Grounds a generated case. Generated PDDL is always well-formed, so failures are bugs.
pub(crate) fn ground_case(case: &GraphCase) -> Task {
    let (domain, problem) = load_combined_str(&case.to_pddl()).expect("generated PDDL parses");

    ground(&domain, &problem).expect("generated problem grounds")
}

/// Returns `true` when `plan` is applicable step by step from the initial state and ends in
/// a goal state.
pub(crate) fn plan_reaches_goal(task: &Task, plan: &Plan) -> bool {
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

/// Runs `planner` with the default limits and returns its plan, or `None` if it proves
/// unsolvability.
///
/// # Panics
///
/// Panics when the planner errors or reports a limit. The tasks are tiny, so neither the
/// default node budget nor the default time budget is ever reached. Also panics when a
/// plan's statistics count fewer generated nodes than plan steps, since every step
/// generates at least one node.
fn solve(planner: &mut dyn Planner, task: &Task) -> Option<Plan> {
    match planner.solve(task, &SearchLimits::default()) {
        Ok(SearchOutcome::Plan(plan, stats)) => {
            assert!(
                stats.nodes_generated >= stats.plan_length as u64,
                "{} generated {} nodes for a {}-step plan",
                planner.name(),
                stats.nodes_generated,
                stats.plan_length
            );
            Some(plan)
        }
        Ok(SearchOutcome::Unsolvable(_)) => None,
        other => panic!("{} returned {other:?}", planner.name()),
    }
}

/// The planners under test, paired with whether they guarantee shortest plans.
///
/// The heuristic planners get admissible heuristics, since they only promise optimal plans
/// with one. `BibfsUc` is optimal by step count, which matches cost here because every
/// generated action costs 1.
fn planners() -> Vec<(Box<dyn Planner>, bool)> {
    vec![
        (Box::new(Bfs::new()), true),
        (Box::new(Astar::new(Box::new(HMax))), true),
        (Box::new(BiDij::new()), true),
        (Box::new(Nbs::new(Box::new(HMax))), true),
        (Box::new(Bae::new(Box::new(HZero))), true),
        (Box::new(BibfsUc::new()), true),
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

/// Largest number of facts a generated propositional case uses.
const MAX_FACTS: usize = 5;

/// Largest number of operators a generated propositional case uses.
const MAX_OPS: usize = 6;

/// One operator of a [`PropCase`], over fact indices.
#[derive(Debug, Clone)]
struct PropOp {
    /// Facts that must hold.
    pre_pos: Vec<usize>,
    /// Facts that must not hold.
    pre_neg: Vec<usize>,
    /// Facts the operator adds.
    add: Vec<usize>,
    /// Facts the operator deletes (never also added).
    del: Vec<usize>,
    /// Action cost, at least 1.
    cost: u32,
}

/// A generated propositional task over nullary facts `f0` to `f{facts - 1}`.
#[derive(Debug, Clone)]
pub(crate) struct PropCase {
    /// Number of facts.
    facts: usize,
    /// Facts true in the initial state.
    init: Vec<usize>,
    /// Facts the goal requires.
    goal: Vec<usize>,
    /// The operators, named `a0`, `a1`, ...
    ops: Vec<PropOp>,
}

impl PropCase {
    /// Renders the domain and problem as one PDDL source string.
    pub(crate) fn to_pddl(&self) -> String {
        let atoms = |facts: &[usize]| {
            facts.iter().fold(String::new(), |mut out, f| {
                write!(out, " (f{f})").expect("writing to a String cannot fail");
                out
            })
        };
        let negated = |facts: &[usize]| {
            facts.iter().fold(String::new(), |mut out, f| {
                write!(out, " (not (f{f}))").expect("writing to a String cannot fail");
                out
            })
        };

        let predicates = atoms(&(0..self.facts).collect::<Vec<_>>());
        let mut pddl = format!(
            "(define (domain prop)\n  (:requirements :strips :negative-preconditions :action-costs)\n  (:predicates{predicates})\n  (:functions (total-cost))"
        );
        for (i, op) in self.ops.iter().enumerate() {
            write!(
                pddl,
                "\n  (:action a{i} :parameters ()\n    :precondition (and{}{})\n    :effect (and{}{} (increase (total-cost) {})))",
                atoms(&op.pre_pos),
                negated(&op.pre_neg),
                atoms(&op.add),
                negated(&op.del),
                op.cost
            )
            .expect("writing to a String cannot fail");
        }
        write!(
            pddl,
            ")\n(define (problem prop-1) (:domain prop)\n  (:init{})\n  (:goal (and{})))",
            atoms(&self.init),
            atoms(&self.goal)
        )
        .expect("writing to a String cannot fail");

        pddl
    }
}

/// Strategy for a subset of `0..facts` with at most `max` elements.
fn fact_subset(facts: usize, max: usize) -> impl Strategy<Value = Vec<usize>> {
    proptest::collection::btree_set(0..facts, 0..=max.min(facts))
        .prop_map(|set| set.into_iter().collect())
}

/// Strategy for one [`PropOp`] over `facts` facts.
fn prop_op(facts: usize) -> impl Strategy<Value = PropOp> {
    (
        fact_subset(facts, 2),
        fact_subset(facts, 1),
        fact_subset(facts, 2),
        fact_subset(facts, 2),
        1..=5u32,
    )
        .prop_filter_map(
            "needs an add effect",
            |(pre_pos, pre_neg, add, del, cost)| {
                let pre_neg: Vec<usize> = pre_neg
                    .into_iter()
                    .filter(|f| !pre_pos.contains(f))
                    .collect();
                let del: Vec<usize> = del.into_iter().filter(|f| !add.contains(f)).collect();
                (!add.is_empty()).then_some(PropOp {
                    pre_pos,
                    pre_neg,
                    add,
                    del,
                    cost,
                })
            },
        )
}

/// Strategy for [`PropCase`]s with 2 to [`MAX_FACTS`] facts and 1 to [`MAX_OPS`] operators.
pub(crate) fn prop_case() -> impl Strategy<Value = PropCase> {
    (2..=MAX_FACTS).prop_flat_map(|facts| {
        (
            fact_subset(facts, facts),
            fact_subset(facts, 3).prop_filter("goal is not empty", |goal| !goal.is_empty()),
            proptest::collection::vec(prop_op(facts), 1..=MAX_OPS),
        )
            .prop_map(move |(init, goal, ops)| PropCase {
                facts,
                init,
                goal,
                ops,
            })
    })
}

/// Cheapest plan cost of `task` by uniform-cost search over grounded states, or `None` if the
/// goal is unreachable.
pub(crate) fn cheapest_cost(task: &Task) -> Option<u64> {
    let mut best: HashMap<State, u64> = HashMap::from([(task.init.clone(), 0)]);
    let mut open = BinaryHeap::from([(Reverse(0u64), 0usize)]);
    let mut states = vec![task.init.clone()];

    while let Some((Reverse(g), idx)) = open.pop() {
        let state = states[idx].clone();
        if best[&state] < g {
            continue;
        }
        if state.satisfies(&task.goal_pos, &task.goal_neg) {
            return Some(g);
        }

        for op in task.operators.iter().filter(|op| state.applicable(op)) {
            let next = state.apply(op);
            let next_g = g + u64::from(op.cost);
            if best.get(&next).is_none_or(|&old| next_g < old) {
                best.insert(next.clone(), next_g);
                states.push(next);
                open.push((Reverse(next_g), states.len() - 1));
            }
        }
    }

    None
}

/// The planners under test on [`PropCase`]s, paired with whether they guarantee cheapest
/// plans. BFS and `BibfsUc` minimise steps, not cost, so they only need valid plans here.
fn cost_planners() -> Vec<(Box<dyn Planner>, bool)> {
    vec![
        (Box::new(Bfs::new()), false),
        (Box::new(BibfsUc::new()), false),
        (Box::new(Gbfs::new(Box::new(HFF))), false),
        (Box::new(Astar::new(Box::new(HMax))), true),
        (Box::new(BiDij::new()), true),
        (Box::new(Nbs::new(Box::new(HMax))), true),
        (Box::new(Nbs::new(Box::new(HZero))), true),
        (Box::new(Bae::new(Box::new(HMax))), true),
        (Box::new(Bae::new(Box::new(HZero))), true),
    ]
}

proptest! {
    /// On propositional tasks with negative preconditions and action costs, every planner
    /// agrees with the oracle on solvability and returns executable plans, and the optimal
    /// planners return plans of the oracle's cost.
    #[test]
    fn cost_plans_are_valid_and_optimal_where_promised(case in prop_case()) {
        let (domain, problem) =
            load_combined_str(&case.to_pddl()).expect("generated PDDL parses");
        let task = ground(&domain, &problem).expect("generated problem grounds");
        let oracle = cheapest_cost(&task);

        for (mut planner, optimal) in cost_planners() {
            let plan = solve(planner.as_mut(), &task);
            prop_assert_eq!(plan.is_some(), oracle.is_some(), "{} solvability", planner.name());

            if let (Some(plan), Some(cheapest)) = (plan, oracle) {
                prop_assert!(plan_reaches_goal(&task, &plan), "{} plan invalid", planner.name());
                #[expect(clippy::cast_precision_loss, reason = "generated costs are tiny")]
                let cheapest = cheapest as f64;
                if optimal {
                    prop_assert!(
                        (plan.cost - cheapest).abs() < 1e-9,
                        "{} not optimal: {} > {cheapest}",
                        planner.name(),
                        plan.cost
                    );
                }
            }
        }
    }
}

/// Regression: a negative precondition on an initially-true fact made the relaxed planning
/// graph rate the goal unreachable, so NBS and BAE* reported the solvable grid as unsolvable.
#[test]
fn negative_precondition_on_initial_fact_is_relaxed_reachable() {
    let src = include_str!("../../../../examples/pddl/grid.pddl");
    let (domain, problem) = load_combined_str(src).expect("grid parses");
    let task = ground(&domain, &problem).expect("grid grounds");

    for mut planner in [
        Box::new(Nbs::new(Box::new(HMax))) as Box<dyn Planner>,
        Box::new(Bae::new(Box::new(HMax))),
    ] {
        let plan = solve(planner.as_mut(), &task);
        assert_eq!(plan.map(|p| p.len()), Some(5), "{}", planner.name());
    }
}

/// Regression: expanding a node in NBS and BAE* dropped another open node's entries from
/// their `g` heaps, so the stopping bound fired early and NBS returned the cost-6 plan
/// `a0, a1` instead of the cost-5 plan `a0, a2`.
#[test]
fn nbs_and_bae_keep_open_nodes_in_their_bounds() {
    let op = |pre_pos: &[usize], pre_neg: &[usize], add: &[usize], del: &[usize], cost| PropOp {
        pre_pos: pre_pos.to_vec(),
        pre_neg: pre_neg.to_vec(),
        add: add.to_vec(),
        del: del.to_vec(),
        cost,
    };
    let case = PropCase {
        facts: 2,
        init: vec![0],
        goal: vec![0, 1],
        ops: vec![
            op(&[], &[], &[1], &[0], 4),
            op(&[], &[0], &[0, 1], &[], 2),
            op(&[1], &[], &[0], &[], 1),
        ],
    };
    let (domain, problem) = load_combined_str(&case.to_pddl()).expect("case parses");
    let task = ground(&domain, &problem).expect("case grounds");

    for mut planner in [
        Box::new(Nbs::new(Box::new(HMax))) as Box<dyn Planner>,
        Box::new(Nbs::new(Box::new(HZero))),
        Box::new(Bae::new(Box::new(HMax))),
        Box::new(Bae::new(Box::new(HZero))),
    ] {
        let plan = solve(planner.as_mut(), &task).expect("case is solvable");
        assert!(
            (plan.cost - 5.0).abs() < 1e-9,
            "{}: cost {}",
            planner.name(),
            plan.cost
        );
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

/// Regression: `BibfsUc` looped forever on the same kind of unreachable goal, because it
/// only stopped once both frontiers were empty and backward regression never runs dry.
#[test]
fn bibfs_uc_reports_unreachable_goal_as_unsolvable() {
    let case = GraphCase {
        nodes: 6,
        edges: vec![(0, 2), (3, 5), (5, 3)],
        goal: 5,
    };

    assert!(solve(&mut BibfsUc::new(), &ground_case(&case)).is_none());
}
