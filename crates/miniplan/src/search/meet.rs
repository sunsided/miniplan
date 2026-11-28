//! Meet detection between a forward and a backward search, shared by the cost-aware
//! bidirectional planners ([`BiDij`](super::BiDij), [`Nbs`](super::Nbs),
//! [`Bae`](super::Bae)).
//!
//! Every labelled forward state and backward subgoal (a root, or a node opened or improved by
//! an expansion) and every expanded one is appended to an arena; see [`MeetIndex`] for which
//! pairs are checked and why. Checking only expanded-against-expanded pairs is not enough: an
//! optimal plan can meet at a node that the other side has only generated, and the planners'
//! stopping rules would then accept a more expensive meet.
//!
//! Per-fact bucket indices over those arenas keep the scan for a matching partner short: a
//! new forward state only visits subgoals keyed by one of its facts, and a new subgoal only
//! visits the forward states holding its rarest required fact.

use crate::plan::Plan;
use crate::search::outcome::{ParentMap, plan_through};
use crate::search::regression::Subgoal;
use crate::task::{State, Task};

/// A forward state that satisfies a backward subgoal, so the two half-paths join into a plan.
pub(super) struct Meet {
    /// The forward state at the end of the forward half-path.
    state: State,
    /// The backward subgoal at the start of the backward half-path.
    subgoal: Subgoal,
    /// Total cost of both half-paths: forward `g` of `state` plus backward `g` of `subgoal`.
    pub(super) cost: f64,
}

impl Meet {
    /// Joins the forward path to [`Meet::state`] and the backward path from
    /// [`Meet::subgoal`] into one plan.
    pub(super) fn plan(
        &self,
        forward_parents: &ParentMap<State>,
        backward_parents: &ParentMap<Subgoal>,
        task: &Task,
    ) -> Plan {
        plan_through(
            forward_parents,
            &self.state,
            backward_parents,
            &self.subgoal,
            task,
        )
    }
}

/// Arenas of the forward states and backward subgoals the search has seen, indexed by fact,
/// plus the cheapest [`Meet`] found so far.
///
/// Each side keeps two arenas: every *labelled* node (a root, or a node opened or improved by
/// an expansion) and every *expanded* node. A newly labelled node is checked against the
/// other side's expanded nodes, a newly expanded node against the other side's labelled
/// nodes. That covers every pair in which one node is expanded, which is all the stopping
/// rules need: an optimal plan not found yet still has an open node with exact `g` on both
/// sides, whose `f` and `g` values bound the plan cost from below. Pairs of two merely
/// labelled nodes are skipped, which keeps the scans short.
pub(super) struct MeetIndex {
    /// Every labelled forward state.
    forward_labelled: ForwardArena,
    /// Every expanded forward state.
    forward_expanded: ForwardArena,
    /// Every labelled backward subgoal.
    backward_labelled: BackwardArena,
    /// Every expanded backward subgoal.
    backward_expanded: BackwardArena,
    /// The cheapest meet found so far.
    best: Option<Meet>,
}

impl MeetIndex {
    /// Creates empty arenas for a task with `num_facts` facts.
    pub(super) fn new(num_facts: usize) -> Self {
        Self {
            forward_labelled: ForwardArena::new(num_facts),
            forward_expanded: ForwardArena::new(num_facts),
            backward_labelled: BackwardArena::new(num_facts),
            backward_expanded: BackwardArena::new(num_facts),
            best: None,
        }
    }

    /// The cheapest meet found so far.
    pub(super) fn best(&self) -> Option<&Meet> {
        self.best.as_ref()
    }

    /// Returns `true` once at least one backward subgoal has been labelled.
    pub(super) fn has_backward(&self) -> bool {
        !self.backward_labelled.subgoals.is_empty()
    }

    /// Records the forward `state`, labelled at cost `g`, and checks it against every
    /// expanded subgoal it satisfies.
    pub(super) fn label_forward(&mut self, state: &State, g: f64) {
        self.forward_labelled.push(state, g);
        self.backward_expanded.meet(state, g, &mut self.best);
    }

    /// Records the forward `state`, expanded at cost `g`, and checks it against every
    /// labelled subgoal it satisfies.
    pub(super) fn expand_forward(&mut self, state: &State, g: f64) {
        self.forward_expanded.push(state, g);
        self.backward_labelled.meet(state, g, &mut self.best);
    }

    /// Records the backward `subgoal`, labelled at cost `g`, and checks it against every
    /// expanded forward state that satisfies it.
    pub(super) fn label_backward(&mut self, subgoal: &Subgoal, g: f64) {
        self.backward_labelled.push(subgoal, g);
        self.forward_expanded.meet(subgoal, g, &mut self.best);
    }

    /// Records the backward `subgoal`, expanded at cost `g`, and checks it against every
    /// labelled forward state that satisfies it.
    pub(super) fn expand_backward(&mut self, subgoal: &Subgoal, g: f64) {
        self.backward_expanded.push(subgoal, g);
        self.forward_labelled.meet(subgoal, g, &mut self.best);
    }
}

