//! Goal formulas to disjunctive normal form, and literal sets to fact bitsets.
//!
//! [`walk_goal_description`] ignores the variables bound by `exists` and
//! `forall` (it drops them and walks the body under the outer bindings). For
//! quantifier-aware evaluation of derived-predicate bodies, use
//! [`derived::eval_gd`](crate::ground::derived::eval_gd) instead.

use pddl::{AtomicFormula, FunctionTerm, GoalDescription, Literal, PredicateAtomicFormula, Term};

use crate::error::MiniplanError;
use crate::ground::types::Binding;
use crate::task::{Fact, State, Task};

/// A grounded atom as `(predicate name, argument names)`.
pub(super) type Atom = (String, Vec<String>);

/// One conjunction of a formula in disjunctive normal form.
///
/// A goal or precondition is a `Vec<LiteralSet>`: a disjunction of these
/// conjunctions. An empty vector is unsatisfiable (`false`), a set with both
/// lists empty is trivially true.
#[derive(Debug, Clone, Default)]
pub(super) struct LiteralSet {
    /// Atoms that must hold.
    pub pos: Vec<Atom>,
    /// Atoms that must not hold.
    pub neg: Vec<Atom>,
}

/// Converts a goal definition under `bindings` into disjunctive normal form.
///
/// `and` multiplies out its operands, `or` concatenates them. Equalities are
/// decided statically: a positive equality is dropped (always true), a negated
/// one is false when both sides ground to the same name. Quantifiers are not
/// expanded (see the module docs).
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] for a negated non-atomic formula, an
/// `imply`, or a numeric comparison anywhere in `gd`.
pub(super) fn walk_goal_description(
    gd: &GoalDescription,
    bindings: &[Binding],
) -> Result<Vec<LiteralSet>, MiniplanError> {
    match gd {
        GoalDescription::AtomicFormula(af) => Ok(vec![walk_atomic_formula(af, bindings)]),
        GoalDescription::Literal(lit) => Ok(walk_literal(lit, bindings)),
        GoalDescription::And(gds) => walk_conjunction(gds, bindings),
        GoalDescription::Or(gds) => {
            let mut all = Vec::new();
            for g in gds {
                all.extend(walk_goal_description(g, bindings)?);
            }

            Ok(all)
        }
        GoalDescription::Not(inner) => match inner.as_ref() {
            GoalDescription::AtomicFormula(af) => Ok(walk_negated_atomic_formula(af, bindings)),
            GoalDescription::Literal(lit) => Ok(walk_negated_literal(lit, bindings)),
            _ => Err(MiniplanError::Ground(
                "negation of non-atom in goal not supported".into(),
            )),
        },
        GoalDescription::Imply(_, _) => {
            Err(MiniplanError::Ground("imply in goal not supported".into()))
        }
        GoalDescription::ForAll(_, body) | GoalDescription::Exists(_, body) => {
            walk_goal_description(body, bindings)
        }
        GoalDescription::FluentComparison(_) => Err(MiniplanError::Ground(
            "numeric comparison in goal not supported in v1".into(),
        )),
    }
}

/// Converts the conjunction of `gds` into disjunctive normal form by
/// multiplying out the operands left to right.
///
/// # Errors
///
/// Propagates the errors of [`walk_goal_description`] for any operand.
fn walk_conjunction(
    gds: &[GoalDescription],
    bindings: &[Binding],
) -> Result<Vec<LiteralSet>, MiniplanError> {
    let mut results = vec![LiteralSet::default()];
    for g in gds {
        let sub = walk_goal_description(g, bindings)?;
        results = cartesian_product_dnf(&results, &sub);
    }

    Ok(results)
}

/// Converts a literal into disjunctive normal form.
fn walk_literal(lit: &Literal<Term>, bindings: &[Binding]) -> Vec<LiteralSet> {
    match lit {
        Literal::AtomicFormula(af) => vec![walk_atomic_formula(af, bindings)],
        Literal::NotAtomicFormula(af) => walk_negated_atomic_formula(af, bindings),
    }
}

