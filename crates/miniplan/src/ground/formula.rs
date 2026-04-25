use pddl::{AtomicFormula, FunctionTerm, GoalDefinition, Literal, Term};

use crate::error::MiniplanError;
use crate::task::{Fact, State, Task};

// NOTE: walk_goal_definition ignores Exists/ForAll bound variables (drops them
// and walks the body under outer bindings).  For quantifier-aware evaluation
// of derived-predicate bodies, use derived::eval_gd instead.

#[derive(Debug, Clone)]
pub struct LiteralSet {
    pub pos: Vec<(String, Vec<String>)>,
    pub neg: Vec<(String, Vec<String>)>,
}

type LiteralPair = (Vec<(String, Vec<String>)>, Vec<(String, Vec<String>)>);

pub fn walk_goal_definition(
    gd: &GoalDefinition,
    bindings: &[(String, String)],
) -> Result<Vec<LiteralSet>, MiniplanError> {
    match gd {
        GoalDefinition::AtomicFormula(af) => walk_atomic_formula(af, bindings),
        GoalDefinition::Literal(lit) => walk_literal(lit, bindings),
        GoalDefinition::And(gds) => {
            let mut results: Vec<LiteralSet> = vec![LiteralSet {
                pos: Vec::new(),
                neg: Vec::new(),
            }];
            for g in gds.iter() {
                let sub = walk_goal_definition(g, bindings)?;
                results = cartesian_product_dnf(&results, &sub);
            }
            Ok(results)
        }
        GoalDefinition::Or(gds) => {
            let mut all = Vec::new();
            for g in gds.iter() {
                all.extend(walk_goal_definition(g, bindings)?);
            }
            Ok(all)
        }
        GoalDefinition::Not(inner) => match inner.as_ref() {
            GoalDefinition::AtomicFormula(af) => walk_negated_atomic_formula(af, bindings),
            GoalDefinition::Literal(lit) => walk_negated_literal(lit, bindings),
            _ => Err(MiniplanError::Ground(
                "negation of non-atom in goal not supported".into(),
            )),
        },
        GoalDefinition::Imply(_, _) => {
            Err(MiniplanError::Ground("imply in goal not supported".into()))
        }
        GoalDefinition::ForAll(_vars, body) => walk_goal_definition(body, bindings),
        GoalDefinition::Exists(_vars, body) => walk_goal_definition(body, bindings),
        GoalDefinition::FluentComparison(_) => Err(MiniplanError::Ground(
            "numeric comparison in goal not supported in v1".into(),
        )),
    }
}

fn walk_literal(
    lit: &Literal<Term>,
    bindings: &[(String, String)],
) -> Result<Vec<LiteralSet>, MiniplanError> {
    if lit.is_negated() {
        if let Literal::NotAtomicFormula(af) = lit {
            walk_negated_atomic_formula(af, bindings)
        } else {
            walk_literal_inner(lit, bindings)
        }
    } else {
        if let Literal::AtomicFormula(af) = lit {
            walk_atomic_formula(af, bindings)
        } else {
            walk_literal_inner(lit, bindings)
        }
    }
}

fn walk_literal_inner(
    lit: &Literal<Term>,
    bindings: &[(String, String)],
) -> Result<Vec<LiteralSet>, MiniplanError> {
    let af = match lit {
        Literal::AtomicFormula(af) => af,
        Literal::NotAtomicFormula(af) => af,
    };
    let (pos, neg) = walk_atomic_formula_inner(af, bindings)?;
    Ok(vec![LiteralSet { pos, neg }])
}

fn walk_atomic_formula(
    af: &AtomicFormula<Term>,
    bindings: &[(String, String)],
) -> Result<Vec<LiteralSet>, MiniplanError> {
    let (pos, neg) = walk_atomic_formula_inner(af, bindings)?;
    Ok(vec![LiteralSet { pos, neg }])
}

fn walk_negated_atomic_formula(
    af: &AtomicFormula<Term>,
    bindings: &[(String, String)],
) -> Result<Vec<LiteralSet>, MiniplanError> {
    match af {
        AtomicFormula::Equality(eq) => {
            let first = term_to_string(eq.first(), bindings);
            let second = term_to_string(eq.second(), bindings);
            if first != second {
                Ok(vec![LiteralSet {
                    pos: Vec::new(),
                    neg: Vec::new(),
                }])
            } else {
                Ok(vec![])
            }
        }
        AtomicFormula::Predicate(pred) => {
            let name = pred.predicate().to_string();
            let args: Vec<String> = pred
                .values()
                .iter()
                .map(|t| term_to_string(t, bindings))
                .collect();
            Ok(vec![LiteralSet {
                pos: Vec::new(),
                neg: vec![(name, args)],
            }])
        }
    }
}

fn walk_negated_literal(
    lit: &Literal<Term>,
    bindings: &[(String, String)],
) -> Result<Vec<LiteralSet>, MiniplanError> {
    let af = match lit {
        Literal::NotAtomicFormula(af) => af,
        Literal::AtomicFormula(af) => af,
    };
    walk_negated_atomic_formula(af, bindings)
}

fn walk_atomic_formula_inner(
    af: &AtomicFormula<Term>,
    bindings: &[(String, String)],
) -> Result<LiteralPair, MiniplanError> {
    match af {
        AtomicFormula::Equality(eq) => {
            let _first = term_to_string(eq.first(), bindings);
            let _second = term_to_string(eq.second(), bindings);
            // Equality is handled as a constraint, not a fact
            Ok((Vec::new(), Vec::new()))
        }
        AtomicFormula::Predicate(pred) => {
            let name = pred.predicate().to_string();
            let args: Vec<String> = pred
                .values()
                .iter()
                .map(|t| term_to_string(t, bindings))
                .collect();
            Ok((vec![(name, args)], Vec::new()))
        }
    }
}

pub(crate) fn term_to_string(term: &Term, bindings: &[(String, String)]) -> String {
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

fn cartesian_product_dnf(left: &[LiteralSet], right: &[LiteralSet]) -> Vec<LiteralSet> {
    let mut result = Vec::new();
    for l in left {
        for r in right {
            let mut pos = l.pos.clone();
            pos.extend_from_slice(&r.pos);
            let mut neg = l.neg.clone();
            neg.extend_from_slice(&r.neg);
            result.push(LiteralSet { pos, neg });
        }
    }
    if result.is_empty() {
        if !left.is_empty() {
            result.extend_from_slice(left);
        }
        if !right.is_empty() {
            result.extend_from_slice(right);
        }
    }
    result
}

pub fn build_state_from_literals(
    literals: &LiteralSet,
    task: &Task,
) -> Result<(State, State), MiniplanError> {
    let mut pos_state = State::new(task.num_facts());
    let mut neg_state = State::new(task.num_facts());

    for (pred, args) in &literals.pos {
        let fact = Fact {
            predicate: pred.clone(),
            args: args.clone(),
        };
        if let Some(id) = task.fact_id(&fact) {
            pos_state.set(id, true);
        }
    }

    for (pred, args) in &literals.neg {
        let fact = Fact {
            predicate: pred.clone(),
            args: args.clone(),
        };
        if let Some(id) = task.fact_id(&fact) {
            neg_state.set(id, true);
        }
    }

    Ok((pos_state, neg_state))
}
