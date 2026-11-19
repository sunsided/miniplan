//! Keyboard handling: routes an [`AppEvent`] to the open modal or the focused widget.

use super::{App, AppFocus, InputField};
use crate::tui::events::AppEvent;

impl App {
    /// Handles a key event that the event loop did not consume itself.
    ///
    /// An open modal takes every key. Otherwise a text field gets the key first; for the
    /// other widgets `q` quits and `s` starts solving before widget-specific keys apply.
    pub fn on_key(&mut self, event: AppEvent) {
        if self.modal.is_some() {
            self.on_modal_key(event);
            return;
        }

        match (self.focus, event) {
            (AppFocus::InputField(field), _) => self.on_input_field(event, field),
            (_, AppEvent::Char('q')) => self.request_quit(),
            (_, AppEvent::Char('s')) => self.start_solving(),
            (AppFocus::DomainList, _) => self.on_domain_list(event),
            (AppFocus::ProblemList, _) => self.on_problem_list(event),
            (AppFocus::PlanScroll, _) => self.on_plan_scroll(event),
            (AppFocus::CommandBar, _) => self.on_command_bar(event),
            (AppFocus::SolveButton, _) => self.on_solve_button(event),
        }
    }

    /// Moves the modal selection (wrapping), applies it with Enter or closes with Escape.
    fn on_modal_key(&mut self, event: AppEvent) {
        let len = self.modal_len();
        if len == 0 {
            return;
        }

        let selected = self.modal_list_state.selected().unwrap_or(0);
        match event {
            AppEvent::Escape => self.modal = None,
            AppEvent::Enter => self.close_modal(),
            AppEvent::Up | AppEvent::Char('k') => {
                self.modal_list_state
                    .select(Some(super::wrap_previous(selected, len)));
            }
            AppEvent::Down | AppEvent::Char('j') => {
                self.modal_list_state
                    .select(Some(super::wrap_next(selected, len)));
            }
            _ => {}
        }
    }

    /// Domain list: move the selection (refilling the problem list) or Tab to the problems.
    fn on_domain_list(&mut self, event: AppEvent) {
        match event {
            AppEvent::Up | AppEvent::Char('k') => {
                self.domain_state.previous();
                self.rebuild_problem_list();
            }
            AppEvent::Down | AppEvent::Char('j') => {
                self.domain_state.next();
                self.rebuild_problem_list();
            }
            AppEvent::Tab if !self.problem_blocked => self.focus = AppFocus::ProblemList,
            _ => {}
        }
    }

    /// Problem list: move the selection or change focus.
    fn on_problem_list(&mut self, event: AppEvent) {
        match event {
            AppEvent::Up | AppEvent::Char('k') => self.problem_state.previous(),
            AppEvent::Down | AppEvent::Char('j') => self.problem_state.next(),
            AppEvent::BackTab => self.focus = AppFocus::DomainList,
            AppEvent::Tab => self.focus = AppFocus::InputField(InputField::Timeout),
            _ => {}
        }
    }

    /// Text field: Enter toggles editing, Delete removes the last character, Tab moves on.
    fn on_input_field(&mut self, event: AppEvent, field: InputField) {
        let editing = self.editing_input == Some(field);
        match event {
            AppEvent::Enter => self.editing_input = if editing { None } else { Some(field) },
            AppEvent::Tab => self.focus = field.next_focus(),
            AppEvent::BackTab => self.focus = field.previous_focus(),
            AppEvent::Char(c) => self.on_input_char(field, c),
            AppEvent::Delete if editing => {
                self.input_mut(field).pop();
            }
            _ => {}
        }
    }

    /// Types `c` into `field`, entering edit mode first if needed.
    ///
    /// Outside edit mode `q` quits and `s` starts solving instead.
    fn on_input_char(&mut self, field: InputField, c: char) {
        if self.editing_input != Some(field) {
            if c == 'q' {
                self.request_quit();
                return;
            }

            if c == 's' {
                self.start_solving();
                return;
            }
        }

        self.editing_input = Some(field);
        self.input_mut(field).push(c);
    }

    /// The text buffer behind `field`.
    const fn input_mut(&mut self, field: InputField) -> &mut String {
        match field {
            InputField::Timeout => &mut self.timeout_input,
            InputField::MaxNodes => &mut self.max_nodes_input,
            InputField::OutputPath => &mut self.output_path,
        }
    }

    /// Plan panel: scroll within the plan or change focus.
    fn on_plan_scroll(&mut self, event: AppEvent) {
        match event {
            AppEvent::Up | AppEvent::Char('k') if self.plan_scroll_offset > 0 => {
                self.plan_scroll_offset -= 1;
            }
            AppEvent::Down | AppEvent::Char('j')
                if self.plan_scroll_offset + 1 < self.plan_line_count() =>
            {
                self.plan_scroll_offset += 1;
            }
            AppEvent::Tab => self.focus = AppFocus::CommandBar,
            AppEvent::BackTab => self.focus = AppFocus::InputField(InputField::OutputPath),
            _ => {}
        }
    }

    /// Solve button: Enter solves, Tab and Shift+Tab move focus.
    fn on_solve_button(&mut self, event: AppEvent) {
        match event {
            AppEvent::Enter => self.start_solving(),
            AppEvent::Tab => self.focus = AppFocus::CommandBar,
            AppEvent::BackTab => self.focus = AppFocus::InputField(InputField::OutputPath),
            _ => {}
        }
    }

    /// Command bar: `c` clears the result, Tab and Shift+Tab move focus.
    fn on_command_bar(&mut self, event: AppEvent) {
        match event {
            AppEvent::BackTab => self.focus = AppFocus::PlanScroll,
            AppEvent::Tab => self.focus = AppFocus::DomainList,
            AppEvent::Char('c') => self.clear_output(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: Delete removed the last byte and panicked on a multi-byte character.
    #[test]
    fn delete_removes_last_multibyte_char() {
        let mut app = App::new(miniplan::pddl_io::PddlBundle {
            domains: Vec::new(),
            problems: Vec::new(),
        });
        let field = InputField::Timeout;
        app.focus = AppFocus::InputField(field);
        app.editing_input = Some(field);
        *app.input_mut(field) = "5é".to_owned();

        app.on_key(AppEvent::Delete);

        assert_eq!(app.input_mut(field), "5");
    }
}
