//! Command-line front end for the miniplan PDDL planner.
//!
//! Parses the arguments with clap, then dispatches to one subcommand: `solve` runs a planner
//! and writes the plan, `check` only parses and grounds, `list-planners` prints the registry,
//! and `tui` starts the interactive terminal UI.
//!
//! `solve` exits with code 0 when a plan was found, 1 when the problem is unsolvable, 2 when
//! a search limit was hit and 3 for an unknown outcome.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use miniplan::ground::ground;
use miniplan::pddl_io::load_files_named;
use miniplan::search::{
    PlannerChoice, PlannerConfig, PlannerKind, SearchLimits, SearchOutcome, SearchStats, Solver,
};

mod duration;
mod plan_writer;
mod tui;

use plan_writer::{OutputFormat, write_plan};

/// Top-level command-line arguments.
#[derive(Parser)]
#[command(name = "miniplan", about = "A small PDDL planner", version)]
struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    command: Command,

    /// Verbosity level (-v info, -vv debug, -vvv trace)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,
}

/// The subcommands of the CLI.
#[derive(Subcommand)]
enum Command {
    /// Solve a PDDL problem
    Solve(SolveArgs),
    /// Parse and ground a PDDL problem (no search)
    Check {
        /// Input PDDL file(s)
        #[arg(required = true)]
        input: Vec<PathBuf>,

        /// Domain name to select (when input contains multiple domains)
        #[arg(long)]
        domain: Option<String>,

        /// Problem name to select (when input contains multiple problems)
        #[arg(long)]
        problem: Option<String>,
    },
    /// List available planners and heuristics
    ListPlanners,
    /// Launch the interactive TUI
    Tui {
        /// Input PDDL file(s) — any mix of domains and problems
        #[arg(required = true)]
        input: Vec<PathBuf>,
    },
}

/// Arguments of the `solve` subcommand.
#[derive(Args)]
struct SolveArgs {
    /// Input PDDL file(s) — one combined, or domain + problem
    #[arg(required = true)]
    input: Vec<PathBuf>,

    /// Domain name to select (when input contains multiple domains)
    #[arg(long)]
    domain: Option<String>,

    /// Problem name to select (when input contains multiple problems)
    #[arg(long)]
    problem: Option<String>,

    /// Planner to use
    #[arg(short = 'p', long, default_value = "astar")]
    planner: String,

    /// Heuristic to use (ignored for blind planners)
    #[arg(short = 'H', long, default_value = "hff")]
    heuristic: String,

    /// Timeout duration (e.g. "30s", "2m")
    #[arg(short = 't', long, value_parser = duration::parse_duration)]
    timeout: Option<Duration>,

    /// Maximum number of nodes to expand
    #[arg(long)]
    max_nodes: Option<u64>,

    /// Output file for the plan ("-" for stdout)
    #[arg(short = 'o', long, default_value = "-")]
    output: String,

    /// Output format
    #[arg(long, default_value = "plain")]
    format: OutputFormat,

    /// Print search statistics to stderr
    #[arg(long)]
    stats: bool,
}

/// Parses the command line, sets up logging and runs the chosen subcommand.
///
/// # Errors
///
/// Returns the error of the subcommand: loading, grounding, planner selection, search or
/// writing the plan failed.
fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::Solve(args) => cmd_solve(&args),
        Command::Check {
            input,
            domain,
            problem,
        } => cmd_check(&input, domain.as_deref(), problem.as_deref()),
        Command::ListPlanners => {
            cmd_list_planners();
            Ok(())
        }
        Command::Tui { input } => tui::run(&input),
    }
}