/// Converts a positive atomic formula into a single conjunction.
///
/// Equalities are treated as constraints rather than facts and contribute no
/// literal.
fn walk_atomic_formula(af: &AtomicFormula<Term>, bindings: &[Binding]) -> LiteralSet {
    match af {
        AtomicFormula::Equality(_) => LiteralSet::default(),
        AtomicFormula::Predicate(pred) => LiteralSet {
            pos: vec![ground_atom(pred, bindings)],
            neg: Vec::new(),
        },
    }
}

/// Converts a negated atomic formula into disjunctive normal form.
///
/// A negated equality is decided right away: unsatisfiable (no conjunction)
/// when both sides ground to the same name, trivially true otherwise.
fn walk_negated_atomic_formula(af: &AtomicFormula<Term>, bindings: &[Binding]) -> Vec<LiteralSet> {
    match af {
        AtomicFormula::Equality(eq) => {
            let first = term_to_string(eq.first(), bindings);
            let second = term_to_string(eq.second(), bindings);
            if first == second {
                vec![]
            } else {
                vec![LiteralSet::default()]
            }
        }
        AtomicFormula::Predicate(pred) => vec![LiteralSet {
            pos: Vec::new(),
            neg: vec![ground_atom(pred, bindings)],
        }],
    }
}

/// Converts `(not lit)` into disjunctive normal form.
///
/// The literal's own polarity is ignored: `(not (not p))` is read as
/// `(not p)`.
fn walk_negated_literal(lit: &Literal<Term>, bindings: &[Binding]) -> Vec<LiteralSet> {
    match lit {
        Literal::NotAtomicFormula(af) | Literal::AtomicFormula(af) => {
            walk_negated_atomic_formula(af, bindings)
        }
    }
}

/// Grounds a predicate application under `bindings` into an [`Atom`].
pub(super) fn ground_atom(pred: &PredicateAtomicFormula<Term>, bindings: &[Binding]) -> Atom {
    let args = pred
        .values()
        .iter()
        .map(|t| term_to_string(t, bindings))
        .collect();

    (pred.predicate().to_string(), args)
}

/// Grounds a term to an object name.
///
/// Variables are replaced by their binding; unbound variables keep their own
/// name (including the `?`). Function terms render as `f(a,b)`.
pub(crate) fn term_to_string(term: &Term, bindings: &[Binding]) -> String {
    match term {
        Term::Name(n) => n.to_string(),
        Term::Variable(v) => {
            let var_name = v.to_string();
            bindings
                .iter()
                .find(|(name, _)| name == &var_name)
                .map_or_else(|| var_name, |(_, val)| val.clone())
        }
        Term::Function(ft) => function_term_to_string(ft, bindings),
    }
}

/// Renders a function term as `f(a,b)`, or just `f` without arguments.
fn function_term_to_string(ft: &FunctionTerm, bindings: &[Binding]) -> String {
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

/// Multiplies two formulas in disjunctive normal form (conjunction of two
/// disjunctions).
///
/// Every pair of conjunctions is merged. If either side is empty (false), the
/// product is empty; in that case the non-empty side is returned instead, so
/// an unsatisfiable operand of `and` is effectively dropped.
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
        result.extend_from_slice(left);
        result.extend_from_slice(right);
    }

    result
}

/// Converts a conjunction into `(positive, negative)` fact bitsets sized for
/// `task`.
///
/// Atoms outside the task's fact universe are silently dropped.
pub(super) fn build_state_from_literals(literals: &LiteralSet, task: &Task) -> (State, State) {
    (
        atoms_to_state(&literals.pos, task),
        atoms_to_state(&literals.neg, task),
    )
}

/// Sets the bit of every atom in `atoms` that is part of the task's fact
/// universe.
fn atoms_to_state(atoms: &[Atom], task: &Task) -> State {
    let mut state = State::new(task.num_facts());
    for (pred, args) in atoms {
        let fact = Fact {
            predicate: pred.clone(),
            args: args.clone(),
        };
        if let Some(id) = task.fact_id(&fact) {
            state.set(id, true);
        }
    }

    state
}
