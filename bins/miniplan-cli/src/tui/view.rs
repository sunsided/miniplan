//! Rendering of the TUI from [`App`] state with ratatui.
//!
//! Layout, top to bottom: domain and problem lists side by side (8 rows), then solver
//! settings (35 %) next to statistics and plan (65 %), then the command bar (3 rows). An open
//! modal and the flash message are drawn on top.
//!
//! Rendering also records the screen areas in [`App::areas`] so mouse clicks can be routed.

use std::rc::Rc;

use miniplan::search::SearchOutcome;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState,
};

use crate::tui::app::{App, AppFocus, InputField, ModalKind, SolvingState};

/// Screen areas of all panels for one frame.
struct ScreenLayout {
    /// Domain list panel.
    domain: Rect,
    /// Problem list panel.
    problem: Rect,
    /// Solver settings panel, border included.
    config: Rect,
    /// Rows inside the settings panel: planner, heuristic, input fields, solve button.
    config_rows: Rc<[Rect]>,
    /// Statistics panel.
    stats: Rect,
    /// Plan panel.
    plan: Rect,
    /// Command bar.
    commands: Rect,
}

/// Draws the whole UI and records the hit areas for the mouse handler.
pub fn render(frame: &mut Frame, app: &mut App) {
    let layout = screen_layout(frame.area());
    record_hit_areas(app, &layout);

    render_domain_panel(frame, app, layout.domain);
    render_problem_panel(frame, app, layout.problem);
    render_solver_config(frame, app, layout.config, &layout.config_rows);
    render_stats_panel(frame, app, layout.stats);
    render_plan_panel(frame, app, layout.plan);
    render_command_bar(frame, app, layout.commands);

    if let Some(msg) = &app.flash_error {
        render_flash(frame, msg);
    }

    if app.modal.is_some() {
        render_modal(frame, app);
    } else {
        app.areas.modal = None;
    }
}

/// Splits the terminal `area` into the panels of [`ScreenLayout`].
fn screen_layout(area: Rect) -> ScreenLayout {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    let mid_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(main_chunks[1]);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(1)])
        .split(mid_chunks[1]);

    ScreenLayout {
        domain: top_chunks[0],
        problem: top_chunks[1],
        config: mid_chunks[0],
        config_rows: config_rows(mid_chunks[0]),
        stats: right_chunks[0],
        plan: right_chunks[1],
        commands: main_chunks[2],
    }
}

/// Splits the inside of the settings panel `outer` into its four rows.
fn config_rows(outer: Rect) -> Rc<[Rect]> {
    let inner = Rect {
        x: outer.x + 1,
        y: outer.y + 1,
        width: outer.width.saturating_sub(2),
        height: outer.height.saturating_sub(2),
    };

    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner)
}

/// Stores the panel areas of this frame in [`App::areas`].
fn record_hit_areas(app: &mut App, layout: &ScreenLayout) {
    app.areas.domain = Some(layout.domain);
    app.areas.problem = Some(layout.problem);
    app.areas.config = Some(layout.config);
    app.areas.plan = Some(layout.plan);
    app.areas.config_planner_row = layout.config_rows[0].y;
    app.areas.config_heuristic_row = layout.config_rows[1].y;
}

/// Draws the flash message, up to 40 cells wide, centered horizontally at mid height.
fn render_flash(frame: &mut Frame, msg: &str) {
    let flash = Paragraph::new(msg.to_owned()).style(bold(Color::Yellow));
    let area = frame.area();
    let width = area.width.min(40);
    let flash_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height / 2,
        width,
        height: area.height.min(1),
    };
    frame.render_widget(flash, flash_area);
}

/// Foreground `color` in bold.
fn bold(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

/// Dimmed dark gray, used for placeholder text.
fn dimmed() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM)
}

/// A bordered panel titled `title` whose border is cyan while focused.
fn panel(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        })
}

/// Highlight of the selected list row: bold on dark gray when focused, a faint bar otherwise.
fn list_highlight(focused: bool) -> Style {
    if focused {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(Color::Rgb(50, 50, 50))
    }
}

