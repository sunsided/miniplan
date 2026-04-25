use pddl::{AtomicFormula, Type};
use pddl::{Domain, GoalDefinition, Literal, StructureDef, Term};
use rustc_hash::FxHashSet;

use crate::error::MiniplanError;
use crate::ground::formula::term_to_string;
use crate::ground::types::objects_of_type;
use crate::task::{Fact, FactId, Object, Task, TypeHierarchy};

#[derive(Debug, Clone)]
pub(crate) struct DerivedRule {
    pub head_name: String,
    pub params: Vec<(String, String)>,
    pub body: GoalDefinition,
}

#[derive(Debug)]
pub(crate) struct DerivedRuleSet {
    pub rules: Vec<DerivedRule>,
    #[allow(dead_code)]
    pub all_static: bool,
}

pub(crate) fn collect(domain: &Domain) -> Result<DerivedRuleSet, MiniplanError> {
    let mut rules = Vec::new();

    let fluent_names = extract_fluent_names(domain);

    // First pass: collect all derived head names
    let mut derived_head_names: FxHashSet<String> = FxHashSet::default();
    for def in domain.structure().iter() {
        if let StructureDef::Derived(dp) = def {
            let head_name = dp.predicate().predicate().to_string();
            derived_head_names.insert(head_name);
        }
    }

    for def in domain.structure().iter() {
        if let StructureDef::Derived(dp) = def {
            let head = dp.predicate();
            let head_name = head.predicate().to_string();

            if fluent_names.contains(&head_name) {
                return Err(MiniplanError::Ground(format!(
                    "derived predicate `{}` appears in action effects; derived predicates cannot be fluents",
                    head_name
                )));
            }

            let params: Vec<(String, String)> = head
                .variables()
                .iter()
                .map(|tv| {
                    let var = tv.value().to_string();
                    let type_name = type_to_string(tv.type_());
                    (var, type_name)
                })
                .collect();

            let body = dp.expression();
            let body_pred_names = collect_pred_names_in_gd(body);

            for bp in &body_pred_names {
                if bp == &head_name {
                    return Err(MiniplanError::Ground(format!(
                        "derived predicate `{}` references itself; recursive derived predicates are not supported",
                        head_name
                    )));
                }
            }

            for dp_name in &body_pred_names {
                if derived_head_names.contains(dp_name) && dp_name != &head_name {
                    return Err(MiniplanError::Ground(format!(
                        "derived predicate `{}` references another derived predicate `{}`; multi-layer derived predicates are not yet supported",
                        head_name, dp_name
                    )));
                }
            }

            for fp in &body_pred_names {
                if fluent_names.contains(fp) {
                    return Err(MiniplanError::Ground(format!(
                        "derived predicate `{}` references fluent predicate `{}`; only static-body derived predicates are supported",
                        head_name, fp
                    )));
                }
            }

            rules.push(DerivedRule {
                head_name,
                params,
                body: body.clone(),
            });
        }
    }

    let all_static = true;

    Ok(DerivedRuleSet { rules, all_static })
}

fn extract_fluent_names(domain: &Domain) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    for def in domain.structure().iter() {
        if let StructureDef::Action(action) = def {
            collect_pred_names_in_effects(action.effect(), &mut names);
        }
    }
    names
}

fn collect_pred_names_in_effects(effects: &Option<pddl::Effects>, names: &mut FxHashSet<String>) {
    let effects = match effects {
        Some(e) => e,
        None => return,
    };
    for ce in effects.iter() {
        collect_pred_names_in_conditional_effect(ce, names);
    }
}

fn collect_pred_names_in_conditional_effect(
    ce: &pddl::ConditionalEffect,
    names: &mut FxHashSet<String>,
) {
    match ce {
        pddl::ConditionalEffect::Effect(pe) => {
            collect_pred_names_in_primitive_effect(pe, names);
        }
        pddl::ConditionalEffect::Forall(forall) => {
            for ce2 in forall.effects.iter() {
                collect_pred_names_in_conditional_effect(ce2, names);
            }
        }
        pddl::ConditionalEffect::When(when) => match &when.effect {
            pddl::EffectCondition::Single(pe) => {
                collect_pred_names_in_primitive_effect(pe, names);
            }
            pddl::EffectCondition::All(pes) => {
                for pe in pes {
                    collect_pred_names_in_primitive_effect(pe, names);
                }
            }
        },
    }
}

