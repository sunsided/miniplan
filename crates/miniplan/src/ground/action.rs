//! The grounding entry point: fact universe, initial state, goal and operators.

use pddl::{ActionDefinition, Domain, GoalDescription, Problem, StructureDef};
use pddl::{InitElement, PreconditionGoalDescription};
use rustc_hash::FxHashMap;

use crate::error::MiniplanError;
use crate::ground::cost::extract_action_cost;
use crate::ground::derived;
use crate::ground::derived::collect as collect_derived;
use crate::ground::effect::extract_effects;
use crate::ground::formula::{build_state_from_literals, walk_goal_description};
use crate::ground::types::{
    Binding, advance_odometer, extract_objects, extract_types, generate_bindings, type_to_string,
};
use crate::task::{Fact, FactId, Object, OpId, Operator, State, Task, TaskMeta};

/// Ground a PDDL domain and problem into a [`Task`].
///
/// This is the main entry point for grounding. It extracts types, objects,
/// builds a fact universe, grounds all operators, and constructs the initial
/// state and goal specification.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when
///
/// - a derived predicate is unsupported: it appears in an action effect,
///   references itself, references another derived predicate, references a
///   fluent predicate, or uses a numeric comparison in its body;
/// - the goal is unsatisfiable (its disjunctive normal form is empty);
/// - the goal or an action precondition or `when` condition uses a negated
///   non-atomic formula, `imply`, or a numeric comparison.
///
/// Returns [`MiniplanError::Unsupported`] when the domain has a PDDL+
/// `:process` or `:event`.
pub fn ground(domain: &Domain, problem: &Problem) -> Result<Task, MiniplanError> {
    reject_pddl_plus(domain)?;

    let types = extract_types(domain);
    let objects = extract_objects(domain, problem);

    let derived_rules = collect_derived(domain)?;
    let (facts, fact_index) = build_fact_universe(domain, &derived_rules, &objects);

    let num_facts = facts.len();
    let mut task = Task {
        facts,
        fact_index,
        operators: Vec::new(),
        init: State::new(num_facts),
        goal_pos: State::new(num_facts),
        goal_neg: State::new(num_facts),
        objects,
        types,
        metadata: TaskMeta {
            domain_name: domain.name().to_string(),
            problem_name: problem.name().to_string(),
            requirements: domain
                .requirements()
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        },
    };

    build_init_state(&mut task, problem);
    derived::expand_into_init_with_rules(&mut task, &derived_rules)?;
    build_goal_state(&mut task, problem)?;
    ground_actions(&mut task, domain)?;

    Ok(task)
}

/// Fails on the first PDDL+ process or event of the domain.
///
/// Both change the state on their own, so grounding only the actions would
/// plan as if they did not exist.
///
/// # Errors
///
/// Returns [`MiniplanError::Unsupported`] naming the process or event.
fn reject_pddl_plus(domain: &Domain) -> Result<(), MiniplanError> {
    for def in domain.structure().iter() {
        let unsupported = match def {
            StructureDef::Process(process) => format!("PDDL+ process `{}`", **process.symbol()),
            StructureDef::Event(event) => format!("PDDL+ event `{}`", **event.symbol()),
            _ => continue,
        };

        return Err(MiniplanError::Unsupported(unsupported));
    }

    Ok(())
}

/// Enumerates every fact that can exist: each domain predicate and each
/// derived predicate head, applied to every tuple of objects.
///
/// Argument types are deliberately ignored, so the universe is a superset of
/// the type-correct facts. Facts get dense ids in enumeration order; the map
/// is the inverse of the returned vector.
fn build_fact_universe(
    domain: &Domain,
    derived_rules: &derived::DerivedRuleSet,
    objects: &[Object],
) -> (Vec<Fact>, FxHashMap<Fact, FactId>) {
    let mut facts = Vec::new();
    let mut fact_index = FxHashMap::default();

    let predicates = domain
        .predicates()
        .iter()
        .map(|pred| (pred.predicate().to_string(), pred.variables().len()));
    let derived_heads = derived_rules
        .rules
        .iter()
        .map(|rule| (rule.head_name.clone(), rule.params.len()));

    for (name, arity) in predicates.chain(derived_heads) {
        generate_predicate_groundings(&name, arity, objects, &mut facts, &mut fact_index);
    }

    (facts, fact_index)
}

/// Adds every grounding of the predicate `pred_name` with `arity` arguments
/// drawn from `objects` to the fact universe.
///
/// Arguments vary in object order with the last argument fastest. A nullary
/// predicate yields exactly one fact; a predicate with arguments yields none
/// when there are no objects.
fn generate_predicate_groundings(
    pred_name: &str,
    arity: usize,
    objects: &[Object],
    facts: &mut Vec<Fact>,
    fact_index: &mut FxHashMap<Fact, FactId>,
) {
    let object_names: Vec<&str> = objects.iter().map(|o| o.name.as_str()).collect();
    if arity > 0 && object_names.is_empty() {
        return;
    }

    let limits = vec![object_names.len(); arity];
    let mut combo = vec![0usize; arity];
    loop {
        let fact = Fact {
            predicate: pred_name.to_owned(),
            args: combo.iter().map(|&i| object_names[i].to_owned()).collect(),
        };
        intern_fact(facts, fact_index, fact);

        if arity == 0 || !advance_odometer(&mut combo, &limits) {
            return;
        }
    }
}

/// Returns the id of `fact`, appending it to the universe first if it is new.
///
/// New facts get the next dense id, so `facts[id.0]` is always the fact that
/// `fact_index` maps to `id`.
pub(super) fn intern_fact(
    facts: &mut Vec<Fact>,
    fact_index: &mut FxHashMap<Fact, FactId>,
    fact: Fact,
) -> FactId {
    *fact_index.entry(fact).or_insert_with_key(|fact| {
        let id = FactId(facts.len());
        facts.push(fact.clone());
        id
    })
}

