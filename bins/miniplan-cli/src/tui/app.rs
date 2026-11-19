//! TUI state: the loaded bundle, list selections, solver settings and the running search.
//!
//! Keyboard handling lives in [`keys`], mouse handling in [`mouse`]; both mutate [`App`].
//! Focus moves through the panels with Tab in this order:
//!
//! ```mermaid
//! flowchart LR
//!     D[DomainList] --> P[ProblemList] --> T[Timeout] --> M[MaxNodes] --> O[OutputPath]
//!     O --> S[SolveButton] --> C[CommandBar] --> D
//!     PS[PlanScroll] --> C
//! ```

use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use miniplan::ground::ground;
use miniplan::pddl_io::PddlBundle;
use miniplan::plan::Plan;
use miniplan::search::{
    PlannerChoice, PlannerConfig, PlannerKind, Registry, SearchLimits, SearchOutcome, Solver,
};
use miniplan::task::Task;
use ratatui::layout::Rect;
use ratatui::widgets::{ListState, ScrollbarState};

use crate::plan_writer::OutputFormat;
use crate::tui::solver::{SolverEvent, spawn_solver};

mod keys;
mod mouse;

/// The panel or widget that receives keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFocus {
    /// The domain list in the top left.
    DomainList,
    /// The problem list in the top right; Tab from the domain list cannot reach it while no
    /// problem matches the selected domain.
    ProblemList,
    /// One of the text fields of the solver settings.
    InputField(InputField),
    /// The plan panel, scrolled with the arrow keys.
    PlanScroll,
    /// The command bar at the bottom.
    CommandBar,
    /// The solve button of the solver settings.
    SolveButton,
}

/// An editable text field in the solver settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    /// Search time budget, parsed with [`crate::duration::parse_duration`]; empty means none.
    Timeout,
    /// Node expansion budget as an unsigned integer; empty means none.
    MaxNodes,
    /// Where `w` writes the plan; `-` means stdout.
    OutputPath,
}

impl InputField {
    /// The focus Tab moves to from this field.
    const fn next_focus(self) -> AppFocus {
        match self {
            Self::Timeout => AppFocus::InputField(Self::MaxNodes),
            Self::MaxNodes => AppFocus::InputField(Self::OutputPath),
            Self::OutputPath => AppFocus::SolveButton,
        }
    }

    /// The focus Shift+Tab moves to from this field.
    const fn previous_focus(self) -> AppFocus {
        match self {
            Self::Timeout => AppFocus::ProblemList,
            Self::MaxNodes => AppFocus::InputField(Self::Timeout),
            Self::OutputPath => AppFocus::InputField(Self::MaxNodes),
        }
    }
}

/// Lifecycle of the current search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolvingState {
    /// No search started since the last clear or reset.
    Idle,
    /// A search runs on the background thread.
    Running,
    /// The search returned an outcome (plan, unsolvable or limit reached).
    Completed,
    /// The search returned an error or was cancelled.
    Failed,
}

/// Which selection modal is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    /// Choose an entry of [`App::planners`].
    Planner,
    /// Choose an entry of [`App::heuristics`].
    Heuristic,
}

/// A planner offered in the planner modal.
pub struct PlannerEntry {
    /// Registry name, parsed into a [`PlannerKind`] when solving.
    pub name: String,
    /// One-line description shown next to the name.
    pub description: String,
}

/// A heuristic offered in the heuristic modal.
pub struct HeuristicEntry {
    /// Name passed to the planner as the `heuristic` option.
    pub name: String,
    /// One-line description shown next to the name.
    pub description: String,
}

/// A list of items with a ratatui selection that wraps around at both ends.
pub struct StatefulList<T> {
    /// Selected index and scroll offset; the selection is `None` only when `items` is empty.
    pub state: ListState,
    /// The items in display order.
    pub items: Vec<T>,
}

impl<T> StatefulList<T> {
    /// Creates a list that selects the first item, if there is one.
    pub fn with_items(items: Vec<T>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }

