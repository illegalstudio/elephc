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