/// Draws a vertical scrollbar on the right edge of `area` for `len` items at `selected`.
fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    state: ScrollbarState,
    selected: usize,
    len: usize,
) {
    let mut scroll_state = state.position(selected).content_length(len);
    let scrollbar = Scrollbar::default()
        .orientation(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("↑"))
        .end_symbol(Some("↓"));
    frame.render_stateful_widget(scrollbar, area, &mut scroll_state);
}

/// Draws the domain list with problem counts; domains without problems are dimmed.
fn render_domain_panel(frame: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = matches!(app.focus, AppFocus::DomainList);
    let block = panel(" Domain ", is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = app
        .domain_state
        .items
        .iter()
        .map(|&bundle_idx| domain_item(app, bundle_idx))
        .collect();

    let list = List::new(items).highlight_style(list_highlight(is_focused));
    frame.render_stateful_widget(list, inner, &mut app.domain_state.state);

    let selected = app.domain_state.state.selected().unwrap_or(0);
    render_scrollbar(
        frame,
        area,
        app.domain_scroll_state,
        selected,
        app.domain_state.items.len(),
    );
}

/// The list row for the domain at `bundle_idx`: name (`*` if duplicated) and problem count.
fn domain_item(app: &App, bundle_idx: usize) -> ListItem<'static> {
    let (_, domain) = &app.bundle.domains[bundle_idx];
    let name = domain.name().to_string();
    let count = app.problem_count_for_domain(&name);
    let display_name = if app.duplicated_domains.contains(&name) {
        format!("{name}*")
    } else {
        name
    };

    let (label, style) = if count > 0 {
        let plural = if count == 1 { "" } else { "s" };
        (
            format!("{display_name} ({count} problem{plural})"),
            Style::default().fg(Color::White),
        )
    } else {
        (format!("{display_name} (0 problems)"), dimmed())
    };

    ListItem::new(Span::styled(label, style))
}

/// Draws the problems of the selected domain, or a notice when there are none.
fn render_problem_panel(frame: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = matches!(app.focus, AppFocus::ProblemList);
    let blocked = app.problem_blocked;
    let block = problem_block(is_focused, blocked);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if blocked {
        let msg = Paragraph::new("No problems match the selected domain.").style(dimmed());
        frame.render_widget(msg, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .problem_state
        .items
        .iter()
        .map(|&prob_idx| {
            let (_, problem) = &app.bundle.problems[prob_idx];
            ListItem::new(Span::styled(
                problem.name().to_string(),
                Style::default().fg(Color::White),
            ))
        })
        .collect();

    let list = List::new(items).highlight_style(list_highlight(is_focused));
    frame.render_stateful_widget(list, inner, &mut app.problem_state.state);

    let selected = app.problem_state.state.selected().unwrap_or(0);
    render_scrollbar(
        frame,
        area,
        app.problem_scroll_state,
        selected,
        app.problem_state.items.len(),
    );
}

/// The problem panel frame: cyan when focused, red and titled "(blocked)" without problems.
fn problem_block(is_focused: bool, blocked: bool) -> Block<'static> {
    let title = if blocked {
        " Problem (blocked) "
    } else {
        " Problem "
    };
    let border = if is_focused {
        Style::default().fg(Color::Cyan)
    } else if blocked {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };

    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border)
}

