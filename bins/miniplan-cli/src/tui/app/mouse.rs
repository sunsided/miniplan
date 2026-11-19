//! Mouse handling: maps clicks and wheel scrolling to widgets via the recorded [`HitAreas`].
//!
//! [`HitAreas`]: super::HitAreas

use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{App, AppFocus, InputField};

/// Returns `rect` if it is set and contains the cell at `col`, `row`.
fn hit(rect: Option<Rect>, col: u16, row: u16) -> Option<Rect> {
    rect.filter(|r| r.contains(Position::new(col, row)))
}

/// Index into a list whose panel has its top border at `outer.y`, for a click on `row`.
///
/// `None` when the click is on or above the top border.
fn list_row_index(row: u16, outer: Rect, scroll_offset: usize) -> Option<usize> {
    let inner_top = outer.y + 1;
    (row >= inner_top).then(|| usize::from(row - inner_top) + scroll_offset)
}

impl App {
    /// Handles a mouse event at terminal cell `col`, `row`; an open modal takes all of them.
    ///
    /// The wheel scrolls the plan when the pointer is over it; a left click focuses or
    /// activates the widget under the pointer.
    pub fn on_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        if self.modal.is_some() {
            self.on_modal_mouse(kind, row);
            return;
        }

        let over_plan = hit(self.areas.plan, col, row).is_some();
        match kind {
            MouseEventKind::ScrollDown
                if over_plan && self.plan_scroll_offset + 1 < self.plan_line_count() =>
            {
                self.plan_scroll_offset += 1;
            }
            MouseEventKind::ScrollUp if over_plan && self.plan_scroll_offset > 0 => {
                self.plan_scroll_offset -= 1;
            }
            MouseEventKind::Down(MouseButton::Left) => self.on_left_click(col, row),
            _ => {}
        }
    }

    /// Activates the first widget under the click, checked from the plan panel downwards.
    fn on_left_click(&mut self, col: u16, row: u16) {
        if hit(self.areas.plan, col, row).is_some() {
            self.focus = AppFocus::PlanScroll;
        } else if hit(self.areas.solve_button, col, row).is_some() {
            self.start_solving();
        } else if let Some(rect) = hit(self.areas.domain, col, row) {
            self.select_domain_at_row(row, rect);
            self.focus = AppFocus::DomainList;
        } else if let Some(rect) = hit(self.areas.problem, col, row) {
            if !self.problem_blocked {
                self.select_problem_at_row(row, rect);
                self.focus = AppFocus::ProblemList;
            }
        } else if hit(self.areas.config, col, row).is_some() {
            self.on_config_click(row);
        }
    }

    /// A click in the settings panel: opens the planner or heuristic modal for their rows,
    /// otherwise focuses the timeout field.
    fn on_config_click(&mut self, row: u16) {
        if row == self.areas.config_planner_row {
            self.open_planner_modal();
        } else if row == self.areas.config_heuristic_row && self.heuristic_enabled {
            self.open_heuristic_modal();
        } else {
            self.focus = AppFocus::InputField(InputField::Timeout);
        }
    }

    /// Selects the domain shown on screen row `row` of the panel `outer`, if there is one.
    fn select_domain_at_row(&mut self, row: u16, outer: Rect) {
        let Some(idx) = list_row_index(row, outer, self.domain_state.state.offset()) else {
            return;
        };

        if idx < self.domain_state.items.len() {
            self.domain_state.state.select(Some(idx));
            self.rebuild_problem_list();
        }
    }

    /// Selects the problem shown on screen row `row` of the panel `outer`, if there is one.
    fn select_problem_at_row(&mut self, row: u16, outer: Rect) {
        if self.problem_blocked {
            return;
        }

        let Some(idx) = list_row_index(row, outer, self.problem_state.state.offset()) else {
            return;
        };

        if idx < self.problem_state.items.len() {
            self.problem_state.state.select(Some(idx));
        }
    }

    /// Mouse input while a modal is open: the wheel moves the selection without wrapping,
    /// a left click on an entry selects and applies it.
    fn on_modal_mouse(&mut self, kind: MouseEventKind, row: u16) {
        let selected = self.modal_list_state.selected().unwrap_or(0);
        match kind {
            MouseEventKind::ScrollDown if selected + 1 < self.modal_len() => {
                self.modal_list_state.select(Some(selected + 1));
            }
            MouseEventKind::ScrollUp if selected > 0 => {
                self.modal_list_state.select(Some(selected - 1));
            }
            MouseEventKind::Down(MouseButton::Left) => self.on_modal_click(row),
            _ => {}
        }
    }

    /// Selects and applies the modal entry on screen row `row`; clicks on the border do nothing.
    fn on_modal_click(&mut self, row: u16) {
        let Some(rect) = self.areas.modal else {
            return;
        };

        if row <= rect.y || row >= rect.y + rect.height - 1 {
            return;
        }

        let idx = usize::from(row - rect.y - 1) + self.modal_list_state.offset();
        if idx < self.modal_len() {
            self.modal_list_state.select(Some(idx));
            self.close_modal();
        }
    }
}

