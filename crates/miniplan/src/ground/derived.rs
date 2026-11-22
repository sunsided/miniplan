//! Derived predicates (`:derived`) with static bodies.
//!
//! Only single-layer, non-recursive rules whose bodies mention no fluent
//! (action-modified) predicate are supported. Their truth never changes during
//! search, so each rule is evaluated once against the initial state and the
//! resulting facts are added to it.

use pddl::{AtomicFormula, ConditionalEffect, EffectCondition, Effects, PrimitiveEffect};
use pddl::{DerivedPredicate, Domain, GoalDescription, Literal, StructureDef, Term, Typed};
use rustc_hash::FxHashSet;

use crate::error::MiniplanError;
use crate::ground::action::intern_fact;
use crate::ground::formula::{ground_atom, term_to_string};
use crate::ground::types::{Binding, generate_bindings, type_to_string};
use crate::task::{Fact, Object, Task, TypeHierarchy};

/// One `(:derived (head ?params) body)` rule.
#[derive(Debug, Clone)]
pub(crate) struct DerivedRule {
    /// Name of the derived predicate, e.g. `can-place`.
    pub head_name: String,
    /// Head parameters as `(variable, type name)` pairs, in declaration order.
    pub params: Vec<(String, String)>,
    /// Rule body; refers only to static predicates.
    pub body: GoalDescription,
}

/// All derived rules of a domain, validated by [`collect`].
#[derive(Debug)]
pub(crate) struct DerivedRuleSet {
    /// Rules in declaration order.
    pub rules: Vec<DerivedRule>,
}

/// Collects and validates the domain's derived predicate rules.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when a derived predicate appears in an
/// action effect, or its body references itself, another derived predicate, or
/// a fluent predicate.
pub(crate) fn collect(domain: &Domain) -> Result<DerivedRuleSet, MiniplanError> {
    let fluent_names = extract_fluent_names(domain);
    let derived_head_names: FxHashSet<String> = derived_predicates(domain)
        .map(|dp| dp.predicate().predicate().to_string())
        .collect();

    let rules = derived_predicates(domain)
        .map(|dp| validate_rule(dp, &fluent_names, &derived_head_names))
        .collect::<Result<_, _>>()?;

    Ok(DerivedRuleSet { rules })
}

/// Iterates over the `:derived` definitions of the domain in declaration order.
fn derived_predicates(domain: &Domain) -> impl Iterator<Item = &DerivedPredicate> {
    domain.structure().iter().filter_map(|def| {
        if let StructureDef::Derived(dp) = def {
            Some(dp)
        } else {
            None
        }
    })
}

/// Checks one derived predicate against the supported fragment and converts it
/// into a [`DerivedRule`].
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when the head is a fluent, or the body
/// references the head itself, another derived predicate, or a fluent.
fn validate_rule(
    dp: &DerivedPredicate,
    fluent_names: &FxHashSet<String>,
    derived_head_names: &FxHashSet<String>,
) -> Result<DerivedRule, MiniplanError> {
    let head = dp.predicate();
    let head_name = head.predicate().to_string();

    if fluent_names.contains(&head_name) {
        return Err(MiniplanError::Ground(format!(
            "derived predicate `{head_name}` appears in action effects; derived predicates cannot be fluents"
        )));
    }

    let body = dp.expression();
    let body_pred_names = collect_pred_names_in_gd(body);
    check_body_references(
        &head_name,
        &body_pred_names,
        fluent_names,
        derived_head_names,
    )?;

    Ok(DerivedRule {
        params: typed_params(head.variables()),
        head_name,
        body: body.clone(),
    })
}

