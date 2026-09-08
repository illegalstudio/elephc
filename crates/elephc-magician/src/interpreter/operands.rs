//! Purpose:
//! Owns expression operands until their consumer has finished reading them.
//!
//! Called from:
//! - Core builtin adapters, property assignments, scalar expressions, and branch conditions.
//!
//! Key details:
//! - Storage reads acquire a retained lease; newly produced cells transfer their owner.
//! - Cleanup runs on failed argument evaluation and failed operations as well as success.

use super::*;

/// Prints a borrowed operand and releases its temporary owner and any separate string conversion.
pub(in crate::interpreter) fn eval_echo_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    with_eval_void_operands(&[expr], context, scope, values, |args, context, _, values| {
        let value = eval_string_context_value(args[0].borrowed(), context, values)?;
        if value == args[0] {
            values.echo(value)
        } else {
            with_eval_value_lease(value, context, values, |value, _, values| values.echo(value))
        }
    })
}

/// Applies a unary operation while consuming its source and any synthetic zero operand.
pub(in crate::interpreter) fn eval_unary_expr(
    op: EvalUnaryOp,
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_operands(&[expr], context, scope, values, |args, _, _, values| {
        match op {
            EvalUnaryOp::Plus | EvalUnaryOp::Negate => {
                let zero = values.int(0)?;
                let result = if op == EvalUnaryOp::Plus {
                    values.add(zero, args[0])
                } else {
                    values.sub(zero, args[0])
                };
                let released = values.release(zero);
                result.and_then(|result| released.map(|()| result))
            }
            EvalUnaryOp::LogicalNot => {
                let truthy = values.truthy(args[0])?;
                values.bool_value(!truthy)
            }
            EvalUnaryOp::BitNot => values.bit_not(args[0]),
        }
    })
}

/// Applies binary operations with source-order leases and lazy boolean right-hand operands.
pub(in crate::interpreter) fn eval_binary_expr(
    op: EvalBinOp,
    left: &EvalExpr,
    right: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if matches!(op, EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr) {
        let left = eval_condition(left, context, scope, values)?;
        let result = if (op == EvalBinOp::LogicalAnd && !left)
            || (op == EvalBinOp::LogicalOr && left)
        {
            left
        } else {
            eval_condition(right, context, scope, values)?
        };
        return values.bool_value(result);
    }
    with_eval_operands(&[left, right], context, scope, values, |args, context, _, values| {
        eval_binary_result(op, args[0], args[1], context, values)
    })
}

/// Evaluates an operand with one owner, retaining borrowed storage before later side effects.
pub(in crate::interpreter) fn eval_owned_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_expr(expr, context, scope, values)?;
    if value.is_borrowed() { values.retain(value) } else { Ok(value) }
}

/// Releases an expression result only when it carries an owner rather than a storage borrow.
pub(in crate::interpreter) fn release_expr_result(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if value.is_borrowed() { Ok(()) } else { eval_release_value(context, values, value) }
}

/// Keeps source-order argument owners alive through dispatch and reference writeback.
/// Borrowed returns acquire an owner before cleanup, including identity calls and constructors.
pub(in crate::interpreter) fn with_eval_call_arguments<V: RuntimeValueOps>(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut V,
    consume: impl FnOnce(
        Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let arguments = eval_owned_call_arg_values(args, context, scope, values)?;
    let borrowed_arguments = arguments.iter().cloned().map(|mut argument| {
        argument.value = argument.value.borrowed();
        argument
    }).collect();
    let result = consume(borrowed_arguments, context, scope, values);
    finish_eval_argument_values(result, arguments.into_iter().map(|argument| argument.value), context, values)
}

/// Retains borrowed returns before releasing argument owners, preserving the original call error.
pub(in crate::interpreter) fn finish_eval_argument_values(
    result: Result<RuntimeCellHandle, EvalStatus>,
    arguments: impl IntoIterator<Item = RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = result.and_then(|value| {
        if value.is_borrowed() { values.retain(value) } else { Ok(value) }
    });
    let mut released = Ok(());
    for argument in arguments {
        let cleanup = release_expr_result(argument, context, values);
        if released.is_ok() { released = cleanup; }
    }
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    }
}

pub(in crate::interpreter) use with_eval_call_arguments as with_eval_method_arguments;

/// Owns extracted call-array values until dispatch and reference writeback have completed.
pub(in crate::interpreter) fn with_eval_array_call_arguments<V: RuntimeValueOps>(
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    consume: impl FnOnce(
        Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let arguments = eval_array_call_arg_values(array, context, values)?;
    let borrowed_arguments = arguments.iter().cloned().map(|mut argument| {
        argument.value = argument.value.borrowed();
        argument
    }).collect();
    let result = consume(borrowed_arguments, context, values);
    finish_eval_argument_values(
        result, arguments.into_iter().map(|argument| argument.value), context, values,
    )
}

/// Evaluates a condition and consumes its temporary owner after reading PHP truthiness.
pub(in crate::interpreter) fn eval_condition(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let value = eval_owned_expr(expr, context, scope, values)?;
    let result = values.truthy(value);
    let released = eval_release_value(context, values, value);
    result.and_then(|result| released.map(|()| result))
}

/// Runs a consumer of borrowed operands and returns its independent result after releasing leases.
pub(in crate::interpreter) fn with_eval_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut V,
    consume: impl FnOnce(
        &[RuntimeCellHandle], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut operands = Vec::with_capacity(args.len());
    let result = (|| {
        for arg in args {
            operands.push(eval_owned_expr(arg, context, scope, values)?);
        }
        consume(&operands, context, scope, values)
    })();
    let mut cleanup = Ok(());
    for operand in operands {
        let released = eval_release_value(context, values, operand);
        if cleanup.is_ok() { cleanup = released; }
    }
    match (result, cleanup) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = release_expr_result(value, context, values);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    }
}

/// Keeps operands alive through a statement and releases every lease on success or failure.
pub(in crate::interpreter) fn with_eval_void_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut V,
    consume: impl FnOnce(
        &[RuntimeCellHandle], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V,
    ) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::with_capacity(args.len());
    let mut result = (|| {
        for arg in args {
            operands.push(eval_owned_expr(arg, context, scope, values)?);
        }
        consume(&operands, context, scope, values)
    })();
    for operand in operands {
        let released = eval_release_value(context, values, operand);
        if result.is_ok() { result = released; }
    }
    result
}

/// Gives a statement consumer one owned lease for a materialized value and releases it on every exit.
pub(in crate::interpreter) fn with_eval_value_lease<V: RuntimeValueOps>(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    consume: impl FnOnce(RuntimeCellHandle, &mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let value = if value.is_borrowed() { values.retain(value)? } else { value };
    let result = consume(value, context, values);
    let released = eval_release_value(context, values, value);
    result.and(released)
}