/// Draws the settings panel: planner, heuristic, input fields and the solve button.
///
/// `chunks` are the four rows from [`config_rows`].
fn render_solver_config(frame: &mut Frame, app: &mut App, outer: Rect, chunks: &[Rect]) {
    let is_config_focused = matches!(app.focus, AppFocus::InputField(_) | AppFocus::SolveButton);
    frame.render_widget(panel(" Solver Settings ", is_config_focused), outer);

    let selector_fg = if matches!(app.focus, AppFocus::InputField(_)) {
        Color::Cyan
    } else {
        Color::White
    };

    let planner = &app.planners[app.selected_planner];
    let planner_label = format!(
        "Planner: {}  ({})  [click/Enter]",
        planner.name, planner.description
    );
    frame.render_widget(selector_line(planner_label, selector_fg), chunks[0]);

    if app.heuristic_enabled {
        let heuristic = &app.heuristics[app.selected_heuristic];
        let heuristic_label = format!(
            "Heuristic: {}  ({})  [click/Enter]",
            heuristic.name, heuristic.description
        );
        frame.render_widget(selector_line(heuristic_label, selector_fg), chunks[1]);
    }

    let lines = vec![
        input_line(app, InputField::Timeout, "Timeout", &app.timeout_input),
        input_line(app, InputField::MaxNodes, "Max Nodes", &app.max_nodes_input),
        input_line(app, InputField::OutputPath, "Output", &app.output_path),
    ];
    frame.render_widget(Paragraph::new(lines), chunks[2]);

    frame.render_widget(solve_button(app), chunks[3]);
    app.areas.solve_button = Some(chunks[3]);
}

/// A one-line planner or heuristic selector in color `fg`.
fn selector_line(label: String, fg: Color) -> Paragraph<'static> {
    Paragraph::new(Span::styled(label, Style::default().fg(fg)))
}

/// One input field row: `label` with an edit/focus hint, then `value` (with a cursor when
/// editing).
fn input_line(app: &App, field: InputField, label: &str, value: &str) -> Line<'static> {
    let editing = app.editing_input == Some(field);
    let focused = matches!(app.focus, AppFocus::InputField(f) if f == field);
    let hint = if editing {
        " (editing)"
    } else if focused {
        " [Enter]"
    } else {
        ""
    };

    let label_style = if focused {
        bold(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };
    let (text, value_fg) = if editing {
        (format!("{value}█"), Color::Yellow)
    } else {
        (value.to_owned(), Color::Gray)
    };

    Line::from(vec![
        Span::styled(format!("{label}:{hint} "), label_style),
        Span::styled(text, Style::default().fg(value_fg)),
    ])
}

/// The solve button: green when solving is possible, dimmed while blocked or running.
fn solve_button(app: &App) -> Paragraph<'static> {
    let can_solve = app.can_solve();
    let is_solving = app.solving_state == SolvingState::Running;
    let solve_text = if is_solving {
        "Solving..."
    } else if can_solve {
        "[ Solve ]"
    } else {
        "[ Solve (blocked) ]"
    };
    let solve_style = if can_solve && !is_solving {
        bold(Color::Green)
    } else {
        dimmed()
    };

    Paragraph::new(Span::styled(solve_text, solve_style))
}

/// Draws the statistics panel for the current [`SolvingState`] and outcome.
fn render_stats_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().title(" Statistics ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = match &app.solving_state {
        SolvingState::Idle => vec![Line::from(Span::styled("Ready to solve", dimmed()))],
        SolvingState::Running => vec![Line::from(Span::styled("Solving...", bold(Color::Cyan)))],
        SolvingState::Completed | SolvingState::Failed => outcome_lines(app.outcome.as_ref()),
    };

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Statistics lines for a finished search, or a placeholder when there is no outcome.
fn outcome_lines(outcome: Option<&Result<SearchOutcome, String>>) -> Vec<Line<'static>> {
    match outcome {
        Some(Ok(SearchOutcome::Plan(_, stats))) => vec![
            Line::from(Span::styled("Plan found!", bold(Color::Green))),
            Line::from(format!(
                "Cost: {:.2}  Length: {}",
                stats.plan_cost, stats.plan_length
            )),
            Line::from(format!(
                "Expanded: {}  Generated: {}",
                stats.nodes_expanded, stats.nodes_generated
            )),
            Line::from(format!("Time: {:?}", stats.elapsed)),
        ],
        Some(Ok(SearchOutcome::Unsolvable(stats))) => vec![
            Line::from(Span::styled("Unsolvable", bold(Color::Yellow))),
            Line::from(format!("Expanded: {}", stats.nodes_expanded)),
            Line::from(format!("Time: {:?}", stats.elapsed)),
        ],
        Some(Ok(SearchOutcome::LimitReached(stats))) => vec![
            Line::from(Span::styled("Limit reached", bold(Color::Yellow))),
            Line::from(format!("Expanded: {}", stats.nodes_expanded)),
            Line::from(format!("Time: {:?}", stats.elapsed)),
        ],
        Some(Err(e)) => vec![
            Line::from(Span::styled("Error", bold(Color::Red))),
            Line::from(e.clone()),
        ],
        _ => vec![Line::from("No stats available")],
    }
}

