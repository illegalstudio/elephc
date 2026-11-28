//! Purpose:
//! Executes exact switch matching with balanced subject and label owners.
//!
//! Called from:
//! - The switch statement dispatcher under strict-comparisons.
//!
//! Key details:
//! - Variable subjects stay live until each label's source effects finish.
//! - Other subjects are evaluated once and retired before the selected body starts.
//! - Existing operand ledgers protect comparison results through argument retirement.

use super::*;

/// Selects an exact case in source order before executing ordinary PHP fallthrough.
pub(super) fn execute(
    expression: &EvalExpr, cases: &[EvalSwitchCase], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let live = matches!(expression, EvalExpr::LoadVar(_));
    let subjects: Vec<_> = (!live).then_some(expression).into_iter().collect();
    let mut default = None;
    let mut selected = None;
    with_eval_void_operands(&subjects, context, scope, values, |subjects, context, scope, values| {
        for (index, case) in cases.iter().enumerate() {
            let Some(condition) = &case.condition else {
                if default.is_none() { default = Some(index); }
                continue;
            };
            let comparison = with_eval_operands(&[condition], context, scope, values,
                |conditions, context, scope, values| {
                    if live {
                        with_eval_operands(&[expression], context, scope, values,
                            |subjects, _, _, values| values.compare(EvalBinOp::StrictEq, subjects[0], conditions[0]))
                    } else { values.compare(EvalBinOp::StrictEq, subjects[0], conditions[0]) }
                })?;
            let matched = values.truthy(comparison);
            let released = eval_release_value(context, values, comparison);
            if matched.and_then(|matched| released.map(|()| matched))? {
                selected = Some(index);
                break;
            }
        }
        Ok(())
    })?;
    let Some(first) = selected.or(default) else { return Ok(EvalControl::None); };
    for case in &cases[first..] {
        match execute_statements(&case.body, context, scope, values)? {
            EvalControl::None => {}
            EvalControl::Break | EvalControl::Continue => break,
            control => return Ok(control),
        }
    }
    Ok(EvalControl::None)
}
