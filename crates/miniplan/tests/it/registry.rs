//! Tests for the planner registry, [`Solver`] and the planner/heuristic name plumbing.

use std::collections::HashSet;

use miniplan::error::MiniplanError;
use miniplan::ground::ground;
use miniplan::pddl_io::load_combined_str;
use miniplan::search::{
    HValue, PlannerChoice, PlannerConfig, PlannerKind, Registry, SearchLimits, SearchOutcome,
    Solver,
};
use miniplan::task::Task;

/// Every built-in planner kind.
const KINDS: [PlannerKind; 8] = [
    PlannerKind::Bfs,
    PlannerKind::Astar,
    PlannerKind::Gbfs,
    PlannerKind::BibfsUc,
    PlannerKind::BiDij,
    PlannerKind::Nbs,
    PlannerKind::Bae,
    PlannerKind::Pop,
];

/// Every valid value of the `heuristic` option.
const HEURISTIC_OPTIONS: [&str; 6] = ["hadd", "hmax", "hff", "blind", "goal-count", "zero"];

/// The planner kinds that take a forward heuristic from the `heuristic` option.
const HEURISTIC_KINDS: [PlannerKind; 4] = [
    PlannerKind::Astar,
    PlannerKind::Gbfs,
    PlannerKind::Nbs,
    PlannerKind::Bae,
];

/// The gripper example: large enough that heuristics change the number of expansions.
const GRIPPER: &str = include_str!("../../../../examples/pddl/gripper.pddl");

/// A two-step task: walk `a -> b -> c`. Every planner must find the 2-step plan.
const WALK: &str = "
(define (domain walk)
    (:requirements :strips)
    (:predicates (at-a) (at-b) (at-c))
    (:action ab :parameters () :precondition (at-a) :effect (and (at-b) (not (at-a))))
    (:action bc :parameters () :precondition (at-b) :effect (and (at-c) (not (at-b)))))
(define (problem walk-1) (:domain walk) (:init (at-a)) (:goal (at-c)))
";

/// Grounds [`WALK`].
fn walk_task() -> Task {
    let (domain, problem) = load_combined_str(WALK).expect("fixture parses");

    ground(&domain, &problem).expect("fixture grounds")
}

/// Solves `task` with `choice` and returns the plan length.
///
/// # Panics
///
/// Panics when the solver errors or finds no plan.
fn plan_len(solver: &Solver, task: &Task, choice: &PlannerChoice) -> usize {
    match solver
        .solve_task(task, choice, &SearchLimits::default())
        .expect("solve runs")
    {
        SearchOutcome::Plan(plan, stats) => {
            assert_eq!(stats.plan_length, plan.len(), "stats agree with the plan");
            plan.len()
        }
        other => panic!("{} found no plan: {other:?}", choice.kind),
    }
}

/// `Display`, `name()` and `FromStr` agree for every planner kind.
#[test]
fn planner_kind_names_round_trip() {
    for kind in KINDS {
        assert_eq!(kind.to_string(), kind.name());
        assert_eq!(kind.name().parse::<PlannerKind>(), Ok(kind));
    }
}

/// Unknown planner names are rejected with the list of valid names.
#[test]
fn unknown_planner_kind_lists_valid_names() {
    let err = "dfs"
        .parse::<PlannerKind>()
        .expect_err("dfs is not a planner");

    assert!(err.contains("unknown planner: dfs"), "{err}");
    for kind in KINDS {
        assert!(err.contains(kind.name()), "{err} lacks {}", kind.name());
    }
}

/// The built-in registry lists exactly the planner kinds, and every listed heuristic builds.
#[test]
fn builtin_registry_lists_all_planners_and_heuristics() {
    let registry = Registry::with_builtins();
    let planners: HashSet<&str> = registry.planners().map(|p| p.name).collect();
    let expected: HashSet<&str> = KINDS.iter().map(PlannerKind::name).collect();

    assert_eq!(planners, expected);

    let task = walk_task();
    let config = PlannerConfig::default();
    for registered in registry.heuristics() {
        let heuristic = registry
            .build_heuristic(registered.name, &config)
            .expect("listed heuristic builds");
        assert_ne!(heuristic.name(), "");
        assert!(heuristic.estimate(&task, &task.init).is_finite());
        assert_eq!(heuristic.preferred_ops(&task, &task.init).len(), 0);
    }
}

