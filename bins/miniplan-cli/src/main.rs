#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use miniplan::ground::ground;
use miniplan::pddl_io::load_files_named;
use miniplan::search::{PlannerChoice, PlannerConfig, SearchLimits, SearchOutcome, Solver};

mod plan_writer;
mod tui;

use plan_writer::{OutputFormat, write_plan};

#[derive(Parser)]
#[command(name = "miniplan", about = "A small PDDL planner", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Verbosity level (-v info, -vv debug, -vvv trace)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Command {
    /// Solve a PDDL problem
    Solve {
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
        #[arg(short = 'H', long, default_value = "ff")]
        heuristic: String,

        /// Timeout duration (e.g. "30s", "2m")
        #[arg(short = 't', long)]
        timeout: Option<humantime::Duration>,

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
    },
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.verbose {
        0 => {}
        1 => {
            tracing_subscriber::fmt()
                .with_env_filter("miniplan=info")
                .init();
        }
        2 => {
            tracing_subscriber::fmt()
                .with_env_filter("miniplan=debug")
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .with_env_filter("miniplan=trace")
                .init();
        }
    }

    match cli.command {
        Command::Solve {
            input,
            domain,
            problem,
            planner,
            heuristic,
            timeout,
            max_nodes,
            output,
            format,
            stats,
        } => cmd_solve(
            &input,
            domain.as_deref(),
            problem.as_deref(),
            &planner,
            &heuristic,
            timeout,
            max_nodes,
            &output,
            &format,
            stats,
        ),
        Command::Check {
            input,
            domain,
            problem,
        } => cmd_check(&input, domain.as_deref(), problem.as_deref()),
        Command::ListPlanners => cmd_list_planners(),
        Command::Tui { input } => tui::run(&input),
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_solve(
    inputs: &[PathBuf],
    domain_name: Option<&str>,
    problem_name: Option<&str>,
    planner_name: &str,
    heuristic_name: &str,
    timeout: Option<humantime::Duration>,
    max_nodes: Option<u64>,
    output: &str,
    format: &OutputFormat,
    print_stats: bool,
) -> Result<()> {
    let solver = Solver::new();

    let (domain, problem) =
        load_files_named(inputs, domain_name, problem_name).context("Failed to load PDDL files")?;
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
        time_budget: timeout.map(|d| d.into()),
        node_budget: max_nodes,
        memory_mb: None,
    };

    let mut config = PlannerConfig::default();
    config
        .opts
        .insert("heuristic".to_owned(), heuristic_name.to_owned());

    let choice = PlannerChoice {
        planner: planner_name.to_owned(),
        heuristic: Some(heuristic_name.to_owned()),
        config,
    };

    let outcome = solver
        .solve_task(&task, &choice, &limits)
        .context("Search failed")?;

    match outcome {
        SearchOutcome::Plan(plan, stats) => {
            write_plan(&plan, format, output)?;
            if print_stats {
                eprintln!("Search stats:");
                eprintln!("  Nodes expanded: {}", stats.nodes_expanded);
                eprintln!("  Nodes generated: {}", stats.nodes_generated);
                eprintln!("  Plan cost: {:.2}", stats.plan_cost);
                eprintln!("  Plan length: {}", stats.plan_length);
                eprintln!("  Time: {:?}", stats.elapsed);
            }
            process::exit(0);
        }
        SearchOutcome::Unsolvable(stats) => {
            eprintln!("Problem is unsolvable.");
            if print_stats {
                eprintln!("Search stats:");
                eprintln!("  Nodes expanded: {}", stats.nodes_expanded);
                eprintln!("  Time: {:?}", stats.elapsed);
            }
            process::exit(1);
        }
        SearchOutcome::LimitReached(stats) => {
            eprintln!("Search limit reached.");
            if print_stats {
                eprintln!("Search stats:");
                eprintln!("  Nodes expanded: {}", stats.nodes_expanded);
                eprintln!("  Time: {:?}", stats.elapsed);
            }
            process::exit(2);
        }
        _ => {
            eprintln!("Unknown search outcome.");
            process::exit(3);
        }
    }
}

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

fn cmd_list_planners() -> Result<()> {
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
    Ok(())
}
