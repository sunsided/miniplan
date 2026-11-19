//! Builders for small hand-written tasks used by the planner unit tests.

use rustc_hash::FxHashMap;

use crate::plan::Plan;
use crate::search::{SearchOutcome, SearchStats};
use crate::task::{Fact, FactId, OpId, Operator, State, Task, TaskMeta, TypeHierarchy};

/// Number of fact slots in the states built by [`make_op`]. Large enough for every test task.
const OP_STATE_SIZE: usize = 10;

/// A state of `size` facts with exactly the facts in `bits` set.
pub(super) fn state_of(bits: &[usize], size: usize) -> State {
    let mut state = State::new(size);
    for &b in bits {
        state.0.set(b, true);
    }

    state
}

/// A task over `num_facts` nullary facts `f0`, `f1`, … with the given initial facts,
/// positive and negative goal facts, and operators.
pub(super) fn make_test_task(
    num_facts: usize,
    init_bits: &[usize],
    goal_pos_bits: &[usize],
    goal_neg_bits: &[usize],
    operators: Vec<Operator>,
) -> Task {
    let mut facts = Vec::new();
    let mut fact_index = FxHashMap::default();
    for i in 0..num_facts {
        let fact = Fact {
            predicate: format!("f{i}"),
            args: vec![],
        };
        fact_index.insert(fact.clone(), FactId(i));
        facts.push(fact);
    }

    Task {
        facts,
        fact_index,
        operators,
        init: state_of(init_bits, num_facts),
        goal_pos: state_of(goal_pos_bits, num_facts),
        goal_neg: state_of(goal_neg_bits, num_facts),
        objects: vec![],
        types: TypeHierarchy::new(),
        metadata: TaskMeta {
            domain_name: "test".to_string(),
            problem_name: "test".to_string(),
            requirements: vec![],
        },
    }
}

/// A STRIPS operator with the given id, name, precondition, effect and cost, without
/// conditional effects.
pub(super) fn make_op(
    id: usize,
    name: &str,
    pre_pos: &[usize],
    pre_neg: &[usize],
    add: &[usize],
    del: &[usize],
    cost: u32,
) -> Operator {
    Operator {
        id: OpId(id),
        name: name.to_string(),
        pre_pos: state_of(pre_pos, OP_STATE_SIZE),
        pre_neg: state_of(pre_neg, OP_STATE_SIZE),
        add: state_of(add, OP_STATE_SIZE),
        del: state_of(del, OP_STATE_SIZE),
        conditional: vec![],
        cost,
    }
}

/// Unwraps a [`SearchOutcome::Plan`].
///
/// # Panics
///
/// Panics if `outcome` is not a plan.
pub(super) fn expect_plan(outcome: SearchOutcome) -> (Plan, SearchStats) {
    let SearchOutcome::Plan(plan, stats) = outcome else {
        panic!("expected a plan, got {outcome:?}");
    };

    (plan, stats)
}
