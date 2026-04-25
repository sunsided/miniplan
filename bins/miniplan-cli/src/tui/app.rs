use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crossterm::event::{MouseButton, MouseEventKind};
use miniplan::ground::ground;
use miniplan::pddl_io::PddlBundle;
use miniplan::plan::Plan;
use miniplan::search::{PlannerChoice, PlannerConfig, SearchLimits, SearchOutcome, Solver};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::plan_writer::OutputFormat;
use crate::tui::solver::{SolverEvent, spawn_solver};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFocus {
    DomainList,
    ProblemList,
    InputField(InputField),
    PlanScroll,
    CommandBar,
    SolveButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Timeout,
    MaxNodes,
    OutputPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolvingState {
    Idle,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    Planner,
    Heuristic,
}

pub struct PlannerEntry {
    pub name: String,
    pub description: String,
}

pub struct HeuristicEntry {
    pub name: String,
    pub description: String,
}

pub struct StatefulList<T> {
    pub state: ListState,
    pub items: Vec<T>,
}

impl<T> StatefulList<T> {
    pub fn with_items(items: Vec<T>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self { state, items }
    }

    pub fn next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn selected(&self) -> Option<&T> {
        self.state.selected().and_then(|i| self.items.get(i))
    }
}

pub struct App {
    pub bundle: PddlBundle,

    pub focus: AppFocus,
    pub editing_input: Option<InputField>,

    pub domain_state: StatefulList<usize>,
    pub duplicated_domains: HashSet<String>,
    pub compatible_problem_indices: Vec<usize>,
    pub problem_state: StatefulList<usize>,
    pub problem_blocked: bool,

    pub planners: Vec<PlannerEntry>,
    pub selected_planner: usize,
    pub heuristics: Vec<HeuristicEntry>,
    pub selected_heuristic: usize,
    pub timeout_input: String,
    pub max_nodes_input: String,
    pub output_format: OutputFormat,
    pub output_path: String,
    pub heuristic_enabled: bool,

    pub solving_state: SolvingState,
    pub rx: Option<Receiver<SolverEvent>>,
    pub outcome: Option<Result<SearchOutcome, String>>,
    pub solved_plan: Option<Plan>,

    pub plan_scroll_offset: usize,

    pub modal: Option<ModalKind>,
    pub modal_list_state: ListState,

    pub error: Option<String>,
    pub flash_error: Option<String>,
    pub flash_error_time: Option<std::time::Instant>,

    pub domain_area_rect: Option<Rect>,
    pub problem_area_rect: Option<Rect>,
    pub config_area_rect: Option<Rect>,
    pub plan_area_rect: Option<Rect>,
    pub solve_button_rect: Option<Rect>,
    pub config_planner_row: u16,
    pub config_heuristic_row: u16,
    pub modal_area_rect: Option<Rect>,
}

const BLIND_PLANNERS: &[&str] = &["bfs", "bibfs-uc", "bidij"];

impl App {
    pub fn new(bundle: PddlBundle) -> Self {
        let registry = Solver::new().registry;

        let planners: Vec<PlannerEntry> = registry
            .planners()
            .map(|p| PlannerEntry {
                name: p.name.to_string(),
                description: p.description.to_string(),
            })
            .collect();
        let selected_planner = planners.iter().position(|p| p.name == "astar").unwrap_or(0);

        let heuristics: Vec<HeuristicEntry> = vec![
            HeuristicEntry {
                name: "hff".to_string(),
                description: "Relaxed-plan (FF) heuristic".to_string(),
            },
            HeuristicEntry {
                name: "hadd".to_string(),
                description: "Additive heuristic".to_string(),
            },
            HeuristicEntry {
                name: "hmax".to_string(),
                description: "Max heuristic".to_string(),
            },
            HeuristicEntry {
                name: "goal-count".to_string(),
                description: "Count of unsatisfied goals".to_string(),
            },
            HeuristicEntry {
                name: "blind".to_string(),
                description: "Blind heuristic (h=1)".to_string(),
            },
        ];

        let mut seen = HashSet::new();
        let mut duplicated_domains = HashSet::new();
        let mut deduped_indices: Vec<usize> = Vec::new();
        for idx in 0..bundle.domains.len() {
            let (_, domain) = &bundle.domains[idx];
            let name = domain.name().to_string();
            if seen.contains(&name) {
                duplicated_domains.insert(name.clone());
            } else {
                seen.insert(name);
                deduped_indices.push(idx);
            }
        }

        let mut app = Self {
            bundle,
            focus: AppFocus::DomainList,
            editing_input: None,

            domain_state: StatefulList::with_items(deduped_indices),
            duplicated_domains,
            compatible_problem_indices: Vec::new(),
            problem_state: StatefulList::with_items(Vec::new()),
            problem_blocked: true,

            planners,
            selected_planner,
            heuristics,
            selected_heuristic: 0,
            timeout_input: "300s".to_string(),
            max_nodes_input: String::new(),
            output_format: OutputFormat::Plain,
            output_path: "-".to_string(),
            heuristic_enabled: true,

            solving_state: SolvingState::Idle,
            rx: None,
            outcome: None,
            solved_plan: None,

            plan_scroll_offset: 0,

            modal: None,
            modal_list_state: ListState::default(),

            error: None,
            flash_error: None,
            flash_error_time: None,

            domain_area_rect: None,
            problem_area_rect: None,
            config_area_rect: None,
            plan_area_rect: None,
            solve_button_rect: None,
            config_planner_row: 0,
            config_heuristic_row: 0,
            modal_area_rect: None,
        };

        app.rebuild_problem_list();
        app
    }

    pub fn problem_count_for_domain(&self, domain_name: &str) -> usize {
        self.bundle
            .problems
            .iter()
            .filter(|(_, p)| *p.domain() == domain_name)
            .count()
    }

    fn rebuild_problem_list(&mut self) {
        if let Some(&domain_bundle_idx) = self.domain_state.selected() {
            let (_, domain) = &self.bundle.domains[domain_bundle_idx];
            let name = domain.name().to_string();
            let compatible: Vec<usize> = self
                .bundle
                .problems
                .iter()
                .enumerate()
                .filter(|(_, (_, p))| *p.domain() == name)
                .map(|(i, _)| i)
                .collect();
            self.compatible_problem_indices = compatible.clone();
            self.problem_state = StatefulList::with_items(compatible);
            self.problem_blocked = self.compatible_problem_indices.is_empty();
        } else {
            self.compatible_problem_indices.clear();
            self.problem_state = StatefulList::with_items(Vec::new());
            self.problem_blocked = true;
        }
    }

    pub fn set_flash_error(&mut self, msg: String) {
        self.flash_error = Some(msg);
        self.flash_error_time = Some(std::time::Instant::now());
    }

    pub fn tick(&mut self) {
        if let Some(t) = self.flash_error_time
            && t.elapsed() > Duration::from_secs(2)
        {
            self.flash_error = None;
            self.flash_error_time = None;
        }
        if self.solving_state == SolvingState::Running
            && let Some(rx) = &self.rx
            && let Ok(event) = rx.try_recv()
        {
            match event {
                SolverEvent::Done(result) => {
                    self.outcome = Some(result.map_err(|e| e.to_string()));
                    if let Some(Ok(SearchOutcome::Plan(plan, _))) = &self.outcome {
                        self.solved_plan = Some(plan.clone());
                    }
                    self.solving_state = match &self.outcome {
                        Some(Ok(_)) => SolvingState::Completed,
                        _ => SolvingState::Failed,
                    };
                }
            }
        }
    }

    pub fn on_key(&mut self, event: crate::tui::events::AppEvent) {
        if self.modal.is_some() {
            self.on_modal_key(event);
            return;
        }
        match &self.focus {
            AppFocus::DomainList => self.on_domain_list(event),
            AppFocus::ProblemList => self.on_problem_list(event),
            AppFocus::InputField(field) => self.on_input_field(event, *field),
            AppFocus::PlanScroll => self.on_plan_scroll(event),
            AppFocus::CommandBar => self.on_command_bar(event),
            AppFocus::SolveButton => self.on_solve_button(event),
        }
    }

    fn on_modal_key(&mut self, event: crate::tui::events::AppEvent) {
        let len = match self.modal {
            Some(ModalKind::Planner) => self.planners.len(),
            Some(ModalKind::Heuristic) => self.heuristics.len(),
            None => 0,
        };
        if len == 0 {
            return;
        }

        match event {
            crate::tui::events::AppEvent::Escape => {
                self.modal = None;
            }
            crate::tui::events::AppEvent::Enter => {
                self.close_modal();
            }
            crate::tui::events::AppEvent::Up | crate::tui::events::AppEvent::Char('k') => {
                let i = self.modal_list_state.selected().unwrap_or(0);
                let next = if i == 0 { len - 1 } else { i - 1 };
                self.modal_list_state.select(Some(next));
            }
            crate::tui::events::AppEvent::Down | crate::tui::events::AppEvent::Char('j') => {
                let i = self.modal_list_state.selected().unwrap_or(0);
                let next = if i >= len - 1 { 0 } else { i + 1 };
                self.modal_list_state.select(Some(next));
            }
            _ => {}
        }
    }

    fn close_modal(&mut self) {
        if let Some(modal_kind) = self.modal {
            match modal_kind {
                ModalKind::Planner => {
                    if let Some(i) = self.modal_list_state.selected() {
                        self.selected_planner = i.min(self.planners.len() - 1);
                    }
                    self.update_heuristic_enabled();
                }
                ModalKind::Heuristic => {
                    if let Some(i) = self.modal_list_state.selected() {
                        self.selected_heuristic = i.min(self.heuristics.len() - 1);
                    }
                }
            }
        }
        self.modal = None;
        self.modal_area_rect = None;
    }

    fn select_domain_at_row(&mut self, row: u16, outer: Rect) {
        let inner_top = outer.y + 1;
        if row < inner_top {
            return;
        }
        let offset = (row - inner_top) as usize;
        let scroll = self.domain_state.state.offset();
        let idx = offset + scroll;
        if idx < self.domain_state.items.len() {
            self.domain_state.state.select(Some(idx));
            self.rebuild_problem_list();
        }
    }

    fn select_problem_at_row(&mut self, row: u16, outer: Rect) {
        if self.problem_blocked {
            return;
        }
        let inner_top = outer.y + 1;
        if row < inner_top {
            return;
        }
        let offset = (row - inner_top) as usize;
        let scroll = self.problem_state.state.offset();
        let idx = offset + scroll;
        if idx < self.problem_state.items.len() {
            self.problem_state.state.select(Some(idx));
        }
    }

    fn on_modal_mouse(&mut self, kind: MouseEventKind, _col: u16, row: u16) {
        match kind {
            MouseEventKind::ScrollDown => {
                let i = self.modal_list_state.selected().unwrap_or(0);
                let len = match self.modal {
                    Some(ModalKind::Planner) => self.planners.len(),
                    _ => self.heuristics.len(),
                };
                if i + 1 < len {
                    self.modal_list_state.select(Some(i + 1));
                }
            }
            MouseEventKind::ScrollUp => {
                let i = self.modal_list_state.selected().unwrap_or(0);
                if i > 0 {
                    self.modal_list_state.select(Some(i - 1));
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(rect) = self.modal_area_rect
                    && row > rect.y
                    && row < rect.y + rect.height - 1
                {
                    let item_row = (row - rect.y - 1) as usize;
                    let offset = self.modal_list_state.offset();
                    let idx = item_row + offset;
                    let len = match self.modal {
                        Some(ModalKind::Planner) => self.planners.len(),
                        _ => self.heuristics.len(),
                    };
                    if idx < len {
                        self.modal_list_state.select(Some(idx));
                        self.close_modal();
                    }
                }
            }
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        if self.modal.is_some() {
            self.on_modal_mouse(kind, col, row);
            return;
        }

        match kind {
            MouseEventKind::ScrollDown => {
                if let Some(rect) = self.plan_area_rect
                    && col >= rect.x
                    && col < rect.x + rect.width
                    && row >= rect.y
                    && row < rect.y + rect.height
                    && self.plan_scroll_offset + 1 < self.plan_line_count()
                {
                    self.plan_scroll_offset += 1;
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(rect) = self.plan_area_rect
                    && col >= rect.x
                    && col < rect.x + rect.width
                    && row >= rect.y
                    && row < rect.y + rect.height
                    && self.plan_scroll_offset > 0
                {
                    self.plan_scroll_offset -= 1;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(rect) = self.plan_area_rect
                    && col >= rect.x
                    && col < rect.x + rect.width
                    && row >= rect.y
                    && row < rect.y + rect.height
                {
                    self.focus = AppFocus::PlanScroll;
                    return;
                }
                if let Some(rect) = self.solve_button_rect
                    && col >= rect.x
                    && col < rect.x + rect.width
                    && row >= rect.y
                    && row < rect.y + rect.height
                {
                    self.start_solving();
                    return;
                }
                if let Some(rect) = self.domain_area_rect
                    && col >= rect.x
                    && col < rect.x + rect.width
                    && row >= rect.y
                    && row < rect.y + rect.height
                {
                    self.select_domain_at_row(row, rect);
                    self.focus = AppFocus::DomainList;
                    return;
                }
                if let Some(rect) = self.problem_area_rect
                    && col >= rect.x
                    && col < rect.x + rect.width
                    && row >= rect.y
                    && row < rect.y + rect.height
                {
                    if !self.problem_blocked {
                        self.select_problem_at_row(row, rect);
                        self.focus = AppFocus::ProblemList;
                    }
                    return;
                }
                if let Some(rect) = self.config_area_rect
                    && col >= rect.x
                    && col < rect.x + rect.width
                    && row >= rect.y
                    && row < rect.y + rect.height
                {
                    let planner_row = self.config_planner_row;
                    let heuristic_row = self.config_heuristic_row;
                    if row == planner_row {
                        self.open_planner_modal();
                    } else if row == heuristic_row && self.heuristic_enabled {
                        self.open_heuristic_modal();
                    } else {
                        self.focus = AppFocus::InputField(InputField::Timeout);
                    }
                }
            }
            _ => {}
        }
    }

    fn plan_line_count(&self) -> usize {
        self.solved_plan
            .as_ref()
            .map(|p| p.steps.len() + 2)
            .unwrap_or(1)
    }

    fn open_planner_modal(&mut self) {
        self.modal = Some(ModalKind::Planner);
        self.modal_list_state.select(Some(self.selected_planner));
    }

    fn open_heuristic_modal(&mut self) {
        self.modal = Some(ModalKind::Heuristic);
        self.modal_list_state.select(Some(self.selected_heuristic));
    }

    fn on_domain_list(&mut self, event: crate::tui::events::AppEvent) {
        match event {
            crate::tui::events::AppEvent::Up | crate::tui::events::AppEvent::Char('k') => {
                self.domain_state.previous();
                self.rebuild_problem_list();
            }
            crate::tui::events::AppEvent::Down | crate::tui::events::AppEvent::Char('j') => {
                self.domain_state.next();
                self.rebuild_problem_list();
            }
            crate::tui::events::AppEvent::Tab if !self.problem_blocked => {
                self.focus = AppFocus::ProblemList;
            }
            crate::tui::events::AppEvent::Char('q') => {
                self.outcome = Some(Err("quit".to_string()));
            }
            crate::tui::events::AppEvent::Char('s') => {
                self.start_solving();
            }
            _ => {}
        }
    }

    fn on_problem_list(&mut self, event: crate::tui::events::AppEvent) {
        match event {
            crate::tui::events::AppEvent::Up | crate::tui::events::AppEvent::Char('k') => {
                self.problem_state.previous();
            }
            crate::tui::events::AppEvent::Down | crate::tui::events::AppEvent::Char('j') => {
                self.problem_state.next();
            }
            crate::tui::events::AppEvent::BackTab => {
                self.focus = AppFocus::DomainList;
            }
            crate::tui::events::AppEvent::Tab => {
                self.focus = AppFocus::InputField(InputField::Timeout);
            }
            crate::tui::events::AppEvent::Char('q') => {
                self.outcome = Some(Err("quit".to_string()));
            }
            crate::tui::events::AppEvent::Char('s') => {
                self.start_solving();
            }
            _ => {}
        }
    }

    fn on_input_field(&mut self, event: crate::tui::events::AppEvent, field: InputField) {
        match event {
            crate::tui::events::AppEvent::Enter => {
                if self.editing_input == Some(field) {
                    self.editing_input = None;
                } else {
                    self.editing_input = Some(field);
                }
            }
            crate::tui::events::AppEvent::Tab => {
                self.focus = match field {
                    InputField::Timeout => AppFocus::InputField(InputField::MaxNodes),
                    InputField::MaxNodes => AppFocus::InputField(InputField::OutputPath),
                    InputField::OutputPath => AppFocus::SolveButton,
                };
            }
            crate::tui::events::AppEvent::BackTab => {
                self.focus = match field {
                    InputField::Timeout => AppFocus::ProblemList,
                    InputField::MaxNodes => AppFocus::InputField(InputField::Timeout),
                    InputField::OutputPath => AppFocus::InputField(InputField::MaxNodes),
                };
            }
            crate::tui::events::AppEvent::Char(c) => {
                if self.editing_input == Some(field) {
                    match field {
                        InputField::Timeout => self.timeout_input.push(c),
                        InputField::MaxNodes => self.max_nodes_input.push(c),
                        InputField::OutputPath => self.output_path.push(c),
                    }
                } else if c == 'q' {
                    self.outcome = Some(Err("quit".to_string()));
                } else if c == 's' {
                    self.start_solving();
                } else {
                    self.editing_input = Some(field);
                    match field {
                        InputField::Timeout => self.timeout_input.push(c),
                        InputField::MaxNodes => self.max_nodes_input.push(c),
                        InputField::OutputPath => self.output_path.push(c),
                    }
                }
            }
            crate::tui::events::AppEvent::Delete if self.editing_input == Some(field) => {
                match field {
                    InputField::Timeout => {
                        if !self.timeout_input.is_empty() {
                            self.timeout_input.remove(self.timeout_input.len() - 1);
                        }
                    }
                    InputField::MaxNodes => {
                        if !self.max_nodes_input.is_empty() {
                            self.max_nodes_input.remove(self.max_nodes_input.len() - 1);
                        }
                    }
                    InputField::OutputPath => {
                        if !self.output_path.is_empty() {
                            self.output_path.remove(self.output_path.len() - 1);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn on_plan_scroll(&mut self, event: crate::tui::events::AppEvent) {
        match event {
            crate::tui::events::AppEvent::Up | crate::tui::events::AppEvent::Char('k')
                if self.plan_scroll_offset > 0 =>
            {
                self.plan_scroll_offset -= 1;
            }
            crate::tui::events::AppEvent::Down | crate::tui::events::AppEvent::Char('j')
                if self.plan_scroll_offset + 1 < self.plan_line_count() =>
            {
                self.plan_scroll_offset += 1;
            }
            crate::tui::events::AppEvent::Tab => {
                self.focus = AppFocus::CommandBar;
            }
            crate::tui::events::AppEvent::BackTab => {
                self.focus = AppFocus::InputField(InputField::OutputPath);
            }
            crate::tui::events::AppEvent::Char('q') => {
                self.outcome = Some(Err("quit".to_string()));
            }
            crate::tui::events::AppEvent::Char('s') => {
                self.start_solving();
            }
            _ => {}
        }
    }

    fn on_solve_button(&mut self, event: crate::tui::events::AppEvent) {
        match event {
            crate::tui::events::AppEvent::Enter => {
                self.start_solving();
            }
            crate::tui::events::AppEvent::Tab => {
                self.focus = AppFocus::CommandBar;
            }
            crate::tui::events::AppEvent::BackTab => {
                self.focus = AppFocus::InputField(InputField::OutputPath);
            }
            crate::tui::events::AppEvent::Char('q') => {
                self.outcome = Some(Err("quit".to_string()));
            }
            crate::tui::events::AppEvent::Char('s') => {
                self.start_solving();
            }
            _ => {}
        }
    }

    fn on_command_bar(&mut self, event: crate::tui::events::AppEvent) {
        match event {
            crate::tui::events::AppEvent::BackTab => {
                self.focus = AppFocus::PlanScroll;
            }
            crate::tui::events::AppEvent::Tab => {
                self.focus = AppFocus::DomainList;
            }
            crate::tui::events::AppEvent::Char('c') => {
                self.clear_output();
            }
            crate::tui::events::AppEvent::Char('q') => {
                self.outcome = Some(Err("quit".to_string()));
            }
            crate::tui::events::AppEvent::Char('s') => {
                self.start_solving();
            }
            _ => {}
        }
    }

    fn update_heuristic_enabled(&mut self) {
        let planner_name = &self.planners[self.selected_planner].name;
        self.heuristic_enabled = !BLIND_PLANNERS.contains(&planner_name.as_str());
    }

    fn start_solving(&mut self) {
        if self.problem_blocked || self.problem_state.selected().is_none() {
            self.set_flash_error("No problem selected or domain has no problems".to_string());
            return;
        }

        let prob_idx = *self.problem_state.selected().unwrap();
        let domain_bundle_idx = *self.domain_state.selected().unwrap();
        let (_, domain) = &self.bundle.domains[domain_bundle_idx];
        let (_, problem) = &self.bundle.problems[prob_idx];

        let task = match ground(domain, problem) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("Grounding failed: {}", e));
                return;
            }
        };

        let planner_name = self.planners[self.selected_planner].name.clone();
        let heuristic_name = self.heuristics[self.selected_heuristic].name.clone();

        let mut config = PlannerConfig::default();
        if self.heuristic_enabled {
            config
                .opts
                .insert("heuristic".to_owned(), heuristic_name.clone());
        }

        let choice = PlannerChoice {
            planner: planner_name.clone(),
            heuristic: if self.heuristic_enabled {
                Some(heuristic_name)
            } else {
                None
            },
            config,
        };

        let limits = SearchLimits {
            time_budget: if self.timeout_input.is_empty() {
                None
            } else {
                match self.timeout_input.parse::<humantime::Duration>() {
                    Ok(d) => Some(d.into()),
                    Err(e) => {
                        self.set_flash_error(format!("Invalid timeout: {}", e));
                        return;
                    }
                }
            },
            node_budget: if self.max_nodes_input.is_empty() {
                None
            } else {
                match self.max_nodes_input.parse::<u64>() {
                    Ok(n) => Some(n),
                    Err(e) => {
                        self.set_flash_error(format!("Invalid max nodes: {}", e));
                        return;
                    }
                }
            },
            memory_mb: None,
        };

        self.rx = Some(spawn_solver(task, choice, limits));
        self.outcome = None;
        self.solved_plan = None;
        self.plan_scroll_offset = 0;
        self.solving_state = SolvingState::Running;
    }

    fn clear_output(&mut self) {
        self.outcome = None;
        self.solved_plan = None;
        self.solving_state = SolvingState::Idle;
        self.error = None;
        self.plan_scroll_offset = 0;
    }

    pub fn reset_to_start(&mut self) {
        self.focus = AppFocus::DomainList;
        self.editing_input = None;
        self.outcome = None;
        self.solved_plan = None;
        self.solving_state = SolvingState::Idle;
        self.error = None;
        self.flash_error = None;
        self.flash_error_time = None;
        self.rx = None;
        self.plan_scroll_offset = 0;
        self.modal = None;
        self.modal_area_rect = None;
        self.rebuild_problem_list();
    }

    pub fn can_solve(&self) -> bool {
        !self.problem_blocked
            && self.problem_state.selected().is_some()
            && self.solving_state != SolvingState::Running
    }
}