        Self { state, items }
    }

    /// Selects the next item, wrapping from the last to the first.
    pub fn next(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let i = self
            .state
            .selected()
            .map_or(0, |i| wrap_next(i, self.items.len()));
        self.state.select(Some(i));
    }

    /// Selects the previous item, wrapping from the first to the last.
    pub fn previous(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let i = self
            .state
            .selected()
            .map_or(0, |i| wrap_previous(i, self.items.len()));
        self.state.select(Some(i));
    }

    /// The selected item, if any.
    pub fn selected(&self) -> Option<&T> {
        self.state.selected().and_then(|i| self.items.get(i))
    }
}

/// The index after `i` in a list of `len > 0` items, wrapping to `0` past the end.
const fn wrap_next(i: usize, len: usize) -> usize {
    if i >= len - 1 { 0 } else { i + 1 }
}

/// The index before `i` in a list of `len > 0` items, wrapping to the last item before `0`.
const fn wrap_previous(i: usize, len: usize) -> usize {
    if i == 0 { len - 1 } else { i - 1 }
}

/// The whole TUI state, mutated by input handlers and read by [`crate::tui::view`].
pub struct App {
    /// Every domain and problem loaded from the input files.
    pub bundle: PddlBundle,

    /// The widget that receives keyboard input.
    pub focus: AppFocus,
    /// The field in text-edit mode, if any; typed characters go there.
    pub editing_input: Option<InputField>,

    /// Indices into `bundle.domains`, one per distinct domain name (first occurrence wins).
    pub domain_state: StatefulList<usize>,
    /// Scrollbar of the domain panel.
    pub domain_scroll_state: ScrollbarState,
    /// Domain names that occur more than once in the bundle; shown with a `*`.
    pub duplicated_domains: HashSet<String>,
    /// Indices into `bundle.problems` whose domain matches the selected domain.
    pub compatible_problem_indices: Vec<usize>,
    /// The problem list; its items are the same as `compatible_problem_indices`.
    pub problem_state: StatefulList<usize>,
    /// Scrollbar of the problem panel.
    pub problem_scroll_state: ScrollbarState,
    /// `true` when no problem matches the selected domain, which blocks solving.
    pub problem_blocked: bool,

    /// Planners from the solver registry.
    pub planners: Vec<PlannerEntry>,
    /// Index into `planners`.
    pub selected_planner: usize,
    /// The heuristics offered in the heuristic modal.
    pub heuristics: Vec<HeuristicEntry>,
    /// Index into `heuristics`.
    pub selected_heuristic: usize,
    /// Raw text of the timeout field.
    pub timeout_input: String,
    /// Raw text of the max-nodes field.
    pub max_nodes_input: String,
    /// Format `w` writes the plan in.
    pub output_format: OutputFormat,
    /// Raw text of the output path field; `-` means stdout.
    pub output_path: String,
    /// `false` for blind planners, which ignore heuristics; hides the heuristic row.
    pub heuristic_enabled: bool,

    /// Lifecycle of the current search.
    pub solving_state: SolvingState,
    /// Channel of the running search; `None` when idle or cancelled.
    pub rx: Option<Receiver<SolverEvent>>,
    /// Result of the last search as text errors; `Err("quit")` asks the loop to exit.
    pub outcome: Option<Result<SearchOutcome, String>>,
    /// The plan of the last successful search.
    pub solved_plan: Option<Plan>,

    /// Index of the first visible line in the plan panel.
    pub plan_scroll_offset: usize,

    /// The open modal, if any; it captures all keyboard and mouse input.
    pub modal: Option<ModalKind>,
    /// Selection inside the open modal.
    pub modal_list_state: ListState,
    /// Scrollbar of the modal.
    pub modal_scroll_state: ScrollbarState,

    /// Last grounding or planner error; set but not currently displayed.
    pub error: Option<String>,
    /// Short message shown in the middle of the screen for two seconds.
    pub flash_error: Option<String>,
    /// When `flash_error` was set; used to expire it.
    pub flash_error_time: Option<Instant>,

    /// Hit areas from the last render, used to route mouse clicks.
    pub areas: HitAreas,
}

