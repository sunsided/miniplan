//! Grounding of action effects into add, delete and conditional effect sets.

use pddl::{
    AtomicFormula, ConditionalEffect, EffectCondition, Effects, PrimitiveEffect,
    WhenConditionalEffect,
};

use crate::error::MiniplanError;
use crate::ground::formula::{
    LiteralSet, build_state_from_literals, ground_atom, walk_goal_description,
};
use crate::ground::types::Binding;
use crate::task::{CondEffect, Fact, State, Task};

/// Grounded effects of one operator: `(add, delete, conditional effects)`.
type GroundEffects = (State, State, Vec<CondEffect>);

/// Grounds an action's effects under `bindings`.
///
/// Plain effects go to the add and delete sets. The plain effects directly
/// inside a `forall` are applied under the outer bindings only (the quantified
/// variable is not expanded); nested `forall`/`when` inside a `forall` are
/// dropped. Each `when` becomes one [`CondEffect`]. Numeric and object fluent
/// effects are ignored here; action costs are read by
/// [`extract_action_cost`](crate::ground::cost::extract_action_cost).
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when a `when` condition is rejected by
/// [`walk_goal_description`] (negated non-atom, `imply`, numeric comparison).
pub(super) fn extract_effects(
    effects: Option<&Effects>,
    bindings: &[Binding],
    task: &Task,
) -> Result<GroundEffects, MiniplanError> {
    let mut add_state = State::new(task.num_facts());
    let mut del_state = State::new(task.num_facts());
    let mut conditional = Vec::new();

    let Some(effects) = effects else {
        return Ok((add_state, del_state, conditional));
    };

    for ceffect in effects {
        match ceffect {
            ConditionalEffect::Effect(pe) => {
                apply_plain_effect(pe, bindings, task, &mut add_state, &mut del_state);
            }
            ConditionalEffect::Forall(forall) => {
                for ce in &forall.effects {
                    if let ConditionalEffect::Effect(pe) = ce {
                        apply_plain_effect(pe, bindings, task, &mut add_state, &mut del_state);
                    }
                }
            }
            ConditionalEffect::When(when) => {
                conditional.push(ground_when_effect(when, bindings, task)?);
            }
        }
    }

    Ok((add_state, del_state, conditional))
}

/// Grounds a `(when condition effect)` into a [`CondEffect`].
///
/// A disjunctive condition is flattened into one conjunction of all its
/// literals; an unsatisfiable condition becomes the always-true condition.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when [`walk_goal_description`] rejects the
/// condition.
fn ground_when_effect(
    when: &WhenConditionalEffect,
    bindings: &[Binding],
    task: &Task,
) -> Result<CondEffect, MiniplanError> {
    let cond_literals = walk_goal_description(&when.condition, bindings)?;
    let (cond_pos, cond_neg) = if cond_literals.is_empty() {
        (State::new(task.num_facts()), State::new(task.num_facts()))
    } else {
        build_state_from_literals(&merge_literal_sets(&cond_literals), task)
    };

    let mut add = State::new(task.num_facts());
    let mut del = State::new(task.num_facts());
    for pe in flatten_effect_condition(&when.effect) {
        apply_plain_effect(&pe, bindings, task, &mut add, &mut del);
    }

    Ok(CondEffect {
        cond_pos,
        cond_neg,
        add,
        del,
    })
}

/// Returns the primitive effects of a `when` effect body as a list.
fn flatten_effect_condition(ec: &EffectCondition) -> Vec<PrimitiveEffect> {
    match ec {
        EffectCondition::Single(pe) => vec![pe.clone()],
        EffectCondition::All(pes) => pes.clone(),
    }
}

/// Sets the bit of a grounded primitive effect in `add_state` (positive atom)
/// or `del_state` (negated atom).
///
/// Atoms outside the fact universe and equality atoms are dropped. Numeric
/// fluent effects are handled by the cost extraction, and object fluents are
/// not supported, so both are no-ops here.
fn apply_plain_effect(
    pe: &PrimitiveEffect,
    bindings: &[Binding],
    task: &Task,
    add_state: &mut State,
    del_state: &mut State,
) {
    let (af, target) = match pe {
        PrimitiveEffect::AtomicFormula(af) => (af, add_state),
        PrimitiveEffect::NotAtomicFormula(af) => (af, del_state),
        PrimitiveEffect::AssignNumericFluent(_, _, _)
        | PrimitiveEffect::AssignObjectFluent(_, _) => {
            return;
        }
    };

    if let AtomicFormula::Predicate(pred) = af {
        let (predicate, args) = ground_atom(pred, bindings);
        if let Some(id) = task.fact_id(&Fact { predicate, args }) {
            target.set(id, true);
        }
    }
}

/// Concatenates the literals of all conjunctions into a single conjunction.
fn merge_literal_sets(sets: &[LiteralSet]) -> LiteralSet {
    let mut merged = LiteralSet::default();
    for s in sets {
        merged.pos.extend_from_slice(&s.pos);
        merged.neg.extend_from_slice(&s.neg);
    }

    merged
}
