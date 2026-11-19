//! Shared fixtures for the TUI unit tests: example bundles, a ready [`App`], rendering into a
//! test backend and a bounded wait for background searches.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use miniplan::pddl_io::{PddlBundle, load_pddl_bundle};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::app::{App, SolvingState};
use super::view;

/// Longest a test waits for a background search before it fails instead of hanging.
const SOLVE_DEADLINE: Duration = Duration::from_secs(20);

/// Loads the given files of the repository's `examples/pddl` directory into one bundle.
///
/// # Panics
///
/// Panics when an example cannot be loaded.
pub fn bundle(names: &[&str]) -> PddlBundle {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/pddl");
    let paths: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
    load_pddl_bundle(&paths).expect("example PDDL files load")
}

/// An [`App`] over `gripper-small.pddl`: one domain (`gripper`) and one problem (`gripper-2`).
pub fn gripper_app() -> App {
    App::new(bundle(&["gripper-small.pddl"]))
}

/// Draws one frame of `app` into a `width` x `height` test terminal and returns its text,
/// one line per row.
///
/// Drawing also records the hit areas, so mouse tests can click real coordinates afterwards.
///
/// # Panics
///
/// Panics when drawing fails.
pub fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|f| view::render(f, app))
        .expect("render succeeds");

    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ticks `app` until its search leaves [`SolvingState::Running`].
///
/// # Panics
///
/// Panics when the search does not finish within [`SOLVE_DEADLINE`].
pub fn wait_for_solve(app: &mut App) {
    let deadline = Instant::now() + SOLVE_DEADLINE;
    while app.solving_state == SolvingState::Running {
        assert!(Instant::now() < deadline, "search did not finish in time");
        std::thread::sleep(Duration::from_millis(1));
        app.tick();
    }
}

/// A path in the system temp directory that is unique to this process and `tag`.
pub fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("miniplan-tui-{}-{tag}", std::process::id()))
}
