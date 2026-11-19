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