/// Sets the problem's `:init` atoms in `task.init`.
///
/// Only positive predicate atoms count. Numeric and object fluent
/// initialisations and atoms outside the fact universe are skipped.
fn build_init_state(task: &mut Task, problem: &Problem) {
    for elem in problem.init().iter() {
        if let InitElement::Literal(pddl::Literal::AtomicFormula(pddl::AtomicFormula::Predicate(
            pred,
        ))) = elem
        {
            let fact = Fact {
                predicate: pred.predicate().to_string(),
                args: pred
                    .values()
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
            };
            if let Some(id) = task.fact_id(&fact) {
                task.init.set(id, true);
            }
        }
    }
}

/// Sets `task.goal_pos` and `task.goal_neg` from the problem's `:goal`.
///
/// Preferences and quantified goal definitions are ignored. A disjunctive goal
/// is reduced to its first disjunct, so only that branch has to be reached.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when the goal is unsatisfiable, or when
/// [`walk_goal_description`] rejects it (negated non-atom, `imply`, numeric
/// comparison).
fn build_goal_state(task: &mut Task, problem: &Problem) -> Result<(), MiniplanError> {
    let goal_gds: Vec<GoalDescription> = problem
        .goals()
        .iter()
        .filter_map(precondition_to_goal)
        .collect();

    let combined = match goal_gds.len() {
        0 => return Ok(()),
        1 => goal_gds[0].clone(),
        _ => GoalDescription::new_and(goal_gds),
    };

    let dnf = walk_goal_description(&combined, &[])?;
    let Some(first) = dnf.first() else {
        return Err(MiniplanError::Ground(
            "goal is unsatisfiable (False)".into(),
        ));
    };

    let (pos, neg) = build_state_from_literals(first, task);
    task.goal_pos = pos;
    task.goal_neg = neg;

    Ok(())
}

/// Unwraps a plain goal definition from a precondition or goal entry.
///
/// Returns `None` for named preferences and `forall` preconditions, which the
/// grounder does not support and silently drops.
fn precondition_to_goal(pgd: &PreconditionGoalDescription) -> Option<GoalDescription> {
    match pgd {
        PreconditionGoalDescription::Preference(pddl::PreferenceGoalDescription::Goal(gd)) => {
            Some(gd.clone())
        }
        PreconditionGoalDescription::Preference(pddl::PreferenceGoalDescription::Preference(_))
        | PreconditionGoalDescription::Forall(_, _) => None,
    }
}

/// Grounds every action of the domain and appends the operators to the task.
///
/// Derived predicates (`StructureDef::Derived`) were already expanded into the
/// initial state by [`derived::expand_into_init_with_rules`] and are skipped.
///
/// # Errors
///
/// Propagates the [`MiniplanError::Ground`] errors of [`ground_action`].
fn ground_actions(task: &mut Task, domain: &Domain) -> Result<(), MiniplanError> {
    for def in domain.structure().iter() {
        if let StructureDef::Action(action) = def {
            ground_action(task, action)?;
        }
    }

    Ok(())
}

/// Grounds one action schema into operators, one per parameter binding and
/// precondition disjunct.
///
/// Bindings whose precondition is unsatisfiable produce no operator.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when [`walk_goal_description`] rejects the
/// precondition or a `when` condition (negated non-atom, `imply`, numeric
/// comparison).
fn ground_action(task: &mut Task, action: &ActionDefinition) -> Result<(), MiniplanError> {
    let action_name = action.symbol().to_string();
    let params: Vec<(String, String)> = action
        .parameters()
        .iter()
        .map(|typed_var| {
            (
                typed_var.value().to_string(),
                type_to_string(typed_var.type_()),
            )
        })
        .collect();
    let precondition = combined_precondition(action);

    let bindings_list = generate_bindings(&params, &task.objects, &task.types);
    for bindings in &bindings_list {
        let dnf = walk_goal_description(&precondition, bindings)?;
        for literals in &dnf {
            let (pre_pos, pre_neg) = build_state_from_literals(literals, task);
            let (add, del, conditional) =
                extract_effects(action.effect().as_ref(), bindings, task)?;

            let op = Operator {
                id: OpId(task.operators.len()),
                name: operator_name(&action_name, bindings),
                pre_pos,
                pre_neg,
                add,
                del,
                conditional,
                cost: extract_action_cost(action),
            };
            task.operators.push(op);
        }
    }

    Ok(())
}

/// Merges an action's precondition entries into one goal definition.
///
/// Several entries become a conjunction. An action without a usable
/// precondition gets the placeholder atom `(dummy)`; it names no fact in the
/// universe, so it adds no precondition bit.
fn combined_precondition(action: &ActionDefinition) -> GoalDescription {
    let mut pre_gds: Vec<GoalDescription> = action
        .precondition()
        .iter()
        .filter_map(precondition_to_goal)
        .collect();

    match pre_gds.len() {
        0 => GoalDescription::AtomicFormula(pddl::AtomicFormula::new_predicate(
            pddl::Predicate::from_static("dummy"),
            vec![],
        )),
        1 => pre_gds.remove(0),
        _ => GoalDescription::new_and(pre_gds),
    }
}

/// Formats a grounded operator name: `action(arg1,arg2)`, or just `action`
/// when there are no parameters.
fn operator_name(action_name: &str, bindings: &[Binding]) -> String {
    if bindings.is_empty() {
        return action_name.to_owned();
    }

    let args: Vec<&str> = bindings.iter().map(|(_, v)| v.as_str()).collect();
    format!("{}({})", action_name, args.join(","))
}