/// Draws the plan from [`App::plan_scroll_offset`] on, with a scroll hint when it overflows.
fn render_plan_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Plan ", matches!(app.focus, AppFocus::PlanScroll));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = plan_lines(app);
    let visible_start = app.plan_scroll_offset.min(lines.len().saturating_sub(1));
    let visible_lines: Vec<Line> = lines.iter().skip(visible_start).cloned().collect();
    frame.render_widget(Paragraph::new(visible_lines), inner);

    if app.solved_plan.is_some() && lines.len() > usize::from(inner.height) {
        let scroll_hint = Paragraph::new(Span::styled(
            format!(
                "(scroll: ↑/↓ or mouse, {}/{})",
                visible_start + 1,
                lines.len()
            ),
            dimmed(),
        ));
        let hint_rect = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(scroll_hint, hint_rect);
    }
}

/// All lines of the plan panel: a step count header, a blank line and numbered steps, or a
/// status message while there is no plan.
fn plan_lines(app: &App) -> Vec<Line<'static>> {
    let Some(plan) = &app.solved_plan else {
        let msg = match &app.solving_state {
            SolvingState::Running => "Solving...",
            SolvingState::Failed => "Solving failed.",
            _ => "No plan yet — select a problem and press s",
        };
        return vec![Line::from(Span::styled(msg, dimmed()))];
    };

    let mut items = vec![
        Line::from(Span::styled(
            format!("{} steps", plan.steps.len()),
            bold(Color::Green),
        )),
        Line::from(""),
    ];
    for (i, step) in plan.steps.iter().enumerate() {
        items.push(Line::from(format!("  {}. {}", i + 1, step.op_name)));
    }

    items
}

/// Draws the key hints that apply in the current state.
fn render_command_bar(frame: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Commands ", matches!(app.focus, AppFocus::CommandBar));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut commands = Vec::new();
    if app.solving_state == SolvingState::Running {
        commands.push(Span::styled(
            "[Esc] Cancel ",
            Style::default().fg(Color::Yellow),
        ));
    } else {
        commands.push(Span::styled("[s] Solve ", bold(Color::Green)));
    }

    if app.solved_plan.is_some() {
        commands.push(Span::styled(
            "[w] Write Plan ",
            Style::default().fg(Color::Cyan),
        ));
    }

    if app.outcome.is_some() || app.solved_plan.is_some() {
        let white = Style::default().fg(Color::White);
        commands.push(Span::styled("[c] Clear ", white));
        commands.push(Span::styled("[r] Restart ", white));
    }

    commands.push(Span::styled("[q] Quit", Style::default().fg(Color::Red)));
    frame.render_widget(Paragraph::new(Line::from(commands)), inner);
}

/// Draws the open planner or heuristic modal centered on screen and records its area.
fn render_modal(frame: &mut Frame, app: &mut App) {
    let Some((title, items)) = modal_items(app) else {
        return;
    };

    let modal_area = modal_rect(frame.area(), items.len());
    app.areas.modal = Some(modal_area);
    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(&block, modal_area);

    let inner = block.inner(modal_area);
    let list = List::new(items).highlight_style(list_highlight(true));
    let mut state: ListState = app.modal_list_state;
    frame.render_stateful_widget(list, inner, &mut state);

    let selected = app.modal_list_state.selected().unwrap_or(0);
    let modal_len = match app.modal {
        Some(ModalKind::Planner) => app.planners.len(),
        Some(ModalKind::Heuristic) => app.heuristics.len(),
        None => 0,
    };
    render_scrollbar(
        frame,
        modal_area,
        app.modal_scroll_state,
        selected,
        modal_len,
    );
}

