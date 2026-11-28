//! Action costs from `(increase (total-cost) n)` effects.

use pddl::{ActionDefinition, FluentExpression, FunctionHead};
use pddl::{AssignOp, ConditionalEffect, PrimitiveEffect};

use crate::error::MiniplanError;

/// Returns the cost of `action`: the constant of its first top-level
/// `(increase (total-cost) n)` effect, or unit cost `1` when there is none.
///
/// Increases nested in `forall` or `when` effects and non-constant increases
/// are ignored.
///
/// # Errors
///
/// Returns [`MiniplanError::Ground`] when the constant is not a whole number
/// in `0..=u32::MAX`; see [`extract_numeric_value`].
pub(super) fn extract_action_cost(action: &ActionDefinition) -> Result<u32, MiniplanError> {
    let Some(effects) = action.effect() else {
        return Ok(1);
    };

    for ceffect in effects {
        if let ConditionalEffect::Effect(PrimitiveEffect::AssignNumericFluent(op, head, exp)) =
            ceffect
            && is_total_cost_increase(op, head)
            && let Some(val) = extract_numeric_value(exp)
        {
            return val.map_err(|val| {
                MiniplanError::Ground(format!(
                    "action '{}' has cost {val}; only whole costs from 0 to {} are supported",
                    **action.symbol(),
                    u32::MAX
                ))
            });
        }
    }

    Ok(1)
}

/// Whether the assignment `op head` is an `increase` of the `total-cost`
/// function (with or without arguments), spelled in any case as PDDL allows.
fn is_total_cost_increase(op: &AssignOp, head: &FunctionHead) -> bool {
    if !matches!(op, AssignOp::Increase) {
        return false;
    }

    match head {
        FunctionHead::Simple(fs) | FunctionHead::WithTerms(fs, _) => {
            fs.eq_ignore_ascii_case("total-cost")
        }
    }
}

/// Returns the value of a numeric literal as a whole cost, or `None` for any
/// other expression.
///
/// The inner result is `Err` with the literal when it is fractional, negative,
/// NaN or above `u32::MAX`: such costs cannot be represented exactly, and
/// rounding them would let the search return non-optimal plans.
fn extract_numeric_value(exp: &FluentExpression) -> Option<Result<u32, f64>> {
    match exp {
        FluentExpression::Number(n) => {
            let val: f64 = **n;
            if val.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&val) {
                return Some(Err(val));
            }

            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "`val` was checked to be a whole number within `u32`"
            )]
            let cost = val as u32;
            Some(Ok(cost))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pddl::{Domain, Parser, StructureDef};

    /// The cost of the only action of `domain`.
    fn single_action_cost(domain: &str) -> Result<u32, MiniplanError> {
        let domain = Domain::from_str(domain).expect("domain should parse");
        let action = domain
            .structure()
            .iter()
            .find_map(|def| match def {
                StructureDef::Action(action) => Some(action),
                _ => None,
            })
            .expect("domain should have an action");

        extract_action_cost(action)
    }

    /// An upper-case `TOTAL-COST`, which parses since pddl 0.3, is the action cost.
    #[test]
    fn total_cost_matches_in_any_case() {
        let domain = "(DEFINE (DOMAIN d) (:REQUIREMENTS :STRIPS :ACTION-COSTS)
            (:PREDICATES (DONE)) (:FUNCTIONS (TOTAL-COST))
            (:ACTION GO :PARAMETERS () :EFFECT (AND (DONE) (INCREASE (TOTAL-COST) 5))))";

        assert_eq!(single_action_cost(domain).expect("whole cost"), 5);
    }

    /// Fractional and out-of-range costs are rejected instead of rounded. (The parser
    /// already refuses negative literals.)
    #[test]
    fn non_whole_costs_are_rejected() {
        for cost in ["0.5", "1.4", "4294967296"] {
            let domain = format!(
                "(define (domain d) (:requirements :strips :action-costs)
                (:predicates (done)) (:functions (total-cost))
                (:action go :parameters () :effect (and (done) (increase (total-cost) {cost}))))"
            );

            let err = single_action_cost(&domain).expect_err(cost);
            assert!(
                matches!(&err, MiniplanError::Ground(msg) if msg.contains("action 'go'")),
                "{cost}: {err}"
            );
        }
    }
}