/// Rejects bodies that reference the head itself, another derived predicate,
/// or a fluent predicate, checked in that order.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] naming the first offending predicate.
fn check_body_references(
    head_name: &str,
    body_pred_names: &FxHashSet<String>,
    fluent_names: &FxHashSet<String>,
    derived_head_names: &FxHashSet<String>,
) -> Result<(), MiniplanError> {
    if body_pred_names.contains(head_name) {
        return Err(MiniplanError::Ground(format!(
            "derived predicate `{head_name}` references itself; recursive derived predicates are not supported"
        )));
    }

    if let Some(dp_name) = body_pred_names
        .iter()
        .find(|name| derived_head_names.contains(*name))
    {
        return Err(MiniplanError::Ground(format!(
            "derived predicate `{head_name}` references another derived predicate `{dp_name}`; multi-layer derived predicates are not yet supported"
        )));
    }

    if let Some(fp) = body_pred_names
        .iter()
        .find(|name| fluent_names.contains(*name))
    {
        return Err(MiniplanError::Ground(format!(
            "derived predicate `{head_name}` references fluent predicate `{fp}`; only static-body derived predicates are supported"
        )));
    }

    Ok(())
}

/// Converts typed variables into `(variable, type name)` pairs.
fn typed_params(vars: &[Typed<pddl::Variable>]) -> Vec<(String, String)> {
    vars.iter()
        .map(|tv| (tv.value().to_string(), type_to_string(tv.type_())))
        .collect()
}

/// Returns the names of all predicates that some action adds or deletes.
fn extract_fluent_names(domain: &Domain) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    for def in domain.structure().iter() {
        if let StructureDef::Action(action) = def {
            collect_pred_names_in_effects(action.effect().as_ref(), &mut names);
        }
    }

    names
}

/// Adds the predicate names touched by `effects` to `names`.
fn collect_pred_names_in_effects(effects: Option<&Effects>, names: &mut FxHashSet<String>) {
    let Some(effects) = effects else {
        return;
    };

    for ce in effects {
        collect_pred_names_in_conditional_effect(ce, names);
    }
}

/// Adds the predicate names touched by one effect, including nested `forall`
/// and `when` bodies, to `names`.
fn collect_pred_names_in_conditional_effect(ce: &ConditionalEffect, names: &mut FxHashSet<String>) {
    match ce {
        ConditionalEffect::Effect(pe) => {
            collect_pred_names_in_primitive_effect(pe, names);
        }
        ConditionalEffect::Forall(forall) => {
            for nested in &forall.effects {
                collect_pred_names_in_conditional_effect(nested, names);
            }
        }
        ConditionalEffect::When(when) => match &when.effect {
            EffectCondition::Single(pe) => {
                collect_pred_names_in_primitive_effect(pe, names);
            }
            EffectCondition::All(pes) => {
                for pe in pes {
                    collect_pred_names_in_primitive_effect(pe, names);
                }
            }
        },
    }
}

/// Adds the predicate name of an add or delete effect to `names`; other
/// effects are ignored.
fn collect_pred_names_in_primitive_effect(pe: &PrimitiveEffect, names: &mut FxHashSet<String>) {
    match pe {
        PrimitiveEffect::AtomicFormula(AtomicFormula::Predicate(pred))
        | PrimitiveEffect::NotAtomicFormula(AtomicFormula::Predicate(pred)) => {
            names.insert(pred.predicate().to_string());
        }
        _ => {}
    }
}

/// Returns the names of all predicates mentioned anywhere in `gd`.
fn collect_pred_names_in_gd(gd: &GoalDescription) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    collect_pred_names_in_gd_inner(gd, &mut names);
    names
}

/// Recursive worker of [`collect_pred_names_in_gd`].
fn collect_pred_names_in_gd_inner(gd: &GoalDescription, names: &mut FxHashSet<String>) {
    match gd {
        GoalDescription::AtomicFormula(af)
        | GoalDescription::Literal(Literal::AtomicFormula(af) | Literal::NotAtomicFormula(af)) => {
            if let AtomicFormula::Predicate(pred) = af {
                names.insert(pred.predicate().to_string());
            }
        }
        GoalDescription::And(gds) | GoalDescription::Or(gds) => {
            for g in gds {
                collect_pred_names_in_gd_inner(g, names);
            }
        }
        GoalDescription::Not(inner) => {
            collect_pred_names_in_gd_inner(inner, names);
        }
        GoalDescription::Imply(a, b) => {
            collect_pred_names_in_gd_inner(a, names);
            collect_pred_names_in_gd_inner(b, names);
        }
        GoalDescription::Exists(_, body) | GoalDescription::ForAll(_, body) => {
            collect_pred_names_in_gd_inner(body, names);
        }
        GoalDescription::FluentComparison(_) => {}
    }
}

