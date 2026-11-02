//! Purpose:
//! Eval registry entries and implementations for `register_tick_function` and
//! `unregister_tick_function`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Neither function does anything on its own: php runs a handler only when a
//!   `declare(ticks=N)` is also in force, and the directive alone runs nothing either. The pair
//!   is what `interpreter/statements/dispatch.rs` consults after each statement.
//! - A registered handler is RETAINED and held by the context, because it outlives the call that
//!   registered it. `unregister_tick_function` hands that reference back and releases it.

eval_builtin! {
    contract: "register_tick_function",
    area: Core,
    direct: Core,
    values: Core,
}

eval_builtin! {
    contract: "unregister_tick_function",
    area: Core,
    direct: Core,
    values: Core,
}

use super::super::super::*;

/// Evaluates direct `register_tick_function(...)` calls.
pub(in crate::interpreter) fn eval_builtin_register_tick_function(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_expr(callback, context, scope, values)?;
    let result = eval_register_tick_function_result(&[callback], context, values);
    values.release(callback)?;
    result
}

/// Evaluates materialized `register_tick_function(...)` arguments.
///
/// php returns `true`. Extra arguments after the callback are bound to the handler; this accepts
/// the callback alone, which is every call Symfony-shaped code makes, and refuses the rest rather
/// than dropping arguments a handler would have received.
pub(in crate::interpreter) fn eval_register_tick_function_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let owned = values.retain(*callback)?;
    context.push_tick_function(owned);
    values.bool_value(true)
}

/// Evaluates direct `unregister_tick_function(...)` calls.
pub(in crate::interpreter) fn eval_builtin_unregister_tick_function(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_expr(callback, context, scope, values)?;
    let result = eval_unregister_tick_function_result(&[callback], context, values);
    values.release(callback)?;
    result
}

/// Evaluates materialized `unregister_tick_function(...)` arguments.
///
/// php returns null and removes the FIRST handler equal to the argument, leaving any duplicate
/// registration of the same callback in place.
pub(in crate::interpreter) fn eval_unregister_tick_function_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let mut found = None;
    for (index, handler) in context.tick_functions().into_iter().enumerate() {
        let equal = values.compare(EvalBinOp::LooseEq, handler, *callback)?;
        let matches = values.truthy(equal)?;
        values.release(equal)?;
        if matches {
            found = Some(index);
            break;
        }
    }
    if let Some(index) = found {
        if let Some(owned) = context.take_tick_function(index) {
            values.release(owned)?;
        }
    }
    values.null()
}