fn collect_pred_names_in_primitive_effect(
    pe: &pddl::PrimitiveEffect,
    names: &mut FxHashSet<String>,
) {
    match pe {
        pddl::PrimitiveEffect::AtomicFormula(AtomicFormula::Predicate(pred))
        | pddl::PrimitiveEffect::NotAtomicFormula(AtomicFormula::Predicate(pred)) => {
            names.insert(pred.predicate().to_string());
        }
        _ => {}
    }
}

fn collect_pred_names_in_gd(gd: &GoalDefinition) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    collect_pred_names_in_gd_inner(gd, &mut names);
    names
}

fn collect_pred_names_in_gd_inner(gd: &GoalDefinition, names: &mut FxHashSet<String>) {
    match gd {
        GoalDefinition::AtomicFormula(af) => {
            if let AtomicFormula::Predicate(pred) = af {
                names.insert(pred.predicate().to_string());
            }
        }
        GoalDefinition::Literal(lit) => {
            let af: &AtomicFormula<Term> = match lit {
                Literal::AtomicFormula(af) => af,
                Literal::NotAtomicFormula(af) => af,
            };
            if let AtomicFormula::Predicate(pred) = af {
                names.insert(pred.predicate().to_string());
            }
        }
        GoalDefinition::And(gds) | GoalDefinition::Or(gds) => {
            for g in gds {
                collect_pred_names_in_gd_inner(g, names);
            }
        }
        GoalDefinition::Not(inner) => {
            collect_pred_names_in_gd_inner(inner, names);
        }
        GoalDefinition::Imply(a, b) => {
            collect_pred_names_in_gd_inner(a, names);
            collect_pred_names_in_gd_inner(b, names);
        }
        GoalDefinition::Exists(_, body) | GoalDefinition::ForAll(_, body) => {
            collect_pred_names_in_gd_inner(body, names);
        }
        GoalDefinition::FluentComparison(_) => {}
    }
}

fn type_to_string(t: &Type) -> String {
    match t {
        Type::Exactly(pt) => pt.to_string(),
        Type::EitherOf(pts) => pts
            .first()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "object".to_owned()),
    }
}

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
        let arity = rule.params.len();
        if arity == 0 {
            let result = eval_gd(&rule.body, &[], &init_facts, &task.objects, &task.types)?;
            if result {
                let fact = Fact {
                    predicate: rule.head_name.clone(),
                    args: vec![],
                };
                add_derived_fact(task, &fact);
            }
        } else {
            let bindings_list =
                generate_bindings_for_params(&rule.params, &task.objects, &task.types)?;
            for bindings in &bindings_list {
                let result = eval_gd(
                    &rule.body,
                    bindings,
                    &init_facts,
                    &task.objects,
                    &task.types,
                )?;
                if result {
                    let args: Vec<String> = bindings.iter().map(|(_, v)| v.clone()).collect();
                    let fact = Fact {
                        predicate: rule.head_name.clone(),
                        args,
                    };
                    add_derived_fact(task, &fact);
                }
            }
        }
    }

    Ok(())
}

fn add_derived_fact(task: &mut Task, fact: &Fact) {
    #[allow(clippy::map_entry)]
    if !task.fact_index.contains_key(fact) {
        let id = FactId(task.facts.len());
        task.facts.push(fact.clone());
        task.fact_index.insert(fact.clone(), id);
        task.init.set(id, true);
    } else {
        let id = *task.fact_index.get(fact).unwrap();
        task.init.set(id, true);
    }
}