/// Evaluates every rule against the initial state and adds the derived facts
/// to `task.init` (and to the fact universe, should one be missing).
///
/// All rules are evaluated against the original initial facts, so derived
/// facts never feed into other rules.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when a rule body contains a numeric
/// comparison.
pub(crate) fn expand_into_init_with_rules(
    task: &mut Task,
    rule_set: &DerivedRuleSet,
) -> Result<(), MiniplanError> {
    if rule_set.rules.is_empty() {
        return Ok(());
    }

    let init_facts: FxHashSet<Fact> = task
        .init
        .0
        .ones()
        .filter_map(|id| task.facts.get(id).cloned())
        .collect();

    for rule in &rule_set.rules {
        let bindings_list = generate_bindings(&rule.params, &task.objects, &task.types);
        for bindings in &bindings_list {
            if eval_gd(
                &rule.body,
                bindings,
                &init_facts,
                &task.objects,
                &task.types,
            )? {
                let fact = Fact {
                    predicate: rule.head_name.clone(),
                    args: bindings.iter().map(|(_, v)| v.clone()).collect(),
                };
                add_derived_fact(task, fact);
            }
        }
    }

    Ok(())
}

/// Marks `fact` as true in the initial state, interning it first if needed.
fn add_derived_fact(task: &mut Task, fact: Fact) {
    let id = intern_fact(&mut task.facts, &mut task.fact_index, fact);
    task.init.set(id, true);
}

/// Evaluates a goal definition under `bindings` against a fixed fact set.
///
/// Unlike [`walk_goal_description`](crate::ground::formula::walk_goal_description),
/// quantifiers are expanded over the typed `objects`, and `imply` is supported.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when `gd` contains a numeric comparison.
pub(crate) fn eval_gd(
    gd: &GoalDescription,
    bindings: &[Binding],
    facts: &FxHashSet<Fact>,
    objects: &[Object],
    types: &TypeHierarchy,
) -> Result<bool, MiniplanError> {
    let eval = |g: &GoalDescription| eval_gd(g, bindings, facts, objects, types);

    match gd {
        GoalDescription::AtomicFormula(af) => Ok(eval_atomic_formula(af, bindings, facts)),
        GoalDescription::Literal(lit) => Ok(eval_literal(lit, bindings, facts)),
        GoalDescription::And(gds) => {
            for g in gds {
                if !eval(g)? {
                    return Ok(false);
                }
            }

            Ok(true)
        }
        GoalDescription::Or(gds) => {
            for g in gds {
                if eval(g)? {
                    return Ok(true);
                }
            }

            Ok(false)
        }
        GoalDescription::Not(inner) => Ok(!eval(inner)?),
        GoalDescription::Imply(a, b) => {
            if !eval(a)? {
                return Ok(true);
            }

            eval(b)
        }
        GoalDescription::Exists(vars, body) => eval_quantified(
            Quantifier::Exists,
            vars,
            body,
            bindings,
            facts,
            objects,
            types,
        ),
        GoalDescription::ForAll(vars, body) => eval_quantified(
            Quantifier::ForAll,
            vars,
            body,
            bindings,
            facts,
            objects,
            types,
        ),
        GoalDescription::FluentComparison(_) => Err(MiniplanError::Ground(
            "numeric comparisons in derived predicate bodies are not supported".into(),
        )),
    }
}

/// The two quantifiers of PDDL goal definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quantifier {
    /// `exists`: true if the body holds for at least one assignment.
    Exists,
    /// `forall`: true if the body holds for every assignment.
    ForAll,
}

