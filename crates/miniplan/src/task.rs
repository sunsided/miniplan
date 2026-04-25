//! Grounded task representation.
//!
//! This module defines the core data structures for a grounded planning task:
//! facts, operators, states, and the [`Task`] itself.

use std::fmt;
use std::hash::{Hash, Hasher};

use fixedbitset::FixedBitSet;
use rustc_hash::FxHashMap;

/// A unique identifier for a fact (predicate instance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FactId(pub usize);

/// A unique identifier for an operator (grounded action).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OpId(pub usize);

/// A named object with an associated type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Object {
    /// The object's name.
    pub name: String,
    /// The object's type name.
    pub type_name: String,
}

/// A hierarchy of types with single inheritance.
///
/// # Examples
///
/// ```
/// use miniplan::task::TypeHierarchy;
///
/// let mut th = TypeHierarchy::new();
/// th.add_type("vehicle".to_string(), None);
/// th.add_type("car".to_string(), Some("vehicle".to_string()));
/// assert!(th.is_subtype_of("car", "vehicle"));
/// assert!(th.is_subtype_of("car", "object"));
/// assert!(!th.is_subtype_of("vehicle", "car"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct TypeHierarchy {
    /// Maps each subtype to its direct supertype.
    pub supertypes: FxHashMap<String, String>,
}

impl TypeHierarchy {
    /// Create an empty type hierarchy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if `child` is a subtype of `parent` (reflexive).
    pub fn is_subtype_of(&self, child: &str, parent: &str) -> bool {
        if child == parent || parent == "object" {
            return true;
        }
        if let Some(sup) = self.supertypes.get(child) {
            return self.is_subtype_of(sup, parent);
        }
        false
    }

    /// Add a type with an optional supertype.
    pub fn add_type(&mut self, name: String, supertype: Option<String>) {
        if let Some(sup) = supertype {
            self.supertypes.insert(name, sup);
        }
    }
}

/// A grounded fact (predicate instance with arguments).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fact {
    /// The predicate name.
    pub predicate: String,
    /// The grounded arguments.
    pub args: Vec<String>,
}

/// A conditional effect: applied only when conditions hold.
#[derive(Debug, Clone)]
pub struct CondEffect {
    /// Facts that must be true for the effect to apply.
    pub cond_pos: State,
    /// Facts that must be false for the effect to apply.
    pub cond_neg: State,
    /// Facts to add when the effect applies.
    pub add: State,
    /// Facts to delete when the effect applies.
    pub del: State,
}

/// A set of facts represented as a bitset.
///
/// Used both for states (which facts are true) and for goal specifications.
#[derive(Clone)]
pub struct State(pub FixedBitSet);

impl State {
    /// Create a new state with the given number of facts.
    pub fn new(size: usize) -> Self {
        State(FixedBitSet::with_capacity(size))
    }

    /// Create an empty state (zero capacity).
    pub fn empty() -> Self {
        State(FixedBitSet::new())
    }

    /// Set or clear a fact in this state.
    pub fn set(&mut self, id: FactId, value: bool) {
        self.0.set(id.0, value);
    }

    /// Check if a fact is true in this state.
    pub fn contains(&self, id: FactId) -> bool {
        self.0.contains(id.0)
    }

    /// Returns the number of bits in the bitset.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the bitset has zero capacity.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Check if an operator's preconditions are satisfied in this state.
    pub fn applicable(&self, op: &Operator) -> bool {
        for bit in op.pre_pos.0.ones() {
            if !self.0.contains(bit) {
                return false;
            }
        }
        for bit in op.pre_neg.0.ones() {
            if self.0.contains(bit) {
                return false;
            }
        }
        true
    }

    /// Apply an operator to produce the resulting state.
    pub fn apply(&self, op: &Operator) -> Self {
        let mut next = self.clone();
        for bit in op.add.0.ones() {
            next.0.set(bit, true);
        }
        for bit in op.del.0.ones() {
            next.0.set(bit, false);
        }
        for cond in &op.conditional {
            let cond_holds = cond.cond_pos.0.ones().all(|b| self.0.contains(b))
                && cond.cond_neg.0.ones().all(|b| !self.0.contains(b));
            if cond_holds {
                for bit in cond.add.0.ones() {
                    next.0.set(bit, true);
                }
                for bit in cond.del.0.ones() {
                    next.0.set(bit, false);
                }
            }
        }
        next
    }

    /// Check if this state satisfies the given goal specification.
    pub fn satisfies(&self, goal_pos: &State, goal_neg: &State) -> bool {
        for bit in goal_pos.0.ones() {
            if !self.0.contains(bit) {
                return false;
            }
        }
        for bit in goal_neg.0.ones() {
            if self.0.contains(bit) {
                return false;
            }
        }
        true
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        self.0.ones().eq(other.0.ones())
    }
}

impl Eq for State {}

impl Hash for State {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_slice().hash(state);
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "State({:?})", self.0.ones().collect::<Vec<_>>())
    }
}

/// A grounded planning operator.
#[derive(Debug, Clone)]
pub struct Operator {
    /// Unique identifier for this operator.
    pub id: OpId,
    /// Human-readable name.
    pub name: String,
    /// Positive preconditions (facts that must be true).
    pub pre_pos: State,
    /// Negative preconditions (facts that must be false).
    pub pre_neg: State,
    /// Add effects (facts to make true).
    pub add: State,
    /// Delete effects (facts to make false).
    pub del: State,
    /// Conditional effects.
    pub conditional: Vec<CondEffect>,
    /// The cost of executing this operator.
    pub cost: u32,
}

/// Metadata about a planning task.
#[derive(Debug, Clone)]
pub struct TaskMeta {
    /// The domain name.
    pub domain_name: String,
    /// The problem name.
    pub problem_name: String,
    /// PDDL requirements used.
    pub requirements: Vec<String>,
}

/// A fully grounded planning task.
///
/// Contains all facts, operators, the initial state, and the goal specification.
#[derive(Debug)]
pub struct Task {
    /// All grounded facts.
    pub facts: Vec<Fact>,
    /// Maps facts to their IDs.
    pub fact_index: FxHashMap<Fact, FactId>,
    /// All grounded operators.
    pub operators: Vec<Operator>,
    /// The initial state.
    pub init: State,
    /// Positive goal facts (must be true).
    pub goal_pos: State,
    /// Negative goal facts (must be false).
    pub goal_neg: State,
    /// Objects in the problem.
    pub objects: Vec<Object>,
    /// Type hierarchy.
    pub types: TypeHierarchy,
    /// Task metadata.
    pub metadata: TaskMeta,
}

impl Task {
    /// Look up the ID of a fact.
    pub fn fact_id(&self, fact: &Fact) -> Option<FactId> {
        self.fact_index.get(fact).copied()
    }

    /// Returns the total number of facts in this task.
    pub fn num_facts(&self) -> usize {
        self.facts.len()
    }
}
