//! Purpose:
//! Owns expression operands and call arguments until their consumers finish reading them.
//!
//! Called from:
//! - Expression, statement, constructor, and callable dispatch paths.
//!
//! Key details:
//! - Storage reads acquire retained leases, while newly produced cells transfer their owner.
//! - Consumers receive borrowed views and borrowed results are promoted before owner cleanup.
//! - Cleanup runs after failed evaluation and failed consumers as well as successful calls.

use super::*;

/// One expression owner plus whether its array metadata belongs to the temporary identity.
pub(in crate::interpreter) struct EvalValueLease {
    pub(super) owner: RuntimeCellHandle,
    clears_metadata: bool,
}

impl EvalValueLease {
    /// Records an already owned value whose source metadata belongs to durable storage.
    pub(in crate::interpreter) const fn preserving_metadata(owner: RuntimeCellHandle) -> Self {
        Self {
            owner,
            clears_metadata: false,
        }
    }
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
                if op == EvalUnaryOp::Negate && values.type_tag(args[0])? == EVAL_TAG_FLOAT {
                    let bits = values.raw_value_word(args[0])? ^ (1_u64 << 63);
                    return values.raw_word_value(EVAL_TAG_FLOAT, bits);
                }
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

/// Applies binary operations with source-order leases and lazy boolean right operands.
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
    with_eval_operands(
        &[left, right],
        context,
        scope,
        values,
        |args, context, _, values| eval_binary_result(op, args[0], args[1], context, values),
    )
}

/// Evaluates an expression and acquires one owner when it borrowed durable storage.
pub(in crate::interpreter) fn acquire_expr_lease(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalValueLease, EvalStatus> {
    let value = eval_expr(expr, context, scope, values)?;
    acquire_value_lease(value, values)
}

/// Acquires one owner for a produced value while remembering its metadata provenance.
pub(in crate::interpreter) fn acquire_value_lease(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalValueLease, EvalStatus> {
    if value.is_borrowed() {
        Ok(EvalValueLease {
            owner: values.retain(value)?,
            clears_metadata: false,
        })
    } else {
        Ok(EvalValueLease {
            owner: value,
            clears_metadata: true,
        })
    }
}

/// Evaluates an operand with one owner, retaining borrowed storage before later side effects.
pub(in crate::interpreter) fn eval_leased_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    acquire_expr_lease(expr, context, scope, values).map(|lease| lease.owner)
}

/// Releases an expression result only when it carries an owner rather than a storage borrow.
pub(in crate::interpreter) fn release_expr_result(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if value.is_borrowed() {
        Ok(())
    } else {
        eval_release_value(context, values, value)
    }
}

/// Evaluates a condition and consumes its temporary owner after reading PHP truthiness.
pub(in crate::interpreter) fn eval_condition(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let lease = acquire_expr_lease(expr, context, scope, values)?;
    let result = values.truthy(lease.owner);
    if lease.clears_metadata {
        context.clear_array_metadata(lease.owner);
    }
    let released = eval_release_value(context, values, lease.owner);
    result.and_then(|result| released.map(|()| result))
}

/// Compares two borrowed values and consumes the comparison cell after reading truthiness.
pub(in crate::interpreter) fn eval_comparison_condition(
    op: EvalBinOp,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let comparison = values.compare(op, left, right)?;
    let result = values.truthy(comparison);
    let released = values.release(comparison);
    result.and_then(|result| released.map(|()| result))
}

/// Runs a consumer of borrowed operands and returns its independent result after cleanup.
pub(in crate::interpreter) fn with_eval_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut V,
    consume: impl FnOnce(
        &[RuntimeCellHandle],
        &mut ElephcEvalContext,
        &mut ElephcEvalScope,
        &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut leases = Vec::with_capacity(args.len());
    let result = (|| {
        for arg in args {
            leases.push(acquire_expr_lease(arg, context, scope, values)?);
        }
        let borrowed = leases
            .iter()
            .map(|lease| lease.owner.borrowed())
            .collect::<Vec<_>>();
        let result = consume(&borrowed, context, scope, values)?;
        promote_borrowed_result(result, values)
    })();
    finish_value_leases(result, leases, context, values)
}

/// Keeps source-order argument owners alive through dispatch and reference writeback.
pub(in crate::interpreter) fn with_eval_call_arguments<V: RuntimeValueOps>(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut V,
    consume: impl FnOnce(
        Vec<EvaluatedCallArg>,
        &mut ElephcEvalContext,
        &mut ElephcEvalScope,
        &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut owners = Vec::new();
    let mut arguments = Vec::new();
    let result = (|| {
        eval_leased_call_arg_values(
            args,
            context,
            scope,
            values,
            &mut owners,
            &mut arguments,
        )?;
        let borrowed = arguments
            .into_iter()
            .map(|argument| EvaluatedCallArg {
                value: argument.value.borrowed(),
                ..argument
            })
            .collect();
        let result = consume(borrowed, context, scope, values)?;
        promote_borrowed_result(result, values)
    })();
    finish_value_leases(result, owners, context, values)
}

/// Converts a borrowed result into a caller-owned result before its source owner is released.
pub(in crate::interpreter) fn promote_borrowed_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if value.is_borrowed() {
        values.retain(value)
    } else {
        Ok(value)
    }
}

/// Releases expression leases after preserving a successful result and its side metadata.
fn finish_value_leases<V: RuntimeValueOps>(
    result: Result<RuntimeCellHandle, EvalStatus>,
    leases: Vec<EvalValueLease>,
    context: &mut ElephcEvalContext,
    values: &mut V,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let preserved = result.as_ref().ok().copied();
    let mut cleanup = Ok(());
    for lease in leases.into_iter().rev() {
        if lease.clears_metadata
            && preserved.map_or(true, |value| value.as_ptr() != lease.owner.as_ptr())
        {
            context.clear_array_metadata(lease.owner);
        }
        let released = eval_release_value(context, values, lease.owner);
        if cleanup.is_ok() {
            cleanup = released;
        }
    }
    match (result, cleanup) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            context.clear_array_metadata(value);
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    }
}
