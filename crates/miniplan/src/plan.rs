//! Plan representation.
//!
//! A plan is a totally-ordered sequence of grounded operator applications
//! whose execution transforms the initial state into a goal-satisfying state.
//! Plans are produced by the planners in [`crate::search`].
//!
//! The steps are always a valid sequence. A plan-space planner such as
//! [`Pop`](crate::search::Pop) additionally reports the [`PartialOrder`] it
//! actually needs: any order of the steps that respects it is a valid plan too.
//!
//! The [`Plan`] struct carries two invariants:
//!
//! - `steps` records *which* grounded operators to apply and in what order.
//! - `cost` is the numeric sum of the executed operators' costs, so empty
//!   plans always have `cost == 0.0`.
//!
//! The [`Display`](std::fmt::Display) impl on [`Plan`] emits a PDDL-style
//! human-readable format.

use std::fmt;

use crate::task::OpId;

/// A single step in a [`Plan`].
///
/// A `PlanStep` names one grounded-operator application inside a plan.
/// The `op_id` is the authoritative identifier (an index into the
/// [`Task`](crate::task::Task)'s operator list), while `op_name` is a
/// redundant, pretty-printed copy carried along so that plans can be
/// displayed or serialised without needing the originating `Task`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanStep {
    /// The operator ID that identifies this step.
    ///
    /// Indexes into `crate::task::Task::operators` of the task that
    /// produced this plan. See [`OpId`] for details.
    pub op_id: OpId,
    /// A human-readable operator name (typically [`Operator::name`](crate::task::Operator::name),
    /// e.g. `"move-A-B"`).
    ///
    /// Not guaranteed unique; intended for display and serialisation only —
    /// do not parse it to recover an operator.
    pub op_name: String,
}

/// The output produced by a planner when search succeeds.
///
/// This is the structure wrapped by [`SearchOutcome::Plan`](crate::search::SearchOutcome::Plan).
///
/// # Invariants
///
/// - `steps` is ordered: `steps[0]` is applied first.
/// - `cost` equals the sum of the applied operators' cost values; it is not
///   necessarily `steps.len()` because operator costs need not be unit.
/// - An empty plan (`steps` is empty, `cost == 0.0`) is a valid result when
///   the initial state already satisfies the goal.
///
/// # Examples
///
/// ```
/// use miniplan::plan::{Plan, PlanStep};
/// use miniplan::task::OpId;
///
/// let plan = Plan::new();
/// assert!(plan.is_empty());
/// assert_eq!(plan.len(), 0);
/// assert_eq!(plan.cost, 0.0);
///
/// let mut plan = Plan::new();
/// plan.steps.push(PlanStep { op_id: OpId(0), op_name: "pick".into() });
/// plan.cost = 1.0;
/// assert!(!plan.is_empty());
/// assert_eq!(plan.len(), 1);
///
/// assert_eq!(
///     plan.to_string(),
///     "; cost = 1\n; length = 1\n(pick)\n"
/// );
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Ordered list of [`PlanStep`]s; execute left-to-right.
    pub steps: Vec<PlanStep>,
    /// Accumulated cost of the steps.
    ///
    /// Must stay in sync with `steps` — callers mutate both together.
    pub cost: f64,
    /// The ordering constraints the plan needs, if the planner knows them.
    ///
    /// `None` for the state-space planners, which only know the total order of `steps`.
    pub partial_order: Option<PartialOrder>,
}

/// The ordering constraints between the steps of a [`Plan`].
///
/// Every linearization of the steps that respects these constraints reaches the goal.
/// Steps not ordered against each other, directly or transitively, are independent.
///
/// # Invariants
///
/// - Each pair `(i, j)` indexes into [`Plan::steps`] and has `i < j`, so `steps`
///   itself respects the order.
/// - The pairs are a transitive reduction: no pair follows from the others.
/// - The pairs are sorted ascending.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PartialOrder {
    /// Pairs `(i, j)` meaning step `i` must run before step `j`.
    pub before: Vec<(usize, usize)>,
}

impl Default for Plan {
    /// Creates an empty plan with `steps` empty and `cost` set to `0.0`.
    ///
    /// Equivalent to [`Plan::new`].
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            cost: 0.0,
            partial_order: None,
        }
    }
}

impl Plan {
    /// Create an empty plan.
    ///
    /// The returned value satisfies `steps.is_empty()` and `cost == 0.0`.
    /// This is equivalent to `Plan::default()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use miniplan::plan::Plan;
    ///
    /// let plan = Plan::new();
    /// assert!(plan.is_empty());
    /// assert_eq!(plan.cost, 0.0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the plan has no steps.
    ///
    /// # Examples
    ///
    /// ```
    /// use miniplan::plan::Plan;
    ///
    /// assert!(Plan::new().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns the number of steps in the plan.
    ///
    /// This counts steps, not cost.
    ///
    /// # Examples
    ///
    /// ```
    /// use miniplan::plan::Plan;
    ///
    /// assert_eq!(Plan::new().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }
}

/// Human-readable display format for a [`Plan`].
///
/// Emits a PDDL-style plan file:
///
/// - Line 1: `; cost = <cost>` (semicolons are PDDL plan comments).
/// - Line 2: `; length = <len>`.
/// - Lines 3..: one `(<op_name>)` per step, each terminated by `\n`.
/// - With a [`PartialOrder`], one `; order <i> < <j>` comment line per pair after
///   the steps, with 0-based step indices.
///
/// The trailing newline after the last step matches the `writeln!` macro.
///
/// # Examples
///
/// ```
/// use miniplan::plan::{Plan, PlanStep};
/// use miniplan::task::OpId;
///
/// let mut plan = Plan::new();
/// plan.steps.push(PlanStep { op_id: OpId(0), op_name: "pick".into() });
/// plan.steps.push(PlanStep { op_id: OpId(1), op_name: "place".into() });
/// plan.cost = 2.0;
///
/// assert_eq!(
///     plan.to_string(),
///     "; cost = 2\n; length = 2\n(pick)\n(place)\n"
/// );
/// ```
impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "; cost = {}", self.cost)?;
        writeln!(f, "; length = {}", self.len())?;
        for step in &self.steps {
            writeln!(f, "({})", step.op_name)?;
        }

        for (before, after) in self.partial_order.iter().flat_map(|order| &order.before) {
            writeln!(f, "; order {before} < {after}")?;
        }

        Ok(())
    }
}
