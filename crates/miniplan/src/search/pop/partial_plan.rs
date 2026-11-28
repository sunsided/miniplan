//! The partial plans that [`Pop`](super::Pop) searches over, and the refinements that
//! turn one partial plan into another.
//!
//! A partial plan holds steps, ordering constraints between them, causal links and the
//! open conditions no link supports yet. Two sentinel steps frame every plan:
//! [`INIT`] produces the initial state and [`GOAL`] consumes the goal.

use fixedbitset::FixedBitSet;

use crate::plan::{PartialOrder, Plan};
use crate::search::outcome::plan_from_ops;
use crate::task::{OpId, Operator, Task};

/// Index of the sentinel step whose effects are the initial state. Ordered before every
/// other step.
pub(super) const INIT: usize = 0;

/// Index of the sentinel step whose preconditions are the goal. Ordered after every other
/// step.
pub(super) const GOAL: usize = 1;

/// A fact required to hold (`positive`) or not to hold (`!positive`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Literal {
    /// Index into [`Task::facts`].
    pub(super) fact: usize,
    /// `true` for "the fact holds", `false` for "the fact does not hold".
    pub(super) positive: bool,
}

/// A causal link: [`Link::producer`] establishes [`Link::literal`] for
/// [`Link::consumer`], so no step may undo it in between.
#[derive(Debug, Clone, Copy)]
pub(super) struct Link {
    /// Step whose effect (or, for [`INIT`], the initial state) establishes the literal.
    pub(super) producer: usize,
    /// The literal the link protects.
    pub(super) literal: Literal,
    /// Step whose precondition (or, for [`GOAL`], the goal) needs the literal.
    pub(super) consumer: usize,
}

/// A step that may undo a [`Link`] because nothing orders it outside the link.
#[derive(Debug, Clone, Copy)]
pub(super) struct Threat {
    /// The threatened link.
    pub(super) link: Link,
    /// The step whose effect undoes the link's literal.
    pub(super) step: usize,
}

/// An open condition: a precondition of [`OpenCondition::consumer`] no link supports yet.
#[derive(Debug, Clone, Copy)]
pub(super) struct OpenCondition {
    /// The step that needs the literal.
    pub(super) consumer: usize,
    /// The needed literal.
    pub(super) literal: Literal,
}

/// A partial-order plan: steps, ordering constraints, causal links and open conditions.
///
/// # Invariants
///
/// - `ops[INIT]` and `ops[GOAL]` are `None`; every other entry is `Some`.
/// - `after` is transitively closed and acyclic: `after[a]` contains `b` exactly when `a`
///   is ordered before `b`. [`INIT`] is before and [`GOAL`] after every other step.
/// - `rank_cost` is the summed cost of the operators in `ops`, each counted as at least 1.
#[derive(Debug, Clone)]
pub(super) struct PartialPlan {
    /// The operator index of each step; `None` for the sentinels.
    ops: Vec<Option<usize>>,
    /// Transitive closure of the ordering constraints, one row per step.
    after: Vec<FixedBitSet>,
    /// Causal links established so far.
    links: Vec<Link>,
    /// Preconditions no link supports yet. The last one was added last.
    pub(super) open: Vec<OpenCondition>,
    /// Summed operator cost of the steps, each step counted as at least 1.
    ///
    /// Only for ranking: the floor keeps zero-cost steps from piling up at equal priority,
    /// so only finitely many partial plans rank below any bound and best-first search
    /// stays complete. The reported plan cost comes from the real operator costs.
    pub(super) rank_cost: f64,
}

impl PartialPlan {
    /// The null plan of `task`: just [`INIT`] before [`GOAL`], with every goal literal open.
    pub(super) fn null(task: &Task) -> Self {
        let mut init_row = FixedBitSet::with_capacity(2);
        init_row.insert(GOAL);

        let goal_literals = literals(&task.goal_pos, true).chain(literals(&task.goal_neg, false));
        let open = goal_literals
            .map(|literal| OpenCondition {
                consumer: GOAL,
                literal,
            })
            .collect();

        Self {
            ops: vec![None, None],
            after: vec![init_row, FixedBitSet::with_capacity(2)],
            links: Vec::new(),
            open,
            rank_cost: 0.0,
        }
    }

    /// The number of steps, sentinels included.
    pub(super) fn num_steps(&self) -> usize {
        self.ops.len()
    }

    /// Whether `a` is ordered before `b`, directly or transitively.
    pub(super) fn is_before(&self, a: usize, b: usize) -> bool {
        self.after[a].contains(b)
    }

    /// Orders `a` before `b` and closes the order transitively.
    ///
    /// Returns `false`, leaving the plan in an unspecified state, if that would form a
    /// cycle. Callers drop the plan then.
    pub(super) fn order(&mut self, a: usize, b: usize) -> bool {
        if a == b || self.is_before(b, a) {
            return false;
        }

        if self.is_before(a, b) {
            return true;
        }

        let mut successors = self.after[b].clone();
        successors.insert(b);
        for x in 0..self.num_steps() {
            if x == a || self.is_before(x, a) {
                self.after[x].union_with(&successors);
            }
        }

        true
    }