#[cfg(test)]
mod tests {
    use miniplan::plan::{Plan, PlanStep};
    use miniplan::task::OpId;

    use super::super::tests::select_planner;
    use super::super::{ModalKind, SolvingState};
    use super::*;
    use crate::tui::test_support::{bundle, draw, gripper_app, wait_for_solve};

    /// Left mouse button press.
    const LEFT: MouseEventKind = MouseEventKind::Down(MouseButton::Left);

    /// Renders `app` at 120x40 so its hit areas match a real frame.
    fn layout(app: &mut App) {
        draw(app, 120, 40);
    }

    /// The panel area `rect` recorded by the last render.
    fn area(rect: Option<Rect>) -> Rect {
        rect.expect("render recorded the area")
    }

    /// Clicks the cell in the middle of row `row` of the panel `rect`.
    fn click_row(app: &mut App, rect: Rect, row: u16) {
        app.on_mouse(LEFT, rect.x + rect.width / 2, row);
    }

    /// Clicking the second row inside the domain panel selects the second domain.
    #[test]
    fn click_selects_domain() {
        let mut app = App::new(bundle(&["gripper-small.pddl", "air-cargo.pddl"]));
        app.focus = AppFocus::CommandBar;
        layout(&mut app);
        let domain = area(app.areas.domain);

        click_row(&mut app, domain, domain.y + 2);
        assert_eq!(app.domain_state.state.selected(), Some(1));
        assert_eq!(app.problem_state.items, [1, 2, 3]);
        assert_eq!(app.focus, AppFocus::DomainList);

        click_row(&mut app, domain, domain.y);
        assert_eq!(
            app.domain_state.state.selected(),
            Some(1),
            "border click keeps it"
        );
        click_row(&mut app, domain, domain.y + 5);
        assert_eq!(
            app.domain_state.state.selected(),
            Some(1),
            "empty row keeps it"
        );
    }

    /// Clicking a row inside the problem panel selects that problem and focuses the list.
    #[test]
    fn click_selects_problem() {
        let mut app = App::new(bundle(&["air-cargo.pddl"]));
        layout(&mut app);
        let problem = area(app.areas.problem);

        click_row(&mut app, problem, problem.y + 3);
        assert_eq!(app.problem_state.selected(), Some(&2));
        assert_eq!(app.focus, AppFocus::ProblemList);

        click_row(&mut app, problem, problem.y + 6);
        assert_eq!(app.problem_state.selected(), Some(&2), "empty row keeps it");
    }

    /// A click on a blocked problem panel neither selects nor focuses it.
    #[test]
    fn click_on_blocked_problem_panel_is_ignored() {
        let mut bundle = bundle(&["gripper-small.pddl"]);
        bundle.problems.clear();
        let mut app = App::new(bundle);
        layout(&mut app);
        let problem = area(app.areas.problem);

        click_row(&mut app, problem, problem.y + 1);

        assert_eq!(app.focus, AppFocus::DomainList);
    }

    /// Clicking the planner row opens the planner modal; clicking an entry applies it.
    #[test]
    fn click_planner_row_opens_modal_and_entry_applies() {
        let mut app = gripper_app();
        layout(&mut app);
        let config = area(app.areas.config);

        let row = app.areas.config_planner_row;
        click_row(&mut app, config, row);
        assert_eq!(app.modal, Some(ModalKind::Planner));
        layout(&mut app);
        let modal = area(app.areas.modal);

        click_row(&mut app, modal, modal.y);
        assert_eq!(
            app.modal,
            Some(ModalKind::Planner),
            "border click does nothing"
        );
        click_row(&mut app, modal, modal.y + 1);

        assert_eq!(app.modal, None);
        assert_eq!(app.areas.modal, None);
        assert_eq!(app.selected_planner, 0);
    }

