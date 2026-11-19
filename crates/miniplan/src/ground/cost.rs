//! Action costs from `(increase (total-cost) n)` effects.

use pddl::{ActionDefinition, FluentExpression, FunctionHead};
use pddl::{AssignOp, ConditionalEffect, PrimitiveEffect};

/// Returns the cost of `action`: the constant of its first top-level
/// `(increase (total-cost) n)` effect, or unit cost `1` when there is none.
///
/// Increases nested in `forall` or `when` effects and non-constant increases
/// are ignored.
pub(super) fn extract_action_cost(action: &ActionDefinition) -> u32 {
    let Some(effects) = action.effect() else {
        return 1;
    };

    for ceffect in effects.iter() {
        if let ConditionalEffect::Effect(PrimitiveEffect::AssignNumericFluent(op, head, exp)) =
            ceffect
            && is_total_cost_increase(op, head)
            && let Some(val) = extract_numeric_value(exp)
        {
            return val;
        }
    }

    1
}

/// Whether the assignment `op head` is an `increase` of the `total-cost`
/// function (with or without arguments).
fn is_total_cost_increase(op: &AssignOp, head: &FunctionHead) -> bool {
    if !matches!(op, AssignOp::Increase) {
        return false;
    }

    match head {
        FunctionHead::Simple(fs) | FunctionHead::WithTerms(fs, _) => fs.to_string() == "total-cost",
    }
}

/// Returns the value of a numeric literal as a whole cost, or `None` for any
/// other expression.
///
/// The conversion saturates: fractions are truncated toward zero, negative
/// values and NaN become `0`, and values above `u32::MAX` become `u32::MAX`.
fn extract_numeric_value(exp: &FluentExpression) -> Option<u32> {
    match exp {
        FluentExpression::Number(n) => {
            let val: f32 = **n;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the saturating, truncating `as` conversion is the established cost \
                          semantics for fractional, negative and out-of-range PDDL numbers"
            )]
            let cost = val as u32;
            Some(cost)
        }
        _ => None,
    }
}
