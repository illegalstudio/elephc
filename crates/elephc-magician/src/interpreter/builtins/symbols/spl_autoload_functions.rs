//! Purpose:
//! Eval registry entry and implementation for `spl_autoload_functions`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Returned callback values preserve request-global registration order.

eval_builtin! {
    contract: "spl_autoload_functions",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload_functions()` calls.
pub(in crate::interpreter) fn eval_spl_autoload_functions_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_spl_autoload_functions(args, context, scope, values)
}

/// Builds the PHP `spl_autoload_functions()` array for generated (AOT) callers.
///
/// The generated `spl_autoload_register` lowering registers through this crate's request-global
/// autoload table, so only the interpreter can answer what PHP's queue holds.
pub(crate) fn eval_registered_spl_autoload_functions_result(
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_functions_result(&[], context, values)
}

/// Evaluates materialized `spl_autoload_functions()` arguments.
pub(in crate::interpreter) fn eval_spl_autoload_functions_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_functions_result(evaluated_args, context, values)
}

/// Evaluates `spl_autoload_functions()`.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_functions(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_spl_autoload_functions_result(&[], context, values)
}

/// Evaluates materialized `spl_autoload_functions()`.
pub(in crate::interpreter) fn eval_spl_autoload_functions_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let callbacks = eval_registered_autoload_callbacks(context);
    let mut result = values.array_new(callbacks.len())?;
    for (index, callback) in callbacks.into_iter().enumerate() {
        let index = values.int(index as i64)?;
        // The array TAKES the value it is given. Handing it the cell the registry holds put a
        // reference the registry still owns into userland, and the entries read back as integers
        // rather than as the callables that were registered.
        let callback = values.retain(callback)?;
        result = values.array_set(result, index, callback)?;
    }
    Ok(result)
}

/// Returns every registered callback in the request's own registration order.
///
/// PHP keeps ONE autoload queue per request. elephc keeps each callback on the context that
/// registered it, because that is where it is retained and released, so the list is assembled from
/// every owner and ordered by the request-wide number each registration was given. Reporting only
/// the asking context's callbacks left out every loader registered by an included file.
fn eval_registered_autoload_callbacks(context: &ElephcEvalContext) -> Vec<RuntimeCellHandle> {
    #[cfg(test)]
    {
        context.autoload_callbacks()
    }
    #[cfg(not(test))]
    {
        let here = context as *const ElephcEvalContext;
        let mut owners = crate::context::global_eval_autoload_contexts_snapshot();
        if !owners
            .iter()
            .any(|owner| std::ptr::eq(*owner as *const ElephcEvalContext, here))
        {
            owners.push(here as *mut ElephcEvalContext);
        }
        let mut ordered: Vec<(i64, RuntimeCellHandle)> = Vec::new();
        for owner in owners {
            if std::ptr::eq(owner as *const ElephcEvalContext, here) {
                ordered.extend(context.autoload_callbacks_ordered());
                continue;
            }
            if let Some(owner) = unsafe { owner.as_ref() } {
                ordered.extend(owner.autoload_callbacks_ordered());
            }
        }
        ordered.sort_by_key(|(sequence, _)| *sequence);
        ordered.into_iter().map(|(_, callback)| callback).collect()
    }
}
