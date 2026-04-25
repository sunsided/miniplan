use std::fmt;
use std::hash::{Hash, Hasher};

use fixedbitset::FixedBitSet;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FactId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OpId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Object {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct TypeHierarchy {
    pub supertypes: FxHashMap<String, String>,
}

impl TypeHierarchy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_subtype_of(&self, child: &str, parent: &str) -> bool {
        if child == parent || parent == "object" {
            return true;
        }
        if let Some(sup) = self.supertypes.get(child) {
            return self.is_subtype_of(sup, parent);
        }
        false
    }

    pub fn add_type(&mut self, name: String, supertype: Option<String>) {
        if let Some(sup) = supertype {
            self.supertypes.insert(name, sup);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fact {
    pub predicate: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CondEffect {
    pub cond_pos: State,
    pub cond_neg: State,
    pub add: State,
    pub del: State,
}

#[derive(Clone)]
pub struct State(pub FixedBitSet);

impl State {
    pub fn new(size: usize) -> Self {
        State(FixedBitSet::with_capacity(size))
    }

    pub fn empty() -> Self {
        State(FixedBitSet::new())
    }

    pub fn set(&mut self, id: FactId, value: bool) {
        self.0.set(id.0, value);
    }

    pub fn contains(&self, id: FactId) -> bool {
        self.0.contains(id.0)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

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

#[derive(Debug, Clone)]
pub struct Operator {
    pub id: OpId,
    pub name: String,
    pub pre_pos: State,
    pub pre_neg: State,
    pub add: State,
    pub del: State,
    pub conditional: Vec<CondEffect>,
    pub cost: u32,
}

#[derive(Debug, Clone)]
pub struct TaskMeta {
    pub domain_name: String,
    pub problem_name: String,
    pub requirements: Vec<String>,
}

#[derive(Debug)]
pub struct Task {
    pub facts: Vec<Fact>,
    pub fact_index: FxHashMap<Fact, FactId>,
    pub operators: Vec<Operator>,
    pub init: State,
    pub goal_pos: State,
    pub goal_neg: State,
    pub objects: Vec<Object>,
    pub types: TypeHierarchy,
    pub metadata: TaskMeta,
}

impl Task {
    pub fn fact_id(&self, fact: &Fact) -> Option<FactId> {
        self.fact_index.get(fact).copied()
    }

    pub fn num_facts(&self) -> usize {
        self.facts.len()
    }
}