/// Installs a tracing subscriber for the `miniplan` target at the level `verbose` asks for.
///
/// `0` installs nothing, `1` logs at info, `2` at debug and anything higher at trace.
fn init_tracing(verbose: u8) {
    let filter = match verbose {
        0 => return,
        1 => "miniplan=info",
        2 => "miniplan=debug",
        _ => "miniplan=trace",
    };

    // Logs go to stderr so they never mix with the plan on stdout.
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

/// Runs the `solve` subcommand: load, ground, search, then report the outcome.
///
/// Never returns on a completed search: the process exits with the code of the outcome
/// (see the crate docs).
///
/// # Errors
///
/// Returns an error when the files cannot be loaded, the task cannot be grounded, the
/// planner name is unknown, the search fails or the plan cannot be written.
fn cmd_solve(args: &SolveArgs) -> Result<()> {
    let solver = Solver::new();

    let (domain, problem) =
        load_files_named(&args.input, args.domain.as_deref(), args.problem.as_deref())
            .context("Failed to load PDDL files")?;
    tracing::info!(
        "Loaded domain '{}' and problem '{}'",
        domain.name().to_string(),
        problem.name().to_string()
    );

    let task = ground(&domain, &problem).context("Failed to ground task")?;
    tracing::info!(
        "Grounded task: {} facts, {} operators",
        task.num_facts(),
        task.operators.len()
    );

    let limits = SearchLimits {
        time_budget: args.timeout,
        node_budget: args.max_nodes,
        memory_mb: None,
    };
    let choice = planner_choice(&args.planner, &args.heuristic)?;

    let outcome = solver
        .solve_task(&task, &choice, &limits)
        .context("Search failed")?;

    report_outcome(outcome, args)
}

/// Builds the planner selection for `planner_name`, always passing `heuristic_name` along.
///
/// # Errors
///
/// Returns an error when `planner_name` is not a known [`PlannerKind`].
fn planner_choice(planner_name: &str, heuristic_name: &str) -> Result<PlannerChoice> {
    let kind: PlannerKind = planner_name.parse().map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut config = PlannerConfig::default();
    config
        .opts
        .insert("heuristic".to_owned(), heuristic_name.to_owned());

    Ok(PlannerChoice {
        kind,
        heuristic: Some(heuristic_name.to_owned()),
        config,
    })
}

/// Writes the plan or a failure message, prints statistics if asked, and exits the process.
///
/// Exit codes: 0 plan found, 1 unsolvable, 2 limit reached, 3 unknown outcome.
///
/// # Errors
///
/// Returns an error only when a found plan cannot be written; otherwise it never returns.
fn report_outcome(outcome: SearchOutcome, args: &SolveArgs) -> Result<()> {
    match outcome {
        SearchOutcome::Plan(plan, stats) => {
            write_plan(&plan, &args.format, &args.output)?;
            if args.stats {
                print_full_stats(&stats);
            }

            process::exit(0);
        }
        SearchOutcome::Unsolvable(stats) => {
            eprintln!("Problem is unsolvable.");
            if args.stats {
                print_short_stats(&stats);
            }

            process::exit(1);
        }
        SearchOutcome::LimitReached(stats) => {
            eprintln!("Search limit reached.");
            if args.stats {
                print_short_stats(&stats);
            }

            process::exit(2);
        }
        _ => {
            eprintln!("Unknown search outcome.");
            process::exit(3);
        }
    }
}

/// Prints all statistics of a successful search to stderr.
fn print_full_stats(stats: &SearchStats) {
    eprintln!("Search stats:");
    eprintln!("  Nodes expanded: {}", stats.nodes_expanded);
    eprintln!("  Nodes generated: {}", stats.nodes_generated);
    eprintln!("  Plan cost: {:.2}", stats.plan_cost);
    eprintln!("  Plan length: {}", stats.plan_length);
    eprintln!("  Time: {:?}", stats.elapsed);
}

/// Prints the expansion count and elapsed time of a search that found no plan to stderr.
fn print_short_stats(stats: &SearchStats) {
    eprintln!("Search stats:");
    eprintln!("  Nodes expanded: {}", stats.nodes_expanded);
    eprintln!("  Time: {:?}", stats.elapsed);
}

/// Runs the `check` subcommand: load and ground the task, then print its size.
///
/// # Errors
///
/// Returns an error when the files cannot be loaded or the task cannot be grounded.
fn cmd_check(
    inputs: &[PathBuf],
    domain_name: Option<&str>,
    problem_name: Option<&str>,
) -> Result<()> {
    let (domain, problem) =
        load_files_named(inputs, domain_name, problem_name).context("Failed to load PDDL files")?;
    tracing::info!(
        "Loaded domain '{}' and problem '{}'",
        domain.name().to_string(),
        problem.name().to_string()
    );

    let task = ground(&domain, &problem).context("Failed to ground task")?;
    println!("Task grounded successfully:");
    println!("  Facts: {}", task.num_facts());
    println!("  Operators: {}", task.operators.len());
    println!("  Objects: {}", task.objects.len());
    println!("  Domain: {}", task.metadata.domain_name);
    println!("  Problem: {}", task.metadata.problem_name);

    Ok(())
}

/// Runs the `list-planners` subcommand: prints every registered planner and heuristic.
fn cmd_list_planners() {
    let solver = Solver::new();

    println!("Available planners:");
    for p in solver.registry.planners() {
        println!("  {:<12} {}", p.name, p.description);
    }

    println!();
    println!("Available heuristics:");
    for h in solver.registry.heuristics() {
        println!("  {}", h.name);
    }
}