/// Evaluates `(exists vars body)` or `(forall vars body)` by trying every
/// assignment of objects to `vars`, short-circuiting on the first decisive one.
///
/// The quantified variables shadow outer `bindings` of the same name. With no
/// assignment at all, `exists` is false and `forall` is true.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when `body` contains a numeric
/// comparison.
fn eval_quantified(
    quantifier: Quantifier,
    vars: &[Typed<pddl::Variable>],
    body: &GoalDescription,
    bindings: &[Binding],
    facts: &FxHashSet<Fact>,
    objects: &[Object],
    types: &TypeHierarchy,
) -> Result<bool, MiniplanError> {
    let decisive = quantifier == Quantifier::Exists;

    for inner_bindings in generate_bindings(&typed_params(vars), objects, types) {
        let mut combined = inner_bindings;
        combined.extend_from_slice(bindings);
        if eval_gd(body, &combined, facts, objects, types)? == decisive {
            return Ok(decisive);
        }
    }

    Ok(!decisive)
}

/// Evaluates an atomic formula: equality compares the grounded names,
/// a predicate is looked up in `facts`.
fn eval_atomic_formula(
    af: &AtomicFormula<Term>,
    bindings: &[Binding],
    facts: &FxHashSet<Fact>,
) -> bool {
    match af {
        AtomicFormula::Equality(eq) => {
            term_to_string(eq.first(), bindings) == term_to_string(eq.second(), bindings)
        }
        AtomicFormula::Predicate(pred) => {
            let (predicate, args) = ground_atom(pred, bindings);
            facts.contains(&Fact { predicate, args })
        }
    }
}

