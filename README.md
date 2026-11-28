# miniplan

A PDDL planner library built around the [`pddl`](https://github.com/sunsided/pddl-rs) crate.

[![CI](https://img.shields.io/github/actions/workflow/status/sunsided/miniplan/ci.yml?branch=main&label=CI)](https://github.com/sunsided/miniplan/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/miniplan)](https://crates.io/crates/miniplan)
[![Crates.io](https://img.shields.io/crates/v/miniplan-cli)](https://crates.io/crates/miniplan-cli)
[![docs.rs](https://img.shields.io/docsrs/miniplan)](https://docs.rs/miniplan)
[![codecov](https://codecov.io/gh/sunsided/miniplan/graph/badge.svg)](https://codecov.io/gh/sunsided/miniplan)
[![MSRV](https://img.shields.io/crates/msrv/miniplan)](https://crates.io/crates/miniplan)
[![License](https://img.shields.io/badge/license-EUPL--1.2-blue)](https://spdx.org/licenses/EUPL-1.2.html)
![Rust Edition](https://img.shields.io/badge/edition-2024-orange?logo=rust)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
<!-- [![GitHub Stars](https://img.shields.io/github/stars/sunsided/miniplan?style=social)](https://github.com/sunsided/miniplan) -->

**miniplan** is a grounded, classical PDDL planner that you can use as a Rust library or as the `miniplan` command-line tool. It reads PDDL with the [`pddl`](https://github.com/sunsided/pddl-rs) crate, grounds the task into a compact bitset representation and searches it with breadth-first search, A*, greedy best-first search or one of four bidirectional searches (bidirectional Dijkstra and BFS, NBS and BAE*), guided by h^FF, h^add, h^max or the goal count. It handles STRIPS with typing, negative preconditions, conditional effects, disjunctive preconditions and derived predicates, and writes plans as plain text, in IPC format or as JSON. The CLI also has an interactive terminal UI for trying planners on a problem.

## Who this is for

- Students and engineers learning classical planning, who want a planner small enough to read end to end, with the textbook searches and heuristics side by side.
- Rust developers who need to solve small and medium PDDL tasks (puzzles, logistics, robot task sequencing) in-process, without shelling out to an external planner.
- Researchers comparing forward, bidirectional and heuristic search on the same grounded task, with node counts and timings from `--stats`.
- If you only need to parse, check or write PDDL, use [`pddl-rs`](https://github.com/sunsided/pddl-rs) directly. For heuristic search over your own graph rather than a PDDL task, see [`heuristic-search`](https://github.com/sunsided/heuristic-search), and for width-based search without a heuristic, [`width-search`](https://github.com/sunsided/width-search).

Part of the [planning and search crates](https://sunsided.github.io/planning-and-search/); see also this crate's [overview page](https://sunsided.github.io/planning-and-search/crates/miniplan/).

## Features

- **PDDL 3.x parsing** via [pddl-rs](https://github.com/sunsided/pddl-rs)
- **Grounding** to a compact bitset-based internal representation
- **Planners**:
  - `bfs` — blind breadth-first search (optimal)
  - `astar` — A* with pluggable heuristics (optimal)
  - `gbfs` — greedy best-first search (fast, non-optimal)
  - `bidij` — bidirectional Dijkstra (cost-aware, optimal)
  - `bibfs-uc` — bidirectional BFS (uniform-cost, optimal)
  - `nbs` — Near-Optimal Bidirectional Search (heuristic-guided, non-optimal with HFF)
  - `bae` — Bidirectional A* with Error / BAE* (heuristic-guided, non-optimal with HFF)
- **Bidirectional heuristic search**: `bidij`, `bibfs-uc`, `nbs` (NBS), `bae` (BAE*)
- **Heuristics**: `hff` (relaxed-plan), `hadd`, `hmax`, `goal-count`, `blind`
- **Output formats**: plain, IPC, JSON
- **Search limits**: time budget, node budget
- **Structured logging** via `tracing`
- **Interactive terminal UI** (`miniplan tui`) for picking a problem, tuning the solver and viewing the plan

## Installation

### Library

```toml
[dependencies]
miniplan = "0.2"
```

### CLI

```bash
cargo install miniplan-cli --locked
```

The resulting binary is `miniplan` (not `miniplan-cli`).

Prebuilt binaries for Linux (glibc and static musl), macOS and Windows are attached to each
[GitHub Release](https://github.com/sunsided/miniplan/releases). Each one comes with a
CycloneDX SBOM and build attestations; verify a download with:

```bash
gh attestation verify <archive-or-binary> --repo sunsided/miniplan
```

### Build from source

```bash
git clone https://github.com/sunsided/miniplan
cd miniplan
cargo build --release
```

## Quick Start (CLI)

Solve a bundled example with search statistics:

```text
$ miniplan solve --stats examples/pddl/blocksworld.pddl
; cost = 8
; length = 8
(unstack(a,c))
(put-down(a))
(pick-up(c))
(stack(c,d))
(pick-up(b))
(stack(b,c))
(pick-up(a))
(stack(a,b))
Search stats:
  Nodes expanded: 27
  Nodes generated: 45
  Plan cost: 8.00
  Plan length: 8
  Time: 3.451169ms
```

Parse and ground a problem without solving:

```bash
miniplan check examples/pddl/gripper.pddl
```

List available planners and heuristics:

```text
$ miniplan list-planners
Available planners:
  bfs          Breadth-first search
  astar        A* search with pluggable heuristic
  gbfs         Greedy best-first search
  bibfs-uc     Bidirectional BFS (uniform-cost, not cost-aware)
  bidij        Bidirectional Dijkstra (cost-aware)
  nbs        Near-Optimal Bidirectional Search (Chen et al. 2017)
  bae        Bidirectional A* with Error (BAE*, Sadhukhan 2013)

Available heuristics:
  goal-count
  hff
  blind
  hadd
  hmax
```

## CLI Reference

### Commands

| Command | Description |
|---------|-------------|
| `solve` | Solve a PDDL problem |
| `check` | Parse and ground a problem (no search) |
| `list-planners` | Show available planners and heuristics |
| `tui` | Interactive terminal UI: pick a domain and problem, tune the solver, run it and view the plan |

### `solve` Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-p, --planner` | `astar` | Planner to use (`bfs`, `astar`, `gbfs`, `bidij`, `bibfs-uc`, `nbs`, `bae`) |
| `-H, --heuristic` | `hff` (h^FF) | Heuristic (`hff`, `hadd`, `hmax`, `goal-count`, `blind`, `zero`); unknown names are rejected |
| `-t, --timeout` | — | Time budget (e.g. `30s`, `2m`) |
| `--max-nodes` | — | Maximum nodes to expand |
| `-o, --output` | `-` | Output file (`-` for stdout) |
| `--format` | `plain` | Output format (`plain`, `ipc`, `json`) |
| `--stats` | — | Print search statistics to stderr |
| `--domain` | — | Domain name (when input contains multiple) |
| `--problem` | — | Problem name (when input contains multiple) |
| `-v` | — | Verbosity (count-based: `-v` info, `-vv` debug, `-vvv` trace) |

### Exit Codes

| Code | Meaning |
|------|--------|
| 0 | Plan found |
| 1 | Provably unsolvable |
| 2 | Search limit reached (time or nodes) |
| 3 | Unknown outcome |

## Library Usage

```rust,no_run
use std::path::PathBuf;

use miniplan::pddl_io::load_files_named;
use miniplan::ground::ground;
use miniplan::search::{Solver, PlannerChoice, PlannerKind, PlannerConfig, SearchLimits, SearchOutcome};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load PDDL files (one combined, or domain + problem)
    let files = [PathBuf::from("examples/pddl/blocksworld.pddl")];
    let (domain, problem) = load_files_named(&files, None, None)?;

    // Ground the task
    let task = ground(&domain, &problem)?;

    // Configure and run the solver
    let solver = Solver::new();

    let mut config = PlannerConfig::default();
    config.opts.insert("heuristic".to_owned(), "hff".to_owned());

    let choice = PlannerChoice {
        kind: PlannerKind::Astar,
        heuristic: Some("hff".to_owned()),
        config,
    };

    let limits = SearchLimits::default();
    let outcome = solver.solve_task(&task, &choice, &limits)?;

    match outcome {
        SearchOutcome::Plan(plan, stats) => {
            println!("Plan found (cost={:.2}, length={}):", stats.plan_cost, stats.plan_length);
            println!("{}", plan);
        }
        SearchOutcome::Unsolvable(_) => {
            eprintln!("Problem is provably unsolvable.");
        }
        SearchOutcome::LimitReached(_) => {
            eprintln!("Search limit reached.");
        }
        // SearchOutcome is #[non_exhaustive].
        _ => eprintln!("Unknown search outcome."),
    }

    Ok(())
}
```

See [docs.rs](https://docs.rs/miniplan) for the full API.

## Supported PDDL Features

| Feature | Status |
|---------|--------|
| STRIPS | Supported |
| Typing | Supported |
| Negative preconditions | Supported |
| Conditional effects | Supported |
| Disjunctive preconditions | Supported (via DNF grounding) |
| Quantified preconditions | Partial |
| Derived predicates (static bodies) | Supported |
| Numeric fluents | Not yet |
| Durative actions | Not yet |

## Examples

Five PDDL problems are bundled in `examples/pddl/`:

| Problem | Domain | Recommended Planner |
|---------|--------|---------------------|
| `blocksworld.pddl` | Blocksworld | `astar` |
| `gripper-small.pddl` | Gripper | `bfs` |
| `gripper.pddl` | Gripper | `astar` |
| `logistics.pddl` | Logistics | `bfs` |
| `air-cargo.pddl` | Air Cargo | `astar` / `gbfs` |

Run all examples via the Taskfile:

```bash
task example        # Full demo sweep
task example:blocksworld   # Single example
```

## Development

This project uses [Task](https://taskfile.dev/) for automation.

```bash
task --list-all        # List all available tasks
task verify            # fmt + check + lint + test (full gate)
task example           # Run the full example suite
```

A [pre-commit](https://pre-commit.com/) configuration is included (`.pre-commit-config.yaml`) for lint checks before commits.

## Project Layout

```text
crates/miniplan/         Core library
  src/
    ground/              PDDL grounding
    heuristic/           Heuristic implementations (h^FF, h^add, h^max, …)
    pddl_io.rs           PDDL file loading
    plan.rs              Plan representation and formatting
    search/              Planner implementations (BFS, A*, GBFS, bidirectional searches)
    task.rs              Grounded task representation
    util/                Utilities (bitsets, etc.)
bins/miniplan-cli/       CLI binary (produces `miniplan`)
examples/pddl/           Bundled PDDL problems
Taskfile.dist.yaml       Task definitions
```

## Roadmap

- Landmark heuristics
- Plan validation
- More PDDL coverage: numeric fluents and durative actions

## MSRV

The minimum supported Rust version is **1.88** (let-chains). It is a signal, not a contract:
it may rise in a minor release when a dependency or a language feature needs it, and the
changelog says so.

## Contributing

See [CONTRIBUTING.md](https://github.com/sunsided/miniplan/blob/main/CONTRIBUTING.md).

## License

Licensed under the **European Union Public Licence 1.2** (EUPL-1.2). See the [SPDX entry](https://spdx.org/licenses/EUPL-1.2.html) or the [official EU page](https://joinup.ec.europa.eu/collection/eupl) for details.

The Rust example programs in `crates/miniplan/examples/` are public domain (CC0-1.0), so you
can copy them into anything. Per-file licensing is declared in
[`REUSE.toml`](https://github.com/sunsided/miniplan/blob/main/REUSE.toml).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in this work by you shall be licensed as above, without any additional terms or conditions.

## Acknowledgements

- [pddl-rs](https://github.com/sunsided/pddl-rs) — PDDL parser, forked and maintained for this project.
- Jörg Hoffmann & Bernhard Nebel — The FF planning system and relaxed-plan heuristic (Hoffmann & Nebel, *The FF Planning System: Fast Planning Generation*, JAIR 2001).