fn generate_bindings_for_params(
    params: &[(String, String)],
    objects: &[Object],
    types: &TypeHierarchy,
) -> Result<Vec<Vec<(String, String)>>, MiniplanError> {
    if params.is_empty() {
        return Ok(vec![vec![]]);
    }

    let param_objects: Vec<Vec<&str>> = params
        .iter()
        .map(|(_, sort)| objects_of_type(objects, sort, types))
        .collect();

    let mut results = Vec::new();
    let mut indices = vec![0usize; params.len()];
    let counts: Vec<usize> = param_objects.iter().map(|v| v.len()).collect();

    if counts.contains(&0) {
        return Ok(results);
    }

    loop {
        let binding: Vec<(String, String)> = params
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (name.clone(), param_objects[i][indices[i]].to_owned()))
            .collect();
        results.push(binding);

        let mut idx = params.len() - 1;
        loop {
            indices[idx] += 1;
            if indices[idx] < counts[idx] {
                break;
            }
            indices[idx] = 0;
            if idx == 0 {
                return Ok(results);
            }
            idx -= 1;
        }
    }
}

pub(crate) fn eval_gd(
    gd: &GoalDefinition,
    bindings: &[(String, String)],
    facts: &FxHashSet<Fact>,
    objects: &[Object],
    types: &TypeHierarchy,
) -> Result<bool, MiniplanError> {
    match gd {
        GoalDefinition::AtomicFormula(af) => eval_atomic_formula(af, bindings, facts),
        GoalDefinition::Literal(lit) => eval_literal(lit, bindings, facts),
        GoalDefinition::And(gds) => {
            for g in gds {
                if !eval_gd(g, bindings, facts, objects, types)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        GoalDefinition::Or(gds) => {
            for g in gds {
                if eval_gd(g, bindings, facts, objects, types)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        GoalDefinition::Not(inner) => {
            let result = eval_gd(inner, bindings, facts, objects, types)?;
            Ok(!result)
        }
        GoalDefinition::Imply(a, b) => {
            let a_val = eval_gd(a, bindings, facts, objects, types)?;
            if !a_val {
                return Ok(true);
            }
            eval_gd(b, bindings, facts, objects, types)
        }
        GoalDefinition::Exists(vars, body) => {
            let typed_params: Vec<(String, String)> = vars
                .iter()
                .map(|tv| {
                    let var = tv.value().to_string();
                    let type_name = type_to_string(tv.type_());
                    (var, type_name)
                })
                .collect();
            let bindings_list = generate_bindings_for_params(&typed_params, objects, types)?;
            for ext_bindings in &bindings_list {
                let mut combined = ext_bindings.clone();
                combined.extend(bindings.to_vec());
                if eval_gd(body, &combined, facts, objects, types)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        GoalDefinition::ForAll(vars, body) => {
            let typed_params: Vec<(String, String)> = vars
                .iter()
                .map(|tv| {
                    let var = tv.value().to_string();
                    let type_name = type_to_string(tv.type_());
                    (var, type_name)
                })
                .collect();
            let bindings_list = generate_bindings_for_params(&typed_params, objects, types)?;
            for ext_bindings in &bindings_list {
                let mut combined = ext_bindings.clone();
                combined.extend(bindings.to_vec());
                if !eval_gd(body, &combined, facts, objects, types)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        GoalDefinition::FluentComparison(_) => Err(MiniplanError::Ground(
            "numeric comparisons in derived predicate bodies are not supported".into(),
        )),
    }
}

fn eval_atomic_formula(
    af: &AtomicFormula<Term>,
    bindings: &[(String, String)],
    facts: &FxHashSet<Fact>,
) -> Result<bool, MiniplanError> {
    match af {
        AtomicFormula::Equality(eq) => {
            let first = term_to_string(eq.first(), bindings);
            let second = term_to_string(eq.second(), bindings);
            Ok(first == second)
        }
        AtomicFormula::Predicate(pred) => {
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
            Ok(facts.contains(&fact))
        }
    }
}

fn eval_literal(
    lit: &Literal<Term>,
    bindings: &[(String, String)],
    facts: &FxHashSet<Fact>,
) -> Result<bool, MiniplanError> {
    if lit.is_negated() {
        if let Literal::NotAtomicFormula(af) = lit {
            let result = eval_atomic_formula(af, bindings, facts)?;
            Ok(!result)
        } else {
            eval_literal_inner(lit, bindings, facts)
        }
    } else {
        if let Literal::AtomicFormula(af) = lit {
            eval_atomic_formula(af, bindings, facts)
        } else {
            eval_literal_inner(lit, bindings, facts)
        }
    }
}

fn eval_literal_inner(
    lit: &Literal<Term>,
    bindings: &[(String, String)],
    facts: &FxHashSet<Fact>,
) -> Result<bool, MiniplanError> {
    let af: &AtomicFormula<Term> = match lit {
        Literal::AtomicFormula(af) => af,
        Literal::NotAtomicFormula(af) => af,
    };
    let result = eval_atomic_formula(af, bindings, facts)?;
    if lit.is_negated() {
        Ok(!result)
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pddl::{Domain, Parser, Problem};

    fn parse_domain(src: &str) -> Domain {
        let (_, domain) = Domain::parse_span(src.into()).expect("domain should parse");
        domain
    }

    fn parse_problem(src: &str) -> Problem {
        let (_, problem) = Problem::parse_span(src.into()).expect("problem should parse");
        problem
    }

    fn ground_task(domain_src: &str, problem_src: &str) -> Task {
        let domain = parse_domain(domain_src);
        let problem = parse_problem(problem_src);
        crate::ground::ground(&domain, &problem).expect("grounding should succeed")
    }

    #[test]
    fn derived_static_expands_to_facts() {
        let domain = r#"
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
"#;
        let problem = r#"
(define (problem test-derived-1)
  (:domain test-derived)
  (:objects a - obj)
  (:init (color a))
  (:goal (done))
)
"#;
        let task = ground_task(domain, problem);

        let red_a = Fact {
            predicate: "red".to_owned(),
            args: vec!["a".to_owned()],
        };
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

    #[test]
    fn derived_with_exists_and_equality() {
        let domain = r#"
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
"#;

        let problem = r#"
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
"#;
        let task = ground_task(domain, problem);

        let can_place_d1_peg1 = Fact {
            predicate: "can-place".to_owned(),
            args: vec!["d1".to_owned(), "peg1".to_owned()],
        };
        let can_place_d1_d2 = Fact {
            predicate: "can-place".to_owned(),
            args: vec!["d1".to_owned(), "d2".to_owned()],
        };
        let can_place_d2_d1 = Fact {
            predicate: "can-place".to_owned(),
            args: vec!["d2".to_owned(), "d1".to_owned()],
        };

        let id1 = task.fact_id(&can_place_d1_peg1);
        assert!(
            id1.is_some() && task.init.contains(id1.unwrap()),
            "can-place(d1,peg1) should be true"
        );

        let id2 = task.fact_id(&can_place_d1_d2);
        assert!(
            id2.is_some() && task.init.contains(id2.unwrap()),
            "can-place(d1,d2) should be true"
        );

        let id3 = task.fact_id(&can_place_d2_d1);
        assert!(
            id3.is_none_or(|id| !task.init.contains(id)),
            "can-place(d2,d1) should be false"
        );
    }

    #[test]
    fn derived_rejects_fluent_body() {
        let domain = r#"
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
"#;
        let problem = r#"
(define (problem fluent-body-1)
  (:domain fluent-body)
  (:objects a - obj)
  (:init (base a))
  (:goal (base a))
)
"#;
        let domain = parse_domain(domain);
        let problem = parse_problem(problem);
        let err = crate::ground::ground(&domain, &problem).expect_err("should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("fluent"),
            "error should mention the fluent predicate, got: {}",
            msg
        );
    }

    #[test]
    fn derived_rejects_recursive_reference() {
        let domain = r#"
(define (domain recursive-derived)
  (:requirements :strips :typing :derived-predicates)
  (:predicates (base ?x - obj))
  (:derived (intermediate ?x - obj) (base ?x))
  (:derived (top ?x - obj) (intermediate ?x))
)
"#;
        let problem = r#"
(define (problem recursive-1)
  (:domain recursive-derived)
  (:objects a - obj)
  (:init (base a))
  (:goal (base a))
)
"#;
        let domain = parse_domain(domain);
        let problem = parse_problem(problem);
        let err = crate::ground::ground(&domain, &problem).expect_err("should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("multi-layer") || msg.contains("intermediate"),
            "error should mention multi-layer or the derived predicate, got: {}",
            msg
        );
    }
}