/// Unknown names fail with the matching error variant.
#[test]
fn registry_rejects_unknown_names() {
    let registry = Registry::with_builtins();
    let config = PlannerConfig::default();

    assert!(matches!(
        registry.build_planner("dfs", &config),
        Err(MiniplanError::InvalidPlanner(name)) if name == "dfs"
    ));
    assert!(matches!(
        registry.build_heuristic("h-magic", &config),
        Err(MiniplanError::InvalidHeuristic(name)) if name == "h-magic"
    ));
}

/// Every planner built through the solver finds the optimal 2-step plan, with every
/// valid `heuristic` option.
#[test]
fn solver_solves_with_every_planner_and_heuristic_option() {
    let solver = Solver::new();
    let task = walk_task();

    for kind in KINDS {
        let mut choice = PlannerChoice::new(kind);
        assert_eq!(plan_len(&solver, &task, &choice), 2, "{kind}");

        for option in HEURISTIC_OPTIONS {
            choice
                .config
                .opts
                .insert("heuristic".to_owned(), option.to_owned());
            assert_eq!(plan_len(&solver, &task, &choice), 2, "{kind} with {option}");
        }
    }
}

/// Heuristic planners, and `pop` with its rankings, reject an unknown `heuristic` option
/// instead of falling back.
#[test]
fn heuristic_planners_reject_unknown_heuristic() {
    let registry = Registry::with_builtins();
    let mut config = PlannerConfig::default();
    config
        .opts
        .insert("heuristic".to_owned(), "no-such".to_owned());

    for kind in HEURISTIC_KINDS.into_iter().chain([PlannerKind::Pop]) {
        assert!(
            matches!(
                registry.build_planner(kind.name(), &config),
                Err(MiniplanError::InvalidHeuristic(ref name)) if name == "no-such"
            ),
            "{kind}"
        );
    }
}

/// Heuristic planners search with the chosen heuristic: `zero` expands a different number
/// of nodes than the default hFF on gripper, and A* with the admissible `hmax` finds the
/// optimal 11-step plan.
#[test]
fn heuristic_planners_use_chosen_heuristic() {
    let (domain, problem) = load_combined_str(GRIPPER).expect("gripper parses");
    let task = ground(&domain, &problem).expect("gripper grounds");
    let solver = Solver::new();

    let solve = |kind: PlannerKind, heuristic: &str| {
        let mut choice = PlannerChoice::new(kind);
        choice
            .config
            .opts
            .insert("heuristic".to_owned(), heuristic.to_owned());
        match solver.solve_task(&task, &choice, &SearchLimits::default()) {
            Ok(SearchOutcome::Plan(_, stats)) => stats,
            other => panic!("{kind} with {heuristic} found no plan: {other:?}"),
        }
    };

    for kind in HEURISTIC_KINDS {
        assert_ne!(
            solve(kind, "zero").nodes_expanded,
            solve(kind, "hff").nodes_expanded,
            "{kind}"
        );
    }

    let astar_hmax = solve(PlannerKind::Astar, "hmax");
    assert!((astar_hmax.plan_cost - 11.0).abs() < 1e-9, "{astar_hmax:?}");
}

/// Every built planner has a name, a description and capabilities.
#[test]
fn built_planners_describe_themselves() {
    let registry = Registry::with_builtins();

    for kind in KINDS {
        let planner = registry
            .build_planner(kind.name(), &PlannerConfig::default())
            .expect("built-in planner builds");
        assert_ne!(planner.name(), "", "{kind}");
        assert_ne!(planner.describe(), "", "{kind}");
        assert!(!planner.capabilities().is_empty(), "{kind}");
    }
}

/// `HValue` compares and hashes by its `f64`, and infinity is not finite.
#[test]
fn hvalue_compares_and_hashes_by_value() {
    let one = HValue(1.0);

    assert_eq!(one, HValue(1.0));
    assert!(one < HValue(2.0));
    assert!(one.is_finite());
    assert!(!HValue::INFINITY.is_finite());
    assert!(one < HValue::INFINITY);

    assert_eq!(hash_of(HValue(1.0)), hash_of(HValue(1.0)));
    assert_ne!(hash_of(HValue(1.0)), hash_of(HValue(2.0)));
}

/// Hashes `value` with a fixed-key [`DefaultHasher`](std::hash::DefaultHasher).
fn hash_of(value: HValue) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};

    BuildHasherDefault::<DefaultHasher>::default().hash_one(value)
}
