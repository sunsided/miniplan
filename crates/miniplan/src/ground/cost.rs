use pddl::{ActionDefinition, FluentExpression, FunctionHead, Optimization, Problem};
use pddl::{AssignOp, ConditionalEffect, PrimitiveEffect};

use crate::error::MiniplanError;

pub fn extract_action_cost(action: &ActionDefinition) -> Result<u32, MiniplanError> {
    let effects = match action.effect() {
        Some(e) => e,
        None => return Ok(1),
    };

    for ceffect in effects.iter() {
        if let ConditionalEffect::Effect(PrimitiveEffect::AssignNumericFluent(op, head, exp)) =
            ceffect
            && is_total_cost_increase(op, head)
            && let Some(val) = extract_numeric_value(exp)
        {
            return Ok(val);
        }
    }

    Ok(1)
}

fn is_total_cost_increase(op: &AssignOp, head: &FunctionHead) -> bool {
    if !matches!(op, AssignOp::Increase) {
        return false;
    }
    match head {
        FunctionHead::Simple(fs) => fs.to_string() == "total-cost",
        FunctionHead::WithTerms(fs, _) => fs.to_string() == "total-cost",
    }
}

fn extract_numeric_value(exp: &FluentExpression) -> Option<u32> {
    match exp {
        FluentExpression::Number(n) => {
            let val: f32 = **n;
            Some(val as u32)
        }
        _ => None,
    }
}

#[allow(dead_code)]
pub fn has_action_costs_metric(problem: &Problem) -> bool {
    if let Some(metric) = problem.metric_spec() {
        matches!(metric.optimization(), Optimization::Minimize)
    } else {
        false
    }
}
