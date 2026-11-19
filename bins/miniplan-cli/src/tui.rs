//! Interactive terminal UI: pick a domain and problem, tune the solver, run it and view the plan.
//!
//! The UI is a plain draw/poll/tick loop:
//!
//! ```mermaid
//! flowchart LR
//!     draw[view::render] --> poll[events::poll_event]
//!     poll --> handle[handle_event]
//!     handle -->|quit| exit([restore terminal])
//!     handle --> tick[App::tick]
//!     tick --> draw
//! ```
//!
//! Searches run on a background thread ([`solver`]); [`App::tick`] picks up their result.

use std::ops::ControlFlow;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossterm::event::{MouseEvent, MouseEventKind};

use miniplan::pddl_io::load_pddl_bundle;

mod app;
mod events;
mod solver;
mod view;

use app::{App, SolvingState};
use events::AppEvent;

/// How long one poll waits for input before the loop ticks and redraws anyway.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Loads `files`, takes over the terminal and runs the TUI until the user quits.
///
/// The terminal is restored even when the loop fails.
///
/// # Errors
///
/// Returns an error when the files cannot be loaded, contain no domain or no problem, or
/// drawing to the terminal fails.
pub fn run(files: &[impl AsRef<Path>]) -> Result<()> {
    let bundle = load_pddl_bundle(files).context("Failed to load PDDL files")?;

    if bundle.domains.is_empty() {
        bail!("no domain definition found in provided files");
    }

    if bundle.problems.is_empty() {
        bail!("no problem definition found in provided files");
    }

    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stderr(), crossterm::event::EnableMouseCapture).ok();
    let mut app = App::new(bundle);

    let res = run_app(&mut terminal, &mut app);

    crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture).ok();
    ratatui::restore();
    res
}

/// Runs the draw, poll and tick loop until an event asks to quit.
///
/// # Errors
///
/// Returns an error when drawing to the terminal fails.
fn run_app(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| view::render(f, app))?;

        if let Some(event) = events::poll_event(POLL_INTERVAL)
            && handle_event(app, event).is_break()
        {
            return Ok(());
        }

        app.tick();
    }
}

/// Applies one event to `app`; global shortcuts first, everything else goes to [`App::on_key`].
///
/// Returns [`ControlFlow::Break`] when the TUI should exit.
fn handle_event(app: &mut App, event: AppEvent) -> ControlFlow<()> {
    match event {
        AppEvent::Quit => return ControlFlow::Break(()),
        AppEvent::Mouse(mouse) => handle_mouse(app, mouse),
        AppEvent::Escape if app.solving_state == SolvingState::Running => cancel_solving(app),
        AppEvent::Char('r') if app.modal.is_none() => app.reset_to_start(),
        AppEvent::Char('w') if app.modal.is_none() => write_solved_plan(app),
        _ => {
            app.on_key(event);
            if app
                .outcome
                .as_ref()
                .is_some_and(|r| matches!(r, Err(e) if e == "quit"))
            {
                return ControlFlow::Break(());
            }
        }
    }

    ControlFlow::Continue(())
}

/// Forwards clicks and wheel scrolling to [`App::on_mouse`]; other mouse events are dropped.
fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    if let MouseEventKind::Down(_) | MouseEventKind::ScrollDown | MouseEventKind::ScrollUp =
        mouse.kind
    {
        app.on_mouse(mouse.kind, mouse.column, mouse.row);
    }
}

/// Abandons the running search: its channel is dropped and the outcome becomes "cancelled".
///
/// The solver thread keeps running in the background until it finishes on its own.
fn cancel_solving(app: &mut App) {
    app.rx = None;
    app.solving_state = SolvingState::Failed;
    app.outcome = Some(Err("cancelled".to_string()));
}

/// Writes the solved plan to the configured output and flashes the result.
fn write_solved_plan(app: &mut App) {
    let Some(plan) = &app.solved_plan else {
        app.set_flash_error("No plan to write".to_string());
        return;
    };

    let message = match crate::plan_writer::write_plan(plan, &app.output_format, &app.output_path) {
        Ok(()) => "Plan written successfully".to_string(),
        Err(e) => format!("Failed to write plan: {e}"),
    };
    app.set_flash_error(message);
}