/// Forward states with their `g` values, indexed by the facts they hold.
///
/// Arena indices are `u32` to keep the buckets small.
struct ForwardArena {
    /// The states with their `g` values, in insertion order.
    states: Vec<(State, f64)>,
    /// For each fact, the [`ForwardArena::states`] indices of states that hold it.
    by_fact: Vec<Vec<u32>>,
}

impl ForwardArena {
    /// Creates an empty arena for `num_facts` facts.
    fn new(num_facts: usize) -> Self {
        Self {
            states: Vec::new(),
            by_fact: vec![Vec::new(); num_facts],
        }
    }

    /// Appends `state` at cost `g`.
    fn push(&mut self, state: &State, g: f64) {
        let idx = arena_index(self.states.len());
        self.states.push((state.clone(), g));
        for f in state.0.ones() {
            self.by_fact[f].push(idx);
        }
    }

    /// Offers every state that satisfies `subgoal` (at backward cost `g`) to `best`.
    ///
    /// Only the states holding the subgoal's rarest required fact are candidates; a subgoal
    /// that requires no fact is checked against every state.
    fn meet(&self, subgoal: &Subgoal, g: f64, best: &mut Option<Meet>) {
        let rarest = subgoal.0.0.ones().min_by_key(|&f| self.by_fact[f].len());

        if let Some(rarest) = rarest {
            for &idx in &self.by_fact[rarest] {
                let (state, state_g) = &self.states[idx as usize];
                try_meet(best, state, *state_g, subgoal, g);
            }
        } else {
            for (state, state_g) in &self.states {
                try_meet(best, state, *state_g, subgoal, g);
            }
        }
    }
}

/// Backward subgoals with their `g` values, each indexed by one fact it requires.
///
/// A state can only satisfy a subgoal that holds all of the subgoal's required facts, so one
/// key fact per subgoal is enough to find it, and each subgoal is visited at most once per
/// scan. The key is the required fact with the smallest bucket at insertion time, which
/// keeps the buckets balanced.
///
/// Arena indices are `u32` to keep the buckets small.
struct BackwardArena {
    /// The subgoals with their `g` values, in insertion order.
    subgoals: Vec<(Subgoal, f64)>,
    /// For each fact, the [`BackwardArena::subgoals`] indices of subgoals keyed by it.
    by_key_fact: Vec<Vec<u32>>,
    /// [`BackwardArena::subgoals`] indices of subgoals that require no fact at all, which no
    /// bucket in [`BackwardArena::by_key_fact`] reaches.
    empty_pos: Vec<u32>,
}

impl BackwardArena {
    /// Creates an empty arena for `num_facts` facts.
    fn new(num_facts: usize) -> Self {
        Self {
            subgoals: Vec::new(),
            by_key_fact: vec![Vec::new(); num_facts],
            empty_pos: Vec::new(),
        }
    }

    /// Appends `subgoal` at cost `g`.
    fn push(&mut self, subgoal: &Subgoal, g: f64) {
        let idx = arena_index(self.subgoals.len());
        self.subgoals.push((subgoal.clone(), g));
        let key = subgoal
            .0
            .0
            .ones()
            .min_by_key(|&f| self.by_key_fact[f].len());
        match key {
            Some(f) => self.by_key_fact[f].push(idx),
            None => self.empty_pos.push(idx),
        }
    }

    /// Offers every subgoal that `state` (at forward cost `g`) satisfies to `best`.
    ///
    /// Candidates are the subgoals keyed by one of `state`'s facts, then the subgoals that
    /// require no fact.
    fn meet(&self, state: &State, g: f64, best: &mut Option<Meet>) {
        let buckets = state.0.ones().map(|f| &self.by_key_fact[f]);
        for &idx in buckets.flatten().chain(&self.empty_pos) {
            let (subgoal, subgoal_g) = &self.subgoals[idx as usize];
            try_meet(best, state, g, subgoal, *subgoal_g);
        }
    }
}

/// Replaces `best` with the meet of `state` and `subgoal` if `state` satisfies `subgoal`
/// and their combined cost `forward_g + backward_g` is strictly below the current best.
fn try_meet(
    best: &mut Option<Meet>,
    state: &State,
    forward_g: f64,
    subgoal: &Subgoal,
    backward_g: f64,
) {
    if !state.satisfies(&subgoal.0, &subgoal.1) {
        return;
    }

    let cost = forward_g + backward_g;
    if best.as_ref().is_none_or(|meet| cost < meet.cost) {
        *best = Some(Meet {
            state: state.clone(),
            subgoal: subgoal.clone(),
            cost,
        });
    }
}

/// Converts an arena length or position into a bucket index.
///
/// # Panics
///
/// Panics if the arena outgrows `u32`, which needs more than four billion expansions on one
/// side, far beyond any memory budget.
fn arena_index(position: usize) -> u32 {
    u32::try_from(position).expect("search arena holds fewer than 2^32 entries")
}