/// Screen regions recorded by the last render, so mouse events can be mapped to widgets.
///
/// All are `None` (or `0`) until the first frame is drawn.
#[derive(Default)]
pub struct HitAreas {
    /// Outer rectangle of the domain panel, border included.
    pub domain: Option<Rect>,
    /// Outer rectangle of the problem panel, border included.
    pub problem: Option<Rect>,
    /// Outer rectangle of the solver settings panel.
    pub config: Option<Rect>,
    /// Outer rectangle of the plan panel.
    pub plan: Option<Rect>,
    /// The solve button.
    pub solve_button: Option<Rect>,
    /// Screen row of the planner line in the settings panel.
    pub config_planner_row: u16,
    /// Screen row of the heuristic line in the settings panel.
    pub config_heuristic_row: u16,
    /// Outer rectangle of the open modal; `None` while no modal is shown.
    pub modal: Option<Rect>,
}

/// Planners that search without a heuristic; selecting one disables the heuristic row.
const BLIND_PLANNERS: &[&str] = &["bfs", "bibfs-uc", "bidij"];

/// How long a flash message stays on screen.
const FLASH_DURATION: Duration = Duration::from_secs(2);

impl App {
    /// Builds the initial state for `bundle`: first domain selected, A* with hFF, 300 s timeout.
    pub fn new(bundle: PddlBundle) -> Self {
        let planners = planner_entries(&Solver::new().registry);
        let selected_planner = planners.iter().position(|p| p.name == "astar").unwrap_or(0);
        let (deduped_indices, duplicated_domains) = dedupe_domains(&bundle);

        let mut app = Self {
            bundle,
            focus: AppFocus::DomainList,
            editing_input: None,

            domain_state: StatefulList::with_items(deduped_indices),
            domain_scroll_state: ScrollbarState::default(),
            duplicated_domains,
            compatible_problem_indices: Vec::new(),
            problem_state: StatefulList::with_items(Vec::new()),
            problem_scroll_state: ScrollbarState::default(),
            problem_blocked: true,

            planners,
            selected_planner,
            heuristics: default_heuristics(),
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
            modal_scroll_state: ScrollbarState::default(),

            error: None,
            flash_error: None,
            flash_error_time: None,

            areas: HitAreas::default(),
        };

        app.rebuild_problem_list();
        app.domain_scroll_state = app
            .domain_scroll_state
            .content_length(app.domain_state.items.len());
        app
    }

    /// Number of problems in the bundle that declare `domain_name` as their domain.
    pub fn problem_count_for_domain(&self, domain_name: &str) -> usize {
        self.bundle
            .problems
            .iter()
            .filter(|(_, p)| *p.domain() == domain_name)
            .count()
    }

    /// Refills the problem list with the problems of the selected domain and selects the first.
    fn rebuild_problem_list(&mut self) {
        let Some(&domain_bundle_idx) = self.domain_state.selected() else {
            self.compatible_problem_indices.clear();
            self.problem_state = StatefulList::with_items(Vec::new());
            self.problem_scroll_state = self.problem_scroll_state.content_length(0);
            self.problem_blocked = true;
            return;
        };

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

        self.compatible_problem_indices.clone_from(&compatible);
        self.problem_scroll_state = self.problem_scroll_state.content_length(compatible.len());
        self.problem_state = StatefulList::with_items(compatible);
        self.problem_blocked = self.compatible_problem_indices.is_empty();
    }

    /// Shows `msg` in the flash line for [`FLASH_DURATION`]; also used for success messages.
    pub fn set_flash_error(&mut self, msg: String) {
        self.flash_error = Some(msg);
        self.flash_error_time = Some(Instant::now());
    }

    /// Per-frame housekeeping: expires the flash message and collects a finished search.
    pub fn tick(&mut self) {
        if let Some(t) = self.flash_error_time
            && t.elapsed() > FLASH_DURATION
        {
            self.flash_error = None;
            self.flash_error_time = None;
        }

        if self.solving_state == SolvingState::Running
            && let Some(rx) = &self.rx
            && let Ok(event) = rx.try_recv()
        {
            let SolverEvent::Done(result) = event;
            self.finish_solving(result.map_err(|e| e.to_string()));
        }
    }