/// Title and entries of the open modal; `None` when no modal is open.
fn modal_items(app: &App) -> Option<(&'static str, Vec<ListItem<'static>>)> {
    let (title, entries): (_, Vec<_>) = match app.modal? {
        ModalKind::Planner => (
            "Select Planner",
            app.planners
                .iter()
                .map(|p| (&p.name, &p.description))
                .collect(),
        ),
        ModalKind::Heuristic => (
            "Select Heuristic",
            app.heuristics
                .iter()
                .map(|h| (&h.name, &h.description))
                .collect(),
        ),
    };

    let items = entries
        .into_iter()
        .map(|(name, description)| ListItem::new(format!("{name} — {description}")))
        .collect();

    Some((title, items))
}

/// A modal at most 60 cells wide and one row per entry plus borders (at least 5 rows),
/// centered in `area` and kept 2 cells away from its edges where possible.
fn modal_rect(area: Rect, entries: usize) -> Rect {
    let modal_width = 60.min(area.width.saturating_sub(4));
    let entry_rows = u16::try_from(entries).unwrap_or(u16::MAX);
    let modal_height = entry_rows
        .saturating_add(2)
        .min(area.height.saturating_sub(4))
        .max(5);

    Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    }
}

#[cfg(test)]
mod tests {
    use miniplan::search::SearchStats;

    use super::*;
    use crate::tui::events::AppEvent;
    use crate::tui::test_support::{bundle, draw, gripper_app, wait_for_solve};

    /// Asserts that `screen` contains every string of `needles`, printing the screen if not.
    fn assert_shows(screen: &str, needles: &[&str]) {
        for needle in needles {
            assert!(screen.contains(needle), "missing {needle:?} in:\n{screen}");
        }
    }

