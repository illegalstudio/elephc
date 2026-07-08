//! Purpose:
//! Declarative eval registry entry for `flock`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference path.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "flock",
    area: Filesystem,
    params: [stream, operation, would_block: by_ref = EvalBuiltinDefaultValue::Null],
    by_ref: [would_block],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `flock` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_flock_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let operation = eval_expr(&args[1], context, scope, values)?;
    if args.len() >= 3 {
        eval_expr(&args[2], context, scope, values)?;
        values.warning("flock(): Argument #3 ($would_block) must be passed by reference, value given")?;
    }
    let (success, _) = eval_flock_result(stream, operation, context, values)?;
    values.bool_value(success)
}

/// Dispatches evaluated-argument calls for the `flock` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_flock_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if evaluated_args.len() >= 3 {
        values.warning("flock(): Argument #3 ($would_block) must be passed by reference, value given")?;
    }
    let (success, _) = eval_flock_result(evaluated_args[0], evaluated_args[1], context, values)?;
    values.bool_value(success)
}

/// Evaluates PHP `flock($stream, $operation, &$would_block = null)` over eval call args.
pub(in crate::interpreter) fn eval_builtin_flock(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (stream, operation, would_block_target) =
        eval_flock_direct_args(args, context, scope, values)?;
    let (success, would_block) = eval_flock_result(stream, operation, context, values)?;
    if let Some(target) = would_block_target {
        let value = values.bool_value(would_block)?;
        eval_write_direct_ref_target(
            &target,
            value,
            context,
            values,
            Some(ScopeCellOwnership::Owned),
        )?;
    }
    values.bool_value(success)
}

/// Applies a materialized PHP `flock()` operation to a local eval stream resource.
pub(in crate::interpreter) fn eval_flock_result(
    stream: RuntimeCellHandle,
    operation: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(bool, bool), EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let operation = eval_int_value(operation, values)?;
    if let Some(success) = eval_user_wrapper_flock_result(id, operation, context, values)? {
        return Ok((success, false));
    }
    Ok(context
        .stream_resources()
        .flock(id, operation)
        .unwrap_or((false, false)))
}

/// Evaluates and binds direct `flock()` arguments while keeping by-ref output writable.
fn eval_flock_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<
    (
        RuntimeCellHandle,
        RuntimeCellHandle,
        Option<EvalReferenceTarget>,
    ),
    EvalStatus,
> {
    let mut stream = None;
    let mut operation = None;
    let mut would_block = None;
    let mut positional_index = 0;
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let parameter = if let Some(name) = arg.name() {
            saw_named = true;
            name
        } else {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let parameter = match positional_index {
                0 => "stream",
                1 => "operation",
                2 => "would_block",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "stream" => {
                if stream.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                stream = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "operation" => {
                if operation.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                operation = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "would_block" => {
                if would_block.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let (_, target) = eval_call_arg_value(arg.value(), context, scope, values)?;
                would_block = Some(target.ok_or(EvalStatus::RuntimeFatal)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let stream = stream.ok_or(EvalStatus::RuntimeFatal)?;
    let operation = operation.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((stream, operation, would_block))
}