    /// Stores the result of a finished search and keeps its plan, if it found one.
    fn finish_solving(&mut self, result: Result<SearchOutcome, String>) {
        if let Ok(SearchOutcome::Plan(plan, _)) = &result {
            self.solved_plan = Some(plan.clone());
        }

        self.solving_state = if result.is_ok() {
            SolvingState::Completed
        } else {
            SolvingState::Failed
        };
        self.outcome = Some(result);
    }

    /// Asks the event loop to exit after the current event.
    fn request_quit(&mut self) {
        self.outcome = Some(Err("quit".to_string()));
    }

    /// Number of entries in the open modal; `0` when none is open.
    const fn modal_len(&self) -> usize {
        match self.modal {
            Some(ModalKind::Planner) => self.planners.len(),
            Some(ModalKind::Heuristic) => self.heuristics.len(),
            None => 0,
        }
    }

    /// Applies the modal selection to the planner or heuristic and closes the modal.
    fn close_modal(&mut self) {
        match self.modal {
            Some(ModalKind::Planner) => {
                if let Some(i) = self.modal_list_state.selected() {
                    self.selected_planner = i.min(self.planners.len() - 1);
                }

                self.update_heuristic_enabled();
            }
            Some(ModalKind::Heuristic) => {
                if let Some(i) = self.modal_list_state.selected() {
                    self.selected_heuristic = i.min(self.heuristics.len() - 1);
                }
            }
            None => {}
        }

        self.modal = None;
        self.areas.modal = None;
    }

    /// Number of lines the plan panel shows: header, blank line and one per step.
    fn plan_line_count(&self) -> usize {
        self.solved_plan.as_ref().map_or(1, |p| p.steps.len() + 2)
    }

    /// Opens the planner modal with the current planner selected.
    fn open_planner_modal(&mut self) {
        self.modal = Some(ModalKind::Planner);
        self.modal_list_state.select(Some(self.selected_planner));
    }

    /// Opens the heuristic modal with the current heuristic selected.
    fn open_heuristic_modal(&mut self) {
        self.modal = Some(ModalKind::Heuristic);
        self.modal_list_state.select(Some(self.selected_heuristic));
    }

    /// Re-derives [`App::heuristic_enabled`] from the selected planner.
    fn update_heuristic_enabled(&mut self) {
        let planner_name = &self.planners[self.selected_planner].name;
        self.heuristic_enabled = !BLIND_PLANNERS.contains(&planner_name.as_str());
    }

    /// Grounds the selected problem and starts a background search with the current settings.
    ///
    /// Invalid input (no problem, grounding failure, bad planner, timeout or node budget)
    /// aborts with a flash message or [`App::error`] and leaves the previous result in place.
    fn start_solving(&mut self) {
        let Some(task) = self.ground_selected() else {
            return;
        };
        let Some(choice) = self.planner_choice() else {
            return;
        };
        let Some(limits) = self.search_limits() else {
            return;
        };

        self.rx = Some(spawn_solver(task, choice, limits));
        self.outcome = None;
        self.solved_plan = None;
        self.plan_scroll_offset = 0;
        self.solving_state = SolvingState::Running;
    }

    /// Grounds the selected domain and problem, or reports why it cannot.
    fn ground_selected(&mut self) -> Option<Task> {
        let selection = (!self.problem_blocked)
            .then(|| {
                self.problem_state
                    .selected()
                    .zip(self.domain_state.selected())
            })
            .flatten();
        let Some((&prob_idx, &domain_bundle_idx)) = selection else {
            self.set_flash_error("No problem selected or domain has no problems".to_string());
            return None;
        };

        let (_, domain) = &self.bundle.domains[domain_bundle_idx];
        let (_, problem) = &self.bundle.problems[prob_idx];

        match ground(domain, problem) {
            Ok(task) => Some(task),
            Err(e) => {
                self.error = Some(format!("Grounding failed: {e}"));
                None
            }
        }
    }