    /// The text of the problem panel's inside rows in `screen`, as recorded in `app`.
    fn problem_panel_text(app: &App, screen: &str) -> String {
        let rect = app.areas.problem.expect("problem area recorded");
        screen
            .lines()
            .skip(usize::from(rect.y))
            .take(usize::from(rect.height))
            .map(|line| {
                line.chars()
                    .skip(usize::from(rect.x))
                    .take(usize::from(rect.width))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The first frame shows every panel title, the selection and the default settings.
    #[test]
    fn initial_frame_shows_panels_and_defaults() {
        let mut app = gripper_app();

        let screen = draw(&mut app, 120, 40);

        assert_shows(
            &screen,
            &[
                " Domain ",
                "gripper (1 problem)",
                " Problem ",
                "gripper-2",
                " Solver Settings ",
                "Planner: astar",
                "Heuristic: hff",
                "Timeout: 300s",
                "Max Nodes:",
                "Output: -",
                "[ Solve ]",
                " Statistics ",
                "Ready to solve",
                " Plan ",
                "No plan yet",
                " Commands ",
                "[s] Solve",
                "[q] Quit",
            ],
        );
        assert!(!screen.contains("[w] Write Plan"));
    }

    /// A small terminal still renders the panels without panicking.
    #[test]
    fn small_frame_renders() {
        let mut app = gripper_app();

        let screen = draw(&mut app, 60, 20);

        assert_shows(&screen, &[" Domain ", "gripper", " Commands ", "[s] Solve"]);
    }

    /// Rendering records every panel area, and the selector rows sit inside the settings panel.
    #[test]
    fn render_records_hit_areas() {
        let mut app = gripper_app();

        draw(&mut app, 120, 40);

        let areas = &app.areas;
        let domain = areas.domain.expect("domain area");
        let problem = areas.problem.expect("problem area");
        let config = areas.config.expect("config area");
        let plan = areas.plan.expect("plan area");
        let button = areas.solve_button.expect("solve button area");
        assert_eq!((domain.y, domain.height), (0, 8));
        assert_eq!(problem.x, domain.x + domain.width);
        assert_eq!(areas.config_planner_row, config.y + 1);
        assert_eq!(areas.config_heuristic_row, config.y + 2);
        assert!(config.contains(button.as_position()));
        assert!(plan.x >= config.x + config.width);
        assert_eq!(areas.modal, None);
    }

    /// The problem panel lists only the problems of the selected domain.
    #[test]
    fn problem_panel_follows_domain_selection() {
        let mut app = App::new(bundle(&["gripper-small.pddl", "air-cargo.pddl"]));

        let screen = draw(&mut app, 120, 40);
        let panel = problem_panel_text(&app, &screen);
        assert!(panel.contains("gripper-2") && !panel.contains("air-cargo-p1"));
        assert_shows(&screen, &["air-cargo (3 problems)"]);

        app.on_key(AppEvent::Down);
        let screen = draw(&mut app, 120, 40);
        let panel = problem_panel_text(&app, &screen);
        assert!(panel.contains("air-cargo-p3") && !panel.contains("gripper-2"));
    }

    /// A domain without problems is dimmed, the problem panel is blocked and so is solving.
    #[test]
    fn blocked_frame() {
        let mut bundle = bundle(&["gripper-small.pddl"]);
        bundle.problems.clear();
        let mut app = App::new(bundle);

        let screen = draw(&mut app, 120, 40);

        assert_shows(
            &screen,
            &[
                "gripper (0 problems)",
                " Problem (blocked) ",
                "No problems match the selected domain.",
                "[ Solve (blocked) ]",
            ],
        );
    }

    /// A domain defined twice is shown once with a `*`.
    #[test]
    fn duplicated_domain_is_starred() {
        let mut app = App::new(bundle(&["gripper-small.pddl", "gripper.pddl"]));

        let screen = draw(&mut app, 120, 40);

        assert_shows(
            &screen,
            &["gripper* (2 problems)", "gripper-2", "gripper-4"],
        );
    }

    /// The planner modal lists every planner with its description and records its area.
    #[test]
    fn planner_modal_frame() {
        let mut app = gripper_app();
        app.modal = Some(ModalKind::Planner);

        let screen = draw(&mut app, 120, 40);

        assert_shows(
            &screen,
            &[" Select Planner ", "bfs — ", "astar — ", "gbfs — "],
        );
        let modal = app.areas.modal.expect("modal area recorded");
        assert_eq!(usize::from(modal.height), app.planners.len() + 2);
    }

    /// The heuristic modal lists the heuristics and fits a small terminal.
    #[test]
    fn heuristic_modal_frame() {
        let mut app = gripper_app();
        app.modal = Some(ModalKind::Heuristic);

        let screen = draw(&mut app, 60, 20);

        assert_shows(&screen, &[" Select Heuristic ", "hff — ", "blind — "]);
        let modal = app.areas.modal.expect("modal area recorded");
        assert!(modal.width <= 56 && modal.height <= 16);
    }

    /// A blind planner hides the heuristic row.
    #[test]
    fn blind_planner_hides_heuristic() {
        let mut app = gripper_app();
        app.selected_planner = app
            .planners
            .iter()
            .position(|p| p.name == "bfs")
            .expect("bfs is registered");
        app.heuristic_enabled = false;

        let screen = draw(&mut app, 120, 40);

        assert_shows(&screen, &["Planner: bfs"]);
        assert!(!screen.contains("Heuristic:"));
    }

    /// The edited field shows the edit marker and a cursor; a focused field shows its hint.
    #[test]
    fn input_field_markers() {
        let mut app = gripper_app();
        app.focus = AppFocus::InputField(InputField::Timeout);

        let screen = draw(&mut app, 120, 40);
        assert_shows(&screen, &["Timeout: [Enter] 300s"]);

        app.editing_input = Some(InputField::Timeout);
        let screen = draw(&mut app, 120, 40);
        assert_shows(&screen, &["Timeout: (editing) 300s█"]);
    }

    /// While a search runs, the statistics, plan, button and command bar say so.
    #[test]
    fn solving_frame() {
        let mut app = gripper_app();
        app.on_key(AppEvent::Char('s'));

        let screen = draw(&mut app, 120, 40);

        assert_shows(&screen, &["Solving...", "[Esc] Cancel"]);
        assert!(!screen.contains("[s] Solve"));
        wait_for_solve(&mut app);
    }

    /// After a successful search the statistics, numbered steps and write hint are shown.
    #[test]
    fn solved_frame_shows_plan() {
        let mut app = gripper_app();
        app.on_key(AppEvent::Char('s'));
        wait_for_solve(&mut app);

        let screen = draw(&mut app, 120, 40);

        assert_shows(
            &screen,
            &[
                "Plan found!",
                "Cost: 7.00  Length: 7",
                "Expanded: ",
                "7 steps",
                "[w] Write Plan",
                "[c] Clear",
                "[r] Restart",
            ],
        );
        let plan = app.solved_plan.as_ref().expect("plan kept");
        for (i, step) in plan.steps.iter().enumerate() {
            assert_shows(&screen, &[&format!("  {}. {}", i + 1, step.op_name)]);
        }
    }

    /// A plan longer than the panel gets a scroll hint, and scrolling moves its position.
    #[test]
    fn long_plan_shows_scroll_hint() {
        let mut app = gripper_app();
        app.on_key(AppEvent::Char('s'));
        wait_for_solve(&mut app);

        let screen = draw(&mut app, 120, 24);
        assert_shows(&screen, &["(scroll: ↑/↓ or mouse, 1/9)"]);

        app.plan_scroll_offset = 2;
        let screen = draw(&mut app, 120, 24);
        assert_shows(&screen, &["(scroll: ↑/↓ or mouse, 3/9)"]);
        draw(&mut app, 60, 20);
    }

    /// A failed search shows the error in the statistics and a failure note in the plan.
    #[test]
    fn failed_frame_shows_error() {
        let mut app = gripper_app();
        app.solving_state = SolvingState::Failed;
        app.outcome = Some(Err("cancelled".to_owned()));

        let screen = draw(&mut app, 120, 40);

        assert_shows(
            &screen,
            &["Error", "cancelled", "Solving failed.", "[c] Clear"],
        );
    }

    /// Unsolvable and limit outcomes get their own headline; no outcome gets a placeholder.
    #[test]
    fn non_plan_outcomes() {
        let mut app = gripper_app();
        app.solving_state = SolvingState::Completed;
        let cases = [
            (
                Some(Ok(SearchOutcome::Unsolvable(SearchStats::default()))),
                "Unsolvable",
            ),
            (
                Some(Ok(SearchOutcome::LimitReached(SearchStats::default()))),
                "Limit reached",
            ),
            (None, "No stats available"),
        ];

        for (outcome, headline) in cases {
            app.outcome = outcome;

            let screen = draw(&mut app, 120, 40);

            assert_shows(&screen, &[headline]);
        }
    }

    /// The flash message is drawn over the middle of the screen.
    #[test]
    fn flash_message_is_drawn() {
        let mut app = gripper_app();
        app.set_flash_error("Invalid timeout: bad".to_owned());

        let screen = draw(&mut app, 120, 40);

        let middle = screen.lines().nth(20).expect("row 20 exists");
        assert!(middle.contains("Invalid timeout: bad"), "{middle}");
    }

    /// A terminal narrower than the flash popup still draws it, truncated.
    #[test]
    fn flash_message_fits_narrow_terminal() {
        let mut app = gripper_app();
        app.set_flash_error("Invalid timeout: bad".to_owned());

        let screen = draw(&mut app, 30, 10);

        let middle = screen.lines().nth(5).expect("row 5 exists");
        assert!(middle.contains("Invalid timeout"), "{middle}");
    }
}
