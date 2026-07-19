//! Purpose:
//! Eval registry entry and implementation for `unset`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Direct calls stay source-sensitive so writable operands can be removed.

eval_builtin! {
    name: "unset",
    area: Symbols,
    params: [var],
    variadic: vars,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `unset(...)` calls over eval-visible variables and object properties.
pub(in crate::interpreter) fn eval_unset_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_unset(args, context, scope, values)
}

/// Evaluates callable `unset(...)` after values have already been materialized.
pub(in crate::interpreter) fn eval_unset_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_unset_result(evaluated_args, values)
}

/// Evaluates direct `unset(...)` calls over eval-visible variables and object properties.
pub(in crate::interpreter) fn eval_builtin_unset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        match arg {
            EvalExpr::LoadVar(name) => {
                if let Some(replaced) = unset_scope_cell(scope, name.clone()) {
                    values.release(replaced)?;
                }
            }
            EvalExpr::PropertyGet { object, property } => {
                let object = eval_expr(object, context, scope, values)?;
                eval_property_unset_result(object, property, context, values)?;
            }
            EvalExpr::DynamicPropertyGet { object, property } => {
                let object = eval_expr(object, context, scope, values)?;
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_unset_result(object, &property, context, values)?;
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    values.null()
}

/// Evaluates callable `unset(...)` after values have already been materialized.
pub(in crate::interpreter) fn eval_unset_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.null()
}