/// Evaluates a possibly negated atomic formula.
fn eval_literal(lit: &Literal<Term>, bindings: &[Binding], facts: &FxHashSet<Fact>) -> bool {
    match lit {
        Literal::AtomicFormula(af) => eval_atomic_formula(af, bindings, facts),
        Literal::NotAtomicFormula(af) => !eval_atomic_formula(af, bindings, facts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pddl::{Domain, Parser, Problem};

    /// Domain whose `can-place` rule combines `or`, `exists` and static facts.
    const MINI_CAN_DOMAIN: &str = r"
(define (domain mini-can)
  (:requirements :strips :typing :derived-predicates)
  (:types disc - location)
  (:predicates (smaller ?d1 - disc ?d2 - disc) (on ?d - disc ?x - location) (is-peg ?x - location) (done))
  (:derived (can-place ?d - disc ?x - location)
    (or
      (is-peg ?x)
      (exists (?od - disc)
        (and
          (on ?d ?od)
          (smaller ?d ?od)
        )
      )
    )
  )
  (:action move
    :parameters (?d - disc ?from ?to - location)
    :precondition (can-place ?d ?to)
    :effect (done)
  )
)
";

    /// Problem for [`MINI_CAN_DOMAIN`]: `d1` sits on the larger `d2`.
    const MINI_CAN_PROBLEM: &str = r"
(define (problem mini-can-1)
  (:domain mini-can)
  (:objects d1 d2 - disc peg1 peg2 - location)
  (:init
    (on d1 d2)
    (smaller d1 d2)
    (is-peg peg1)
    (is-peg peg2)
  )
  (:goal (done))
)
";

    /// Parses a domain, panicking on invalid input.
    fn parse_domain(src: &str) -> Domain {
        Domain::from_str(src).expect("domain should parse")
    }

    /// Parses a problem, panicking on invalid input.
    fn parse_problem(src: &str) -> Problem {
        Problem::from_str(src).expect("problem should parse")
    }

    /// Parses and grounds a domain/problem pair, panicking on any error.
    fn ground_task(domain_src: &str, problem_src: &str) -> Task {
        let domain = parse_domain(domain_src);
        let problem = parse_problem(problem_src);
        crate::ground::ground(&domain, &problem).expect("grounding should succeed")
    }

    /// Builds the fact `predicate(args...)`.
    fn fact(predicate: &str, args: &[&str]) -> Fact {
        Fact {
            predicate: predicate.to_owned(),
            args: args.iter().map(|&a| a.to_owned()).collect(),
        }
    }

    /// Whether `fact` exists in the universe and holds in the initial state.
    fn holds_in_init(task: &Task, fact: &Fact) -> bool {
        task.fact_id(fact).is_some_and(|id| task.init.contains(id))
    }

    /// A static derived rule adds its facts to the initial state and enables
    /// the operators that depend on it.
    #[test]
    fn derived_static_expands_to_facts() {
        let domain = r"
(define (domain test-derived)
  (:requirements :strips :typing :derived-predicates)
  (:types obj - object)
  (:predicates (color ?x - obj) (done))
  (:derived (red ?x - obj) (color ?x))
  (:action check
    :parameters (?x - obj)
    :precondition (red ?x)
    :effect (done)
  )
)
";
        let problem = r"
(define (problem test-derived-1)
  (:domain test-derived)
  (:objects a - obj)
  (:init (color a))
  (:goal (done))
)
";
        let task = ground_task(domain, problem);

        let red_a = fact("red", &["a"]);
        let id = task.fact_id(&red_a).expect("red(a) fact should exist");
        assert!(task.init.contains(id), "red(a) should be true in init");

        let check_op = task
            .operators
            .iter()
            .find(|op| op.name.starts_with("check"))
            .expect("check operator should exist");
        assert!(
            task.init.applicable(check_op),
            "check(a) should be applicable in init"
        );
    }

    /// Quantified bodies are expanded over typed objects: `exists` finds the
    /// disc below `d1`, and no fact is derived where no disjunct holds.
    #[test]
    fn derived_with_exists_and_equality() {
        let task = ground_task(MINI_CAN_DOMAIN, MINI_CAN_PROBLEM);

        assert!(
            holds_in_init(&task, &fact("can-place", &["d1", "peg1"])),
            "can-place(d1,peg1) should be true"
        );
        assert!(
            holds_in_init(&task, &fact("can-place", &["d1", "d2"])),
            "can-place(d1,d2) should be true"
        );
        assert!(
            !holds_in_init(&task, &fact("can-place", &["d2", "d1"])),
            "can-place(d2,d1) should be false"
        );
    }

    /// A rule whose body reads an action-modified predicate is rejected.
    #[test]
    fn derived_rejects_fluent_body() {
        let domain = r"
(define (domain fluent-body)
  (:requirements :strips :typing :derived-predicates)
  (:predicates (base ?x - obj) (fluent ?x - obj))
  (:derived (derived ?x - obj) (fluent ?x))
  (:action act
    :parameters (?x - obj)
    :precondition ()
    :effect (and (fluent ?x))
  )
)
";
        let problem = r"
(define (problem fluent-body-1)
  (:domain fluent-body)
  (:objects a - obj)
  (:init (base a))
  (:goal (base a))
)
";
        let domain = parse_domain(domain);
        let problem = parse_problem(problem);
        let err = crate::ground::ground(&domain, &problem).expect_err("should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("fluent"),
            "error should mention the fluent predicate, got: {msg}"
        );
    }

    /// A rule whose body reads another derived predicate is rejected.
    #[test]
    fn derived_rejects_recursive_reference() {
        let domain = r"
(define (domain recursive-derived)
  (:requirements :strips :typing :derived-predicates)
  (:predicates (base ?x - obj))
  (:derived (intermediate ?x - obj) (base ?x))
  (:derived (top ?x - obj) (intermediate ?x))
)
";
        let problem = r"
(define (problem recursive-1)
  (:domain recursive-derived)
  (:objects a - obj)
  (:init (base a))
  (:goal (base a))
)
";
        let domain = parse_domain(domain);
        let problem = parse_problem(problem);
        let err = crate::ground::ground(&domain, &problem).expect_err("should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("multi-layer") || msg.contains("intermediate"),
            "error should mention multi-layer or the derived predicate, got: {msg}"
        );
    }
}
