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
#[cfg(test)]
mod test_support;
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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseButton};

    use super::*;
    use crate::plan_writer::OutputFormat;
    use crate::tui::app::{AppFocus, ModalKind};
    use crate::tui::test_support::{draw, gripper_app, temp_path, wait_for_solve};

    /// A mouse event of `kind` at `column`, `row` without modifiers.
    const fn mouse(kind: MouseEventKind, column: u16, row: u16) -> AppEvent {
        AppEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    /// An app over gripper-2 whose search has already finished with a plan.
    fn solved_app() -> App {
        let mut app = gripper_app();
        app.on_key(AppEvent::Char('s'));
        wait_for_solve(&mut app);
        app
    }

    /// Ctrl+C and `q` end the loop; ordinary keys keep it running.
    #[test]
    fn quit_events_break() {
        let mut app = gripper_app();

        assert!(handle_event(&mut app, AppEvent::Tick).is_continue());
        assert!(handle_event(&mut app, AppEvent::Down).is_continue());
        assert!(handle_event(&mut app, AppEvent::Quit).is_break());
        assert!(handle_event(&mut app, AppEvent::Char('q')).is_break());
    }

    /// Escape during a search cancels it: the channel is dropped and the outcome says so.
    #[test]
    fn escape_cancels_running_search() {
        let mut app = gripper_app();
        app.on_key(AppEvent::Char('s'));
        assert_eq!(app.solving_state, SolvingState::Running);

        let flow = handle_event(&mut app, AppEvent::Escape);

        assert!(flow.is_continue());
        assert!(app.rx.is_none());
        assert_eq!(app.solving_state, SolvingState::Failed);
        assert!(matches!(app.outcome, Some(Err(ref e)) if e == "cancelled"));
    }

    /// `r` resets to the domain list, but not while a modal is open.
    #[test]
    fn r_resets_outside_modal() {
        let mut app = solved_app();
        app.focus = AppFocus::CommandBar;

        assert!(handle_event(&mut app, AppEvent::Char('r')).is_continue());
        assert_eq!(app.focus, AppFocus::DomainList);
        assert!(app.solved_plan.is_none());

        app.modal = Some(ModalKind::Heuristic);
        app.focus = AppFocus::CommandBar;
        assert!(handle_event(&mut app, AppEvent::Char('r')).is_continue());
        assert_eq!(app.focus, AppFocus::CommandBar);
        assert_eq!(app.modal, Some(ModalKind::Heuristic));
    }

    /// `w` without a plan flashes a notice instead of writing.
    #[test]
    fn w_without_plan_flashes() {
        let mut app = gripper_app();

        assert!(handle_event(&mut app, AppEvent::Char('w')).is_continue());

        assert_eq!(app.flash_error.as_deref(), Some("No plan to write"));
    }

    /// `w` writes the solved plan to the output path in the chosen format.
    #[test]
    fn w_writes_plan_to_file() {
        let mut app = solved_app();
        let path = temp_path("w-writes.json");
        app.output_path = path.to_string_lossy().into_owned();
        app.output_format = OutputFormat::Json;

        assert!(handle_event(&mut app, AppEvent::Char('w')).is_continue());

        let written = std::fs::read_to_string(&path).expect("plan file written");
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            app.flash_error.as_deref(),
            Some("Plan written successfully")
        );
        assert!(written.contains("\"length\": 7"), "{written}");
    }

    /// A failing write is reported in the flash line.
    #[test]
    fn w_reports_write_failure() {
        let mut app = solved_app();
        app.output_path = temp_path("no-such-dir")
            .join("plan.txt")
            .to_string_lossy()
            .into_owned();

        assert!(handle_event(&mut app, AppEvent::Char('w')).is_continue());

        let flash = app.flash_error.expect("flash is set");
        assert!(flash.starts_with("Failed to write plan: "), "{flash}");
    }

    /// Clicks and wheel events reach the app; pointer moves are dropped.
    #[test]
    fn mouse_events_are_forwarded() {
        let mut app = gripper_app();
        draw(&mut app, 120, 40);
        let plan = app.areas.plan.expect("plan area");
        let (col, row) = (plan.x + 1, plan.y + 1);

        assert!(handle_event(&mut app, mouse(MouseEventKind::Moved, col, row)).is_continue());
        assert_eq!(app.focus, AppFocus::DomainList);

        let flow = handle_event(
            &mut app,
            mouse(MouseEventKind::Down(MouseButton::Left), col, row),
        );
        assert!(flow.is_continue());
        assert_eq!(app.focus, AppFocus::PlanScroll);

        assert!(handle_event(&mut app, mouse(MouseEventKind::ScrollDown, col, row)).is_continue());
        assert!(handle_event(&mut app, mouse(MouseEventKind::ScrollUp, col, row)).is_continue());
        assert_eq!(app.plan_scroll_offset, 0);
    }
}
