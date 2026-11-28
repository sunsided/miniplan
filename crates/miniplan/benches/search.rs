//! Search benchmarks on the bundled example problems.
//!
//! Grounding happens once per problem outside the measured loop, so the numbers cover search
//! only. Run with `task bench`.

use std::hint::black_box;
use std::path::PathBuf;

use criterion::Criterion;
use miniplan::ground::ground;
use miniplan::heuristic::{HFF, HMax, HZero};
use miniplan::pddl_io::load_files_named;
use miniplan::search::{Astar, Bae, Bfs, BiDij, Gbfs, Nbs, Planner, SearchLimits, SearchOutcome};
use miniplan::task::Task;

/// Runs every benchmark in this file with criterion's command-line flags.
fn main() {
    let mut criterion = Criterion::default().configure_from_args();

    bench_gripper(&mut criterion);
    bench_blocksworld(&mut criterion);
    bench_air_cargo(&mut criterion);
    criterion.final_summary();
}

/// Gripper-4: blind, optimal heuristic and bidirectional searches on a small task.
fn bench_gripper(criterion: &mut Criterion) {
    let task = load_example("gripper.pddl", None);

    bench_planner(criterion, "gripper/bfs", &task, Bfs::new);
    bench_planner(criterion, "gripper/astar-hff", &task, || {
        Astar::new(Box::new(HFF))
    });
    bench_planner(criterion, "gripper/astar-hmax", &task, || {
        Astar::new(Box::new(HMax))
    });
    bench_planner(criterion, "gripper/bidij", &task, BiDij::new);
    bench_planner(criterion, "gripper/nbs-hmax", &task, || {
        Nbs::new(Box::new(HMax))
    });
    bench_planner(criterion, "gripper/bae-hmax", &task, || {
        Bae::new(Box::new(HMax))
    });
}

/// Blocksworld-4: A* with hFF on a deeper search space.
fn bench_blocksworld(criterion: &mut Criterion) {
    let task = load_example("blocksworld.pddl", None);

    bench_planner(criterion, "blocksworld/astar-hff", &task, || {
        Astar::new(Box::new(HFF))
    });
}

/// Air Cargo P3: greedy best-first search and the bidirectional searches on the largest
/// example.
fn bench_air_cargo(criterion: &mut Criterion) {
    let task = load_example("air-cargo.pddl", Some("air-cargo-p3"));

    bench_planner(criterion, "air-cargo-p3/gbfs-hff", &task, || {
        Gbfs::new(Box::new(HFF))
    });
    bench_planner(criterion, "air-cargo-p3/bidij", &task, BiDij::new);
    bench_planner(criterion, "air-cargo-p3/nbs-zero", &task, || {
        Nbs::new(Box::new(HZero))
    });
    bench_planner(criterion, "air-cargo-p3/bae-zero", &task, || {
        Bae::new(Box::new(HZero))
    });
}

/// Benchmarks one full `solve` call per iteration with a planner fresh from `make_planner`.
///
/// # Panics
///
/// Panics when the planner does not find a plan, since every example is solvable.
fn bench_planner<P: Planner>(
    criterion: &mut Criterion,
    name: &str,
    task: &Task,
    make_planner: impl Fn() -> P,
) {
    let limits = SearchLimits::default();

    criterion.bench_function(name, |bencher| {
        bencher.iter(|| {
            let outcome = make_planner()
                .solve(black_box(task), &limits)
                .expect("search runs");
            assert!(
                matches!(outcome, SearchOutcome::Plan(..)),
                "{name}: no plan"
            );
            outcome
        });
    });
}

/// Loads and grounds `examples/pddl/<file>`, picking `problem` when the file has several.
///
/// # Panics
///
/// Panics when the bundled example fails to load or ground.
fn load_example(file: &str, problem: Option<&str>) -> Task {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "examples",
        "pddl",
        file,
    ]
    .iter()
    .collect();
    let (domain, problem) = load_files_named(&[path], None, problem).expect("example loads");

    ground(&domain, &problem).expect("example grounds")
}
