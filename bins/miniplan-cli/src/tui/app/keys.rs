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
    use miniplan::plan::{Plan, PlanStep};
    use miniplan::search::{SearchOutcome, SearchStats};
    use miniplan::task::OpId;

    use super::super::tests::select_planner;
    use super::super::{ModalKind, SolvingState};
    use super::*;
    use crate::tui::test_support::{bundle, gripper_app, wait_for_solve};

    /// Sends every event of `events` to `app` in order.
    fn press_all(app: &mut App, events: &[AppEvent]) {
        for &event in events {
            app.on_key(event);
        }
    }

    /// A plan of `len` dummy steps, enough to make the plan panel scrollable.
    fn plan_of(len: usize) -> Plan {
        Plan {
            steps: (0..len)
                .map(|i| PlanStep {
                    op_id: OpId(i),
                    op_name: format!("step-{i}"),
                })
                .collect(),
            cost: 0.0,
            partial_order: None,
        }
    }

    /// Tab visits every widget in the documented order and wraps back to the domain list.
    #[test]
    fn tab_cycles_focus_forward() {
        let mut app = gripper_app();
        let expected = [
            AppFocus::ProblemList,
            AppFocus::InputField(InputField::Timeout),
            AppFocus::InputField(InputField::MaxNodes),
            AppFocus::InputField(InputField::OutputPath),
            AppFocus::SolveButton,
            AppFocus::CommandBar,
            AppFocus::DomainList,
        ];

        for focus in expected {
            app.on_key(AppEvent::Tab);

            assert_eq!(app.focus, focus);
        }
    }

    /// Shift+Tab walks back from the command bar through the plan panel to the domain list.
    #[test]
    fn back_tab_cycles_focus_backward() {
        let mut app = gripper_app();
        app.focus = AppFocus::CommandBar;
        let expected = [
            AppFocus::PlanScroll,
            AppFocus::InputField(InputField::OutputPath),
            AppFocus::InputField(InputField::MaxNodes),
            AppFocus::InputField(InputField::Timeout),
            AppFocus::ProblemList,
            AppFocus::DomainList,
        ];

        for focus in expected {
            app.on_key(AppEvent::BackTab);

            assert_eq!(app.focus, focus);
        }

        app.focus = AppFocus::SolveButton;
        app.on_key(AppEvent::BackTab);
        assert_eq!(app.focus, AppFocus::InputField(InputField::OutputPath));
        app.focus = AppFocus::PlanScroll;
        app.on_key(AppEvent::Tab);
        assert_eq!(app.focus, AppFocus::CommandBar);
    }

    /// Moving through the domain list refills the problem list with that domain's problems.
    #[test]
    fn domain_selection_rebuilds_problems() {
        let mut app = App::new(bundle(&["gripper-small.pddl", "air-cargo.pddl"]));

        app.on_key(AppEvent::Down);
        assert_eq!(app.problem_state.items, [1, 2, 3]);

        app.on_key(AppEvent::Char('k'));
        assert_eq!(app.problem_state.items, [0]);

        app.on_key(AppEvent::Char('k'));
        assert_eq!(app.problem_state.items, [1, 2, 3], "selection wraps");

        app.on_key(AppEvent::Char('j'));
        assert_eq!(app.problem_state.items, [0]);
    }

    /// Tab cannot leave the domain list while no problem matches the selected domain.
    #[test]
    fn tab_from_blocked_domain_stays() {
        let mut bundle = bundle(&["gripper-small.pddl"]);
        bundle.problems.clear();
        let mut app = App::new(bundle);

        app.on_key(AppEvent::Tab);

        assert_eq!(app.focus, AppFocus::DomainList);
    }

    /// The problem list moves its selection with arrows and `j`/`k`, wrapping at the ends.
    #[test]
    fn problem_selection_moves() {
        let mut app = App::new(bundle(&["air-cargo.pddl"]));
        app.focus = AppFocus::ProblemList;

        app.on_key(AppEvent::Down);
        assert_eq!(app.problem_state.selected(), Some(&1));
        app.on_key(AppEvent::Char('j'));
        assert_eq!(app.problem_state.selected(), Some(&2));
        app.on_key(AppEvent::Char('j'));
        assert_eq!(app.problem_state.selected(), Some(&0));
        app.on_key(AppEvent::Up);
        assert_eq!(app.problem_state.selected(), Some(&2));
        app.on_key(AppEvent::Char('k'));
        assert_eq!(app.problem_state.selected(), Some(&1));
    }

    /// Enter starts editing, Delete erases, typing appends and Enter again stops editing.
    #[test]
    fn timeout_field_edits() {
        let mut app = gripper_app();
        app.focus = AppFocus::InputField(InputField::Timeout);

        app.on_key(AppEvent::Delete);
        assert_eq!(
            app.timeout_input, "300s",
            "Delete outside edit mode does nothing"
        );

        app.on_key(AppEvent::Enter);
        assert_eq!(app.editing_input, Some(InputField::Timeout));
        press_all(&mut app, &[AppEvent::Delete; 5]);
        press_all(&mut app, &[AppEvent::Char('5'), AppEvent::Char('s')]);
        app.on_key(AppEvent::Enter);

        assert_eq!(app.timeout_input, "5s");
        assert_eq!(app.editing_input, None);
        assert_eq!(app.solving_state, SolvingState::Idle, "typed `s` is text");
    }

    /// Typing into a field that is not in edit mode enters edit mode with that character.
    #[test]
    fn typing_starts_editing_max_nodes() {
        let mut app = gripper_app();
        app.focus = AppFocus::InputField(InputField::MaxNodes);

        press_all(&mut app, &[AppEvent::Char('4'), AppEvent::Char('2')]);

        assert_eq!(app.editing_input, Some(InputField::MaxNodes));
        assert_eq!(app.max_nodes_input, "42");
    }

    /// While editing, `q` is text; outside edit mode it asks the loop to quit.
    #[test]
    fn q_in_field_types_or_quits() {
        let mut app = gripper_app();
        app.focus = AppFocus::InputField(InputField::OutputPath);
        app.editing_input = Some(InputField::OutputPath);

        app.on_key(AppEvent::Char('q'));
        assert_eq!(app.output_path, "-q");
        assert!(app.outcome.is_none());

        app.editing_input = None;
        app.on_key(AppEvent::Char('q'));
        assert!(matches!(app.outcome, Some(Err(ref e)) if e == "quit"));
    }

    /// `s` in a field outside edit mode starts a search instead of typing.
    #[test]
    fn s_in_field_solves() {
        let mut app = gripper_app();
        app.focus = AppFocus::InputField(InputField::Timeout);

        app.on_key(AppEvent::Char('s'));

        assert_eq!(app.solving_state, SolvingState::Running);
        assert_eq!(app.timeout_input, "300s");
        wait_for_solve(&mut app);
    }

    /// `q` on a list asks the loop to quit.
    #[test]
    fn q_on_list_quits() {
        let mut app = gripper_app();

        app.on_key(AppEvent::Char('q'));

        assert!(matches!(app.outcome, Some(Err(ref e)) if e == "quit"));
    }

    /// Enter on the solve button starts a search that completes with a plan.
    #[test]
    fn enter_on_solve_button_solves() {
        let mut app = gripper_app();
        app.focus = AppFocus::SolveButton;

        app.on_key(AppEvent::Enter);
        wait_for_solve(&mut app);

        assert_eq!(app.solving_state, SolvingState::Completed);
        assert!(app.solved_plan.is_some());
    }

    /// The modal selection wraps with arrows and `j`/`k`; Enter applies it and closes.
    #[test]
    fn planner_modal_navigates_and_applies() {
        let mut app = gripper_app();
        let start = app.selected_planner;
        let len = app.planners.len();
        app.open_planner_modal();

        app.on_key(AppEvent::Down);
        app.on_key(AppEvent::Char('j'));
        app.on_key(AppEvent::Char('k'));
        assert_eq!(app.modal_list_state.selected(), Some((start + 1) % len));
        app.on_key(AppEvent::Tab);
        assert_eq!(
            app.modal,
            Some(ModalKind::Planner),
            "other keys are swallowed"
        );
        app.on_key(AppEvent::Enter);

        assert_eq!(app.modal, None);
        assert_eq!(app.selected_planner, (start + 1) % len);
    }

    /// Up from the first modal entry wraps to the last; selecting a blind planner disables
    /// the heuristic.
    #[test]
    fn planner_modal_wraps_to_blind_planner() {
        let mut app = gripper_app();
        select_planner(&mut app, "bfs");
        let bfs = app.selected_planner;
        app.selected_planner = 0;
        app.open_planner_modal();
        let len = app.planners.len();

        app.on_key(AppEvent::Up);
        assert_eq!(app.modal_list_state.selected(), Some(len - 1));
        app.modal_list_state.select(Some(bfs));
        app.on_key(AppEvent::Enter);

        assert_eq!(app.planners[app.selected_planner].name, "bfs");
        assert!(!app.heuristic_enabled);
    }

    /// Escape closes the heuristic modal without applying its selection.
    #[test]
    fn heuristic_modal_escape_discards() {
        let mut app = gripper_app();
        app.open_heuristic_modal();

        app.on_key(AppEvent::Down);
        app.on_key(AppEvent::Escape);

        assert_eq!(app.modal, None);
        assert_eq!(app.selected_heuristic, 0);
    }

    /// Enter in the heuristic modal applies the selected heuristic.
    #[test]
    fn heuristic_modal_applies() {
        let mut app = gripper_app();
        app.open_heuristic_modal();

        press_all(&mut app, &[AppEvent::Down, AppEvent::Down, AppEvent::Enter]);

        assert_eq!(app.heuristics[app.selected_heuristic].name, "hmax");
    }

    /// The plan panel scrolls within its lines and stops at both ends.
    #[test]
    fn plan_scroll_is_bounded() {
        let mut app = gripper_app();
        app.focus = AppFocus::PlanScroll;
        app.solved_plan = Some(plan_of(3));

        app.on_key(AppEvent::Up);
        assert_eq!(app.plan_scroll_offset, 0);
        press_all(&mut app, &[AppEvent::Down; 10]);
        assert_eq!(
            app.plan_scroll_offset, 4,
            "header, blank line and three steps"
        );
        app.on_key(AppEvent::Char('k'));
        assert_eq!(app.plan_scroll_offset, 3);
        app.on_key(AppEvent::Char('j'));
        assert_eq!(app.plan_scroll_offset, 4);
    }

    /// Without a plan the plan panel has one line and cannot scroll.
    #[test]
    fn plan_scroll_without_plan_stays() {
        let mut app = gripper_app();
        app.focus = AppFocus::PlanScroll;

        app.on_key(AppEvent::Down);

        assert_eq!(app.plan_scroll_offset, 0);
    }

    /// `c` on the command bar clears the result but keeps the settings.
    #[test]
    fn command_bar_clears_output() {
        let mut app = gripper_app();
        app.focus = AppFocus::CommandBar;
        app.solved_plan = Some(plan_of(2));
        app.outcome = Some(Ok(SearchOutcome::Unsolvable(SearchStats::default())));
        app.solving_state = SolvingState::Completed;
        app.error = Some("old".to_owned());
        app.plan_scroll_offset = 1;
        app.max_nodes_input = "9".to_owned();

        app.on_key(AppEvent::Char('c'));

        assert!(app.solved_plan.is_none() && app.outcome.is_none() && app.error.is_none());
        assert_eq!(app.solving_state, SolvingState::Idle);
        assert_eq!(app.plan_scroll_offset, 0);
        assert_eq!(app.max_nodes_input, "9");
    }

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
