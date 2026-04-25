use pddl::Term;
use pddl::{AtomicFormula, ConditionalEffect, EffectCondition, Effects, FunctionTerm, PrimitiveEffect};

use crate::error::MiniplanError;
use crate::ground::formula::{LiteralSet, build_state_from_literals, walk_goal_definition};
use crate::task::{CondEffect, Fact, State, Task};

pub fn extract_effects(
    effects: &Option<Effects>,
    bindings: &[(String, String)],
    task: &Task,
) -> Result<(State, State, Vec<CondEffect>), MiniplanError> {
    let mut add_state = State::new(task.num_facts());
    let mut del_state = State::new(task.num_facts());
    let mut conditional = Vec::new();

    let effects = match effects {
        Some(e) => e,
        None => return Ok((add_state, del_state, conditional)),
    };

    for ceffect in effects.iter() {
        match ceffect {
            ConditionalEffect::Effect(pe) => {
                apply_plain_effect(pe, bindings, task, &mut add_state, &mut del_state)?;
            }
            ConditionalEffect::Forall(forall) => {
                // Forall effects are expanded at grounding time.
                for ce in forall.effects.iter() {
                    if let ConditionalEffect::Effect(pe) = ce {
                        apply_plain_effect(pe, bindings, task, &mut add_state, &mut del_state)?;
                    }
                }
            }
            ConditionalEffect::When(when) => {
                let cond_literals = walk_goal_definition(&when.condition, bindings)?;
                let (cond_pos, cond_neg) = if cond_literals.is_empty() {
                    (State::new(task.num_facts()), State::new(task.num_facts()))
                } else {
                    let merged = merge_literal_sets(&cond_literals);
                    build_state_from_literals(&merged, task)?
                };

                let mut cond_add = State::new(task.num_facts());
                let mut cond_del = State::new(task.num_facts());
                for pe in flatten_effect_condition(&when.effect) {
                    apply_plain_effect(&pe, bindings, task, &mut cond_add, &mut cond_del)?;
                }

                conditional.push(CondEffect {
                    cond_pos,
                    cond_neg,
                    add: cond_add,
                    del: cond_del,
                });
            }
        }
    }

    Ok((add_state, del_state, conditional))
}

fn flatten_effect_condition(ec: &EffectCondition) -> Vec<PrimitiveEffect> {
    match ec {
        EffectCondition::Single(pe) => vec![pe.clone()],
        EffectCondition::All(pes) => pes.clone(),
    }
}

fn apply_plain_effect(
    pe: &PrimitiveEffect,
    bindings: &[(String, String)],
    task: &Task,
    add_state: &mut State,
    del_state: &mut State,
) -> Result<(), MiniplanError> {
    match pe {
        PrimitiveEffect::AtomicFormula(af) => {
            if let AtomicFormula::Predicate(pred) = af {
                let name = pred.predicate().to_string();
                let args: Vec<String> = pred
                    .values()
                    .iter()
                    .map(|t| term_to_string(t, bindings))
                    .collect();
                let fact = Fact {
                    predicate: name,
                    args,
                };
                if let Some(id) = task.fact_id(&fact) {
                    add_state.set(id, true);
                }
            }
        }
        PrimitiveEffect::NotAtomicFormula(af) => {
            if let AtomicFormula::Predicate(pred) = af {
                let name = pred.predicate().to_string();
                let args: Vec<String> = pred
                    .values()
                    .iter()
                    .map(|t| term_to_string(t, bindings))
                    .collect();
                let fact = Fact {
                    predicate: name,
                    args,
                };
                if let Some(id) = task.fact_id(&fact) {
                    del_state.set(id, true);
                }
            }
        }
        PrimitiveEffect::AssignNumericFluent(_, _, _) => {
            // Numeric effects handled in cost.rs
        }
        PrimitiveEffect::AssignObjectFluent(_, _) => {
            // Object fluents not supported in v1
        }
    }
    Ok(())
}

fn term_to_string(term: &Term, bindings: &[(String, String)]) -> String {
    match term {
        Term::Name(n) => n.to_string(),
        Term::Variable(v) => {
            let var_name = v.to_string();
            bindings
                .iter()
                .find(|(name, _)| name == &var_name)
                .map(|(_, val)| val.clone())
                .unwrap_or_else(|| var_name)
        }
        Term::Function(ft) => function_term_to_string(ft, bindings),
    }
}

fn function_term_to_string(ft: &FunctionTerm, bindings: &[(String, String)]) -> String {
    let symbol = ft.symbol().to_string();
    let args: Vec<String> = ft
        .terms()
        .iter()
        .map(|t| term_to_string(t, bindings))
        .collect();
    if args.is_empty() {
        symbol
    } else {
        format!("{}({})", symbol, args.join(","))
    }
}

fn merge_literal_sets(sets: &[LiteralSet]) -> LiteralSet {
    let mut pos = Vec::new();
    let mut neg = Vec::new();
    for s in sets {
        pos.extend_from_slice(&s.pos);
        neg.extend_from_slice(&s.neg);
    }
    LiteralSet { pos, neg }
}
