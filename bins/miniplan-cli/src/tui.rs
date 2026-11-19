use std::path::Path;

use anyhow::{Context, Result, bail};
use crossterm::event::MouseEventKind;

use miniplan::pddl_io::load_pddl_bundle;

mod app;
mod events;
mod solver;
mod view;

use app::App;

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

fn run_app(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    use std::time::Duration;

    loop {
        terminal.draw(|f| view::render(f, app))?;

        let event = events::poll_event(Duration::from_millis(100));
        if let Some(event) = event {
            match event {
                events::AppEvent::Quit => {
                    return Ok(());
                }
                events::AppEvent::Mouse(mouse) => {
                    if let MouseEventKind::Down(_)
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollUp = mouse.kind
                    {
                        app.on_mouse(mouse.kind, mouse.column, mouse.row);
                    }
                }
                events::AppEvent::Escape
                    if app.solving_state == crate::tui::app::SolvingState::Running =>
                {
                    app.rx = None;
                    app.solving_state = crate::tui::app::SolvingState::Failed;
                    app.outcome = Some(Err("cancelled".to_string()));
                }
                events::AppEvent::Char('r') if app.modal.is_none() => {
                    app.reset_to_start();
                }
                events::AppEvent::Char('w') if app.modal.is_none() => {
                    if let Some(ref plan) = app.solved_plan {
                        let result = crate::plan_writer::write_plan(
                            plan,
                            &app.output_format,
                            &app.output_path,
                        );
                        match result {
                            Ok(()) => {
                                app.set_flash_error("Plan written successfully".to_string());
                            }
                            Err(e) => {
                                app.set_flash_error(format!("Failed to write plan: {}", e));
                            }
                        }
                    } else {
                        app.set_flash_error("No plan to write".to_string());
                    }
                }
                _ => {
                    app.on_key(event);
                    if app
                        .outcome
                        .as_ref()
                        .is_some_and(|r| matches!(r, Err(e) if e == "quit"))
                    {
                        return Ok(());
                    }
                }
            }
        }
        app.tick();
    }
}
