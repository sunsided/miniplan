//! Meet detection between a forward and a backward search, shared by the cost-aware
//! bidirectional planners ([`BiDij`](super::BiDij), [`Nbs`](super::Nbs),
//! [`Bae`](super::Bae)).
//!
//! Every expanded forward state and every expanded backward subgoal is appended to an arena.
//! Per-fact bucket indices over those arenas keep the scan for a matching partner short: a
//! new forward state only visits subgoals that require one of its facts, and a new subgoal
//! only visits the forward states holding its rarest required fact.

use fixedbitset::FixedBitSet;

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

/// Arenas of expanded forward states and backward subgoals, indexed by fact, plus the
/// cheapest [`Meet`] found so far.
///
/// Arena indices are `u32` to keep the buckets small.
pub(super) struct MeetIndex {
    /// Expanded forward states with their `g` values, in expansion order.
    forward: Vec<(State, f64)>,
    /// For each fact, the [`MeetIndex::forward`] indices of states that hold it.
    forward_by_fact: Vec<Vec<u32>>,
    /// Expanded backward subgoals with their `g` values, in expansion order.
    backward: Vec<(Subgoal, f64)>,
    /// For each fact, the [`MeetIndex::backward`] indices of subgoals that require it.
    backward_by_pos_fact: Vec<Vec<u32>>,
    /// [`MeetIndex::backward`] indices of subgoals that require no fact at all, which no
    /// bucket in [`MeetIndex::backward_by_pos_fact`] reaches.
    backward_empty_pos: Vec<u32>,
    /// Scratch set of backward indices already visited by the current forward scan, so a
    /// subgoal found through several facts is tested only once.
    seen_backward: FixedBitSet,
    /// The cheapest meet found so far.
    best: Option<Meet>,
}

impl MeetIndex {
    /// Creates empty arenas for a task with `num_facts` facts.
    pub(super) fn new(num_facts: usize) -> Self {
        Self {
            forward: Vec::new(),
            forward_by_fact: vec![Vec::new(); num_facts],
            backward: Vec::new(),
            backward_by_pos_fact: vec![Vec::new(); num_facts],
            backward_empty_pos: Vec::new(),
            seen_backward: FixedBitSet::with_capacity(0),
            best: None,
        }
    }

    /// The cheapest meet found so far.
    pub(super) fn best(&self) -> Option<&Meet> {
        self.best.as_ref()
    }

    /// Returns `true` once at least one backward subgoal has been expanded.
    pub(super) fn has_backward(&self) -> bool {
        !self.backward.is_empty()
    }

    /// Records the expanded forward `state` at cost `g` and checks it against every expanded
    /// subgoal it satisfies.
    ///
    /// Candidates come from the buckets of `state`'s facts, then from the subgoals that
    /// require no fact; each is tested once.
    pub(super) fn add_forward(&mut self, state: &State, g: f64) {
        let idx = arena_index(self.forward.len());
        self.forward.push((state.clone(), g));
        for f in state.0.ones() {
            self.forward_by_fact[f].push(idx);
        }

        self.seen_backward.grow(self.backward.len());
        self.seen_backward.clear();

        let buckets = state.0.ones().map(|f| &self.backward_by_pos_fact[f]);
        for &idx in buckets.flatten().chain(&self.backward_empty_pos) {
            if !self.seen_backward.put(idx as usize) {
                let (subgoal, subgoal_g) = &self.backward[idx as usize];
                try_meet(&mut self.best, state, g, subgoal, *subgoal_g);
            }
        }
    }

    /// Records the expanded backward `subgoal` at cost `g` and checks it against every
    /// expanded forward state that satisfies it.
    ///
    /// Only the forward states holding the subgoal's rarest required fact are candidates; a
    /// subgoal that requires no fact is checked against every forward state.
    pub(super) fn add_backward(&mut self, subgoal: &Subgoal, g: f64) {
        let idx = arena_index(self.backward.len());
        self.backward.push((subgoal.clone(), g));
        let required: Vec<_> = subgoal.0.0.ones().collect();
        if required.is_empty() {
            self.backward_empty_pos.push(idx);
        } else {
            for &f in &required {
                self.backward_by_pos_fact[f].push(idx);
            }
        }

        if let Some(rarest) = self.rarest_fact(&required) {
            for &idx in &self.forward_by_fact[rarest] {
                let (state, state_g) = &self.forward[idx as usize];
                try_meet(&mut self.best, state, *state_g, subgoal, g);
            }
        } else {
            for (state, state_g) in &self.forward {
                try_meet(&mut self.best, state, *state_g, subgoal, g);
            }
        }
    }

    /// Of the `required` facts, the one held by the fewest forward states (the first such
    /// fact on ties), or `None` if `required` is empty.
    fn rarest_fact(&self, required: &[usize]) -> Option<usize> {
        required
            .iter()
            .min_by_key(|&&f| self.forward_by_fact[f].len())
            .copied()
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