    /// Clicking the heuristic row opens the heuristic modal, whose wheel moves the selection
    /// without wrapping.
    #[test]
    fn heuristic_modal_wheel_does_not_wrap() {
        let mut app = gripper_app();
        layout(&mut app);
        let config = area(app.areas.config);
        let len = app.heuristics.len();

        let row = app.areas.config_heuristic_row;
        click_row(&mut app, config, row);
        assert_eq!(app.modal, Some(ModalKind::Heuristic));

        app.on_mouse(MouseEventKind::ScrollUp, 0, 0);
        assert_eq!(app.modal_list_state.selected(), Some(0));
        for _ in 0..len + 2 {
            app.on_mouse(MouseEventKind::ScrollDown, 0, 0);
        }

        assert_eq!(app.modal_list_state.selected(), Some(len - 1));
        app.on_mouse(MouseEventKind::ScrollUp, 0, 0);
        assert_eq!(app.modal_list_state.selected(), Some(len - 2));
    }

    /// A modal click before any render recorded its area is ignored.
    #[test]
    fn modal_click_without_area_is_ignored() {
        let mut app = gripper_app();
        app.open_heuristic_modal();

        app.on_mouse(LEFT, 10, 10);

        assert_eq!(app.modal, Some(ModalKind::Heuristic));
    }

    /// With a blind planner the heuristic row is inert and a click there focuses the timeout.
    #[test]
    fn click_disabled_heuristic_row_focuses_timeout() {
        let mut app = gripper_app();
        select_planner(&mut app, "bfs");
        layout(&mut app);
        let config = area(app.areas.config);

        let row = app.areas.config_heuristic_row;
        click_row(&mut app, config, row);

        assert_eq!(app.modal, None);
        assert_eq!(app.focus, AppFocus::InputField(InputField::Timeout));
    }

    /// Clicking the solve button starts a search.
    #[test]
    fn click_solve_button_solves() {
        let mut app = gripper_app();
        layout(&mut app);
        let button = area(app.areas.solve_button);

        app.on_mouse(LEFT, button.x + 1, button.y);
        assert_eq!(app.solving_state, SolvingState::Running);
        wait_for_solve(&mut app);

        assert!(app.solved_plan.is_some());
    }

    /// The wheel scrolls the plan only while the pointer is over the plan panel, and a click
    /// there focuses it.
    #[test]
    fn wheel_scrolls_plan_under_pointer() {
        let mut app = gripper_app();
        app.solved_plan = Some(Plan {
            steps: vec![PlanStep {
                op_id: OpId(0),
                op_name: "only".to_owned(),
            }],
            cost: 1.0,
        });
        layout(&mut app);
        let plan = area(app.areas.plan);
        let (col, row) = (plan.x + 1, plan.y + 1);

        app.on_mouse(MouseEventKind::ScrollDown, 0, 0);
        assert_eq!(app.plan_scroll_offset, 0, "pointer is not over the plan");
        app.on_mouse(MouseEventKind::ScrollUp, col, row);
        assert_eq!(app.plan_scroll_offset, 0);
        for _ in 0..5 {
            app.on_mouse(MouseEventKind::ScrollDown, col, row);
        }
        assert_eq!(
            app.plan_scroll_offset, 2,
            "stops at the last of three lines"
        );
        app.on_mouse(MouseEventKind::ScrollUp, col, row);
        assert_eq!(app.plan_scroll_offset, 1);

        app.on_mouse(LEFT, col, row);
        assert_eq!(app.focus, AppFocus::PlanScroll);
    }

    /// A click outside every panel and a right click change nothing.
    #[test]
    fn unrelated_clicks_are_ignored() {
        let mut app = gripper_app();
        app.focus = AppFocus::CommandBar;
        layout(&mut app);

        app.on_mouse(LEFT, 1, 39);
        app.on_mouse(MouseEventKind::Down(MouseButton::Right), 1, 1);

        assert_eq!(app.focus, AppFocus::CommandBar);
    }
}