    /// Builds the planner selection, passing the heuristic only when it is enabled.
    fn planner_choice(&mut self) -> Option<PlannerChoice> {
        let kind: PlannerKind = match self.planners[self.selected_planner].name.parse() {
            Ok(k) => k,
            Err(e) => {
                self.error = Some(format!("Invalid planner: {e}"));
                return None;
            }
        };

        let heuristic = self
            .heuristic_enabled
            .then(|| self.heuristics[self.selected_heuristic].name.clone());
        let mut config = PlannerConfig::default();
        if let Some(name) = &heuristic {
            config.opts.insert("heuristic".to_owned(), name.clone());
        }

        Some(PlannerChoice {
            kind,
            heuristic,
            config,
        })
    }

    /// Parses the timeout and node budget fields; an empty field means no limit.
    fn search_limits(&mut self) -> Option<SearchLimits> {
        let time_budget = match parse_optional(&self.timeout_input, crate::duration::parse_duration)
        {
            Ok(d) => d,
            Err(e) => {
                self.set_flash_error(format!("Invalid timeout: {e}"));
                return None;
            }
        };

        let node_budget = match parse_optional(&self.max_nodes_input, str::parse::<u64>) {
            Ok(n) => n,
            Err(e) => {
                self.set_flash_error(format!("Invalid max nodes: {e}"));
                return None;
            }
        };

        Some(SearchLimits {
            time_budget,
            node_budget,
            memory_mb: None,
        })
    }

    /// Drops the last result and plan; settings and selections stay.
    fn clear_output(&mut self) {
        self.outcome = None;
        self.solved_plan = None;
        self.solving_state = SolvingState::Idle;
        self.error = None;
        self.plan_scroll_offset = 0;
    }

    /// Returns to the domain list and forgets results, messages, modals and any running search.
    ///
    /// Selections and solver settings are kept.
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
        self.areas.modal = None;
        self.rebuild_problem_list();
    }

    /// `true` when a problem is selected and no search is running.
    pub fn can_solve(&self) -> bool {
        !self.problem_blocked
            && self.problem_state.selected().is_some()
            && self.solving_state != SolvingState::Running
    }
}

/// Parses `input` with `parse`, treating an empty string as "not set".
///
/// # Errors
///
/// Returns the parser's error for non-empty input it rejects.
fn parse_optional<T, E>(input: &str, parse: impl Fn(&str) -> Result<T, E>) -> Result<Option<T>, E> {
    if input.is_empty() {
        Ok(None)
    } else {
        parse(input).map(Some)
    }
}

/// Lists every planner of `registry` with its description.
fn planner_entries(registry: &Registry) -> Vec<PlannerEntry> {
    registry
        .planners()
        .map(|p| PlannerEntry {
            name: p.name.to_string(),
            description: p.description.to_string(),
        })
        .collect()
}

/// The heuristics the TUI offers, in modal order; the first is the default.
fn default_heuristics() -> Vec<HeuristicEntry> {
    [
        ("hff", "Relaxed-plan (FF) heuristic"),
        ("hadd", "Additive heuristic"),
        ("hmax", "Max heuristic"),
        ("goal-count", "Count of unsatisfied goals"),
        ("blind", "Blind heuristic (h=1)"),
    ]
    .into_iter()
    .map(|(name, description)| HeuristicEntry {
        name: name.to_string(),
        description: description.to_string(),
    })
    .collect()
}

/// Returns one bundle index per distinct domain name (first wins) and the repeated names.
fn dedupe_domains(bundle: &PddlBundle) -> (Vec<usize>, HashSet<String>) {
    let mut seen = HashSet::new();
    let mut duplicated = HashSet::new();
    let mut indices = Vec::new();

    for (idx, (_, domain)) in bundle.domains.iter().enumerate() {
        let name = domain.name().to_string();
        if seen.contains(&name) {
            duplicated.insert(name);
        } else {
            seen.insert(name);
            indices.push(idx);
        }
    }

    (indices, duplicated)
}
