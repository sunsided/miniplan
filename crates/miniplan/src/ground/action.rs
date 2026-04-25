use pddl::{ActionDefinition, Domain, GoalDefinition, Problem, StructureDef};
use pddl::{InitElement, PreconditionGoalDefinition};

use crate::error::MiniplanError;
use crate::ground::cost::extract_action_cost;
use crate::ground::derived;
use crate::ground::derived::collect as collect_derived;
use crate::ground::effect::extract_effects;
use crate::ground::formula::{build_state_from_literals, walk_goal_definition};
use crate::ground::types::{extract_objects, extract_types, objects_of_type};
use crate::task::{Fact, FactId, Object, OpId, Operator, State, Task, TaskMeta, TypeHierarchy};

/// Ground a PDDL domain and problem into a [`Task`](crate::task::Task).
///
/// This is the main entry point for grounding. It extracts types, objects,
/// builds a fact universe, grounds all operators, and constructs the initial
/// state and goal specification.
pub fn ground(domain: &Domain, problem: &Problem) -> Result<Task, MiniplanError> {
    let types = extract_types(domain)?;
    let objects = extract_objects(domain, problem, &types)?;

    let (facts, fact_index) = build_fact_universe(domain, &objects)?;

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
                .map(|r| r.to_string())
                .collect(),
        },
    };

    build_init_state(&mut task, problem)?;
    derived::expand_into_init(&mut task, domain)?;
    build_goal_state(&mut task, problem)?;
    ground_actions(&mut task, domain)?;

    Ok(task)
}

fn build_fact_universe(
    domain: &Domain,
    objects: &[Object],
) -> Result<(Vec<Fact>, rustc_hash::FxHashMap<Fact, FactId>), MiniplanError> {
    let mut facts = Vec::new();
    let mut fact_index = rustc_hash::FxHashMap::default();

    for pred in domain.predicates().iter() {
        let pred_name = pred.predicate().to_string();
        let arity = pred.variables().len();
        if arity == 0 {
            let fact = Fact {
                predicate: pred_name.clone(),
                args: Vec::new(),
            };
            #[allow(clippy::map_entry)]
            if !fact_index.contains_key(&fact) {
                let id = FactId(facts.len());
                facts.push(fact.clone());
                fact_index.insert(fact, id);
            }
        } else {
            generate_predicate_groundings(&pred_name, arity, objects, &mut facts, &mut fact_index);
        }
    }

    let derived_rules = collect_derived(domain)?;
    for rule in &derived_rules.rules {
        let arity = rule.params.len();
        if arity == 0 {
            let fact = Fact {
                predicate: rule.head_name.clone(),
                args: Vec::new(),
            };
            #[allow(clippy::map_entry)]
            if !fact_index.contains_key(&fact) {
                let id = FactId(facts.len());
                facts.push(fact.clone());
                fact_index.insert(fact, id);
            }
        } else {
            generate_predicate_groundings(
                &rule.head_name,
                arity,
                objects,
                &mut facts,
                &mut fact_index,
            );
        }
    }

    Ok((facts, fact_index))
}

fn generate_predicate_groundings(
    pred_name: &str,
    arity: usize,
    objects: &[Object],
    facts: &mut Vec<Fact>,
    fact_index: &mut rustc_hash::FxHashMap<Fact, FactId>,
) {
    let object_names: Vec<&str> = objects.iter().map(|o| o.name.as_str()).collect();
    let mut combo = vec![0usize; arity];
    let n = object_names.len();

    if n == 0 {
        return;
    }

    loop {
        let args: Vec<String> = combo.iter().map(|&i| object_names[i].to_owned()).collect();
        let fact = Fact {
            predicate: pred_name.to_owned(),
            args,
        };
        #[allow(clippy::map_entry)]
        if !fact_index.contains_key(&fact) {
            let id = FactId(facts.len());
            facts.push(fact.clone());
            fact_index.insert(fact, id);
        }

        let mut idx = arity - 1;
        loop {
            combo[idx] += 1;
            if combo[idx] < n {
                break;
            }
            combo[idx] = 0;
            if idx == 0 {
                return;
            }
            idx -= 1;
        }
    }
}