    /// Adds a step for `task.operators[op]`, between the sentinels, with its preconditions
    /// open. Returns the new step's index.
    pub(super) fn add_step(&mut self, task: &Task, op: usize) -> usize {
        let step = self.num_steps();
        let operator = &task.operators[op];

        self.ops.push(Some(op));
        for row in &mut self.after {
            row.grow(step + 1);
        }

        self.after.push(FixedBitSet::with_capacity(step + 1));
        self.after[step].insert(GOAL);
        // INIT has no predecessors, so it is the only row that gains the new step.
        self.after[INIT].insert(step);

        let pre = literals(&operator.pre_pos, true).chain(literals(&operator.pre_neg, false));
        self.open.extend(pre.map(|literal| OpenCondition {
            consumer: step,
            literal,
        }));
        self.rank_cost += f64::from(operator.cost.max(1));

        step
    }

    /// Records `link` and orders its producer before its consumer.
    ///
    /// Returns `false` if the ordering would form a cycle.
    pub(super) fn add_link(&mut self, link: Link) -> bool {
        self.links.push(link);
        self.order(link.producer, link.consumer)
    }

    /// Whether `step` makes `literal` true: as part of the initial state for [`INIT`], as a
    /// net effect of its operator otherwise.
    pub(super) fn achieves(&self, task: &Task, step: usize, literal: Literal) -> bool {
        match self.ops[step] {
            None if step == INIT => task.init.0.contains(literal.fact) == literal.positive,
            None => false,
            Some(op) => operator_achieves(&task.operators[op], literal),
        }
    }

    /// The first link some step threatens, if any.
    ///
    /// A step threatens a link when its operator undoes the link's literal and the order
    /// still allows it to run between producer and consumer. The sentinels threaten
    /// nothing.
    pub(super) fn first_threat(&self, task: &Task) -> Option<Threat> {
        self.links.iter().find_map(|&link| {
            (0..self.num_steps())
                .find(|&step| self.threatens(task, step, link))
                .map(|step| Threat { link, step })
        })
    }

    /// Whether `step` threatens `link`; see [`Self::first_threat`].
    fn threatens(&self, task: &Task, step: usize, link: Link) -> bool {
        let Some(op) = self.ops[step] else {
            return false;
        };

        step != link.producer
            && step != link.consumer
            && !self.is_before(step, link.producer)
            && !self.is_before(link.consumer, step)
            && operator_clobbers(&task.operators[op], link.literal)
    }

    /// Turns a plan without flaws into a [`Plan`] with its [`PartialOrder`].
    ///
    /// The steps are linearized topologically, lowest step index first among the steps
    /// that are ready, so the result is deterministic.
    pub(super) fn to_plan(&self, task: &Task) -> Plan {
        let linear = self.linearize();
        let ops: Vec<OpId> = linear
            .iter()
            .filter_map(|&step| self.ops[step].map(OpId))
            .collect();

        let mut plan = plan_from_ops(&ops, task);
        plan.partial_order = Some(self.reduced_order(&linear));
        plan
    }

    /// The real (non-sentinel) steps in a topological order of the ordering constraints.
    fn linearize(&self) -> Vec<usize> {
        let real = GOAL + 1..self.num_steps();
        let mut placed = FixedBitSet::with_capacity(self.num_steps());
        let mut linear = Vec::with_capacity(real.len());

        while linear.len() < real.len() {
            let next = real
                .clone()
                .find(|&s| !placed.contains(s) && self.predecessors_placed(s, &placed))
                .expect("the ordering constraints are acyclic");
            placed.insert(next);
            linear.push(next);
        }

        linear
    }

    /// Whether every real step ordered before `step` is in `placed`.
    fn predecessors_placed(&self, step: usize, placed: &FixedBitSet) -> bool {
        (GOAL + 1..self.num_steps()).all(|x| !self.is_before(x, step) || placed.contains(x))
    }

    /// The transitive reduction of the order among the real steps, as pairs of positions
    /// in `linear`, sorted ascending.
    fn reduced_order(&self, linear: &[usize]) -> PartialOrder {
        let mut before = Vec::new();
        for (i, &a) in linear.iter().enumerate() {
            for (j, &b) in linear.iter().enumerate().skip(i + 1) {
                let implied = linear[i + 1..j]
                    .iter()
                    .any(|&k| self.is_before(a, k) && self.is_before(k, b));
                if self.is_before(a, b) && !implied {
                    before.push((i, j));
                }
            }
        }

        PartialOrder { before }
    }
}

/// The literals of the facts in `facts`, all with polarity `positive`.
fn literals(facts: &crate::task::State, positive: bool) -> impl Iterator<Item = Literal> + '_ {
    facts.0.ones().map(move |fact| Literal { fact, positive })
}

/// Whether applying `op` leaves `literal` true.
///
/// [`State::apply`](crate::task::State::apply) applies adds before deletes, so a fact in
/// both sets ends up false.
pub(super) fn operator_achieves(op: &Operator, literal: Literal) -> bool {
    let deleted = op.del.0.contains(literal.fact);
    if literal.positive {
        op.add.0.contains(literal.fact) && !deleted
    } else {
        deleted
    }
}

/// Whether applying `op` makes `literal` false, the dual of [`operator_achieves`].
fn operator_clobbers(op: &Operator, literal: Literal) -> bool {
    operator_achieves(
        op,
        Literal {
            fact: literal.fact,
            positive: !literal.positive,
        },
    )
}