fn build_init_state(task: &mut Task, problem: &Problem) -> Result<(), MiniplanError> {
    for elem in problem.init().iter() {
        if let InitElement::Literal(pddl::Literal::AtomicFormula(pddl::AtomicFormula::Predicate(
            pred,
        ))) = elem
        {
            let name = pred.predicate().to_string();
            let args: Vec<String> = pred.values().iter().map(|n| n.to_string()).collect();
            let fact = Fact {
                predicate: name,
                args,
            };
            if let Some(id) = task.fact_id(&fact) {
                task.init.set(id, true);
            }
        }
    }
    Ok(())
}

fn build_goal_state(task: &mut Task, problem: &Problem) -> Result<(), MiniplanError> {
    let goals = problem.goals();
    if goals.is_empty() {
        return Ok(());
    }

    // Convert PreconditionGoalDefinitions to GoalDefinitions
    let goal_gds: Vec<GoalDefinition> = goals.iter().filter_map(precondition_to_goal).collect();

    if goal_gds.is_empty() {
        return Ok(());
    }

    let combined = if goal_gds.len() == 1 {
        goal_gds[0].clone()
    } else {
        GoalDefinition::new_and(goal_gds)
    };

    let dnf = walk_goal_definition(&combined, &[])?;
    if dnf.is_empty() {
        return Err(MiniplanError::Ground(
            "goal is unsatisfiable (False)".into(),
        ));
    }

    let (pos, neg) = build_state_from_literals(&dnf[0], task)?;
    task.goal_pos = pos;
    task.goal_neg = neg;

    Ok(())
}

fn precondition_to_goal(pgd: &PreconditionGoalDefinition) -> Option<GoalDefinition> {
    match pgd {
        PreconditionGoalDefinition::Preference(pref_gd) => match pref_gd {
            pddl::PreferenceGoalDefinition::Goal(gd) => Some(gd.clone()),
            pddl::PreferenceGoalDefinition::Preference(_) => None,
        },
        PreconditionGoalDefinition::Forall(_, _) => None,
    }
}

fn ground_actions(task: &mut Task, domain: &Domain) -> Result<(), MiniplanError> {
    for def in domain.structure().iter() {
        if let StructureDef::Action(action) = def {
            ground_action(task, action)?;
        }
        // StructureDef::Derived is handled by derived::expand_into_init above.
    }
    Ok(())
}

fn ground_action(task: &mut Task, action: &ActionDefinition) -> Result<(), MiniplanError> {
    let action_name = action.symbol().to_string();

    let params: Vec<(String, String)> = action
        .parameters()
        .iter()
        .map(|typed_var| {
            let var = typed_var.value();
            let name = var.to_string();
            let sort = type_to_string(typed_var.type_());
            (name, sort)
        })
        .collect();

    let precondition = action.precondition();
    let pre_gds: Vec<GoalDefinition> = precondition
        .iter()
        .filter_map(precondition_to_goal)
        .collect();

    let combined_pre = if pre_gds.len() == 1 {
        pre_gds[0].clone()
    } else if !pre_gds.is_empty() {
        GoalDefinition::new_and(pre_gds)
    } else {
        GoalDefinition::AtomicFormula(pddl::AtomicFormula::new_predicate(
            pddl::Predicate::from_static("dummy"),
            vec![],
        ))
    };

    let bindings_list = generate_bindings(&params, &task.objects, &task.types)?;

    for bindings in &bindings_list {
        let dnf = walk_goal_definition(&combined_pre, bindings)?;
        if dnf.is_empty() {
            continue;
        }

        for literals in dnf.iter() {
            let (pre_pos, pre_neg) = build_state_from_literals(literals, task)?;

            let effects = action.effect();
            let (add, del, conditional) = extract_effects(effects, bindings, task)?;

            let op_name = if bindings.is_empty() {
                action_name.clone()
            } else {
                let args: Vec<&str> = bindings.iter().map(|(_, v)| v.as_str()).collect();
                format!("{}({})", action_name, args.join(","))
            };

            let cost = extract_action_cost(action)?;

            let op = Operator {
                id: OpId(task.operators.len()),
                name: op_name,
                pre_pos,
                pre_neg,
                add,
                del,
                conditional,
                cost,
            };

            task.operators.push(op);
        }
    }

    Ok(())
}

fn generate_bindings(
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

fn type_to_string(t: &pddl::Type) -> String {
    match t {
        pddl::Type::Exactly(pt) => pt.to_string(),
        pddl::Type::EitherOf(pts) => pts
            .first()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "object".to_owned()),
    }
}
