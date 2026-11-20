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
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Prints a borrowed operand and releases its temporary owner and any separate string conversion.
pub(in crate::interpreter) fn eval_echo_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    with_eval_void_operands(&[expr], context, scope, values, |args, context, _, values| {
        checked_strings::formatting(args[0], context, values)?;
        let value = eval_string_context_value(args[0].borrowed(), context, values)?;
        if value == args[0] {
            values.echo(value)
        } else {
            with_eval_value_lease(value, context, values, |value, _, values| values.echo(value))
        }
    })
}

/// Applies a unary operation while consuming its source and any synthetic numeric operand.
pub(in crate::interpreter) fn eval_unary_expr(
    op: EvalUnaryOp,
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        return mixed_eval::unary(op, expr, context, scope, values).map(|result| result.0);
    }
    with_eval_operands(&[expr], context, scope, values, |args, context, _, values| {
        eval_unary_value(op, args[0], context, values)
    })
}

/// Applies a unary operation to an already evaluated operand without releasing it.
pub(in crate::interpreter) fn eval_unary_value(
    op: EvalUnaryOp,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match op {
        EvalUnaryOp::Plus | EvalUnaryOp::Negate => {
            use elephc_semantics::numeric_unary::NumericUnaryOp;
            let numeric = if op == EvalUnaryOp::Plus { NumericUnaryOp::Plus } else { NumericUnaryOp::Negate };
            if values.type_tag(value)? == EVAL_TAG_OBJECT {
                if op == EvalUnaryOp::Negate {
                    if let Some(result) = runtime_ops::object_operators::negate(values, value)? { return Ok(result); }
                } else {
                    let zero = values.int(0)?;
                    let result = runtime_ops::object_operators::arithmetic(values,
                        elephc_builtin_contract::ObjectArithmetic::Add, zero, value)
                        .and_then(|result| match result {
                            Some(result) => Ok(result),
                            None => checked_numeric::unary(numeric, value, context, values),
                        });
                    return finish_synthesized_method_arguments(result, [zero], context, values);
                }
            }
            checked_numeric::unary(numeric, value, context, values)
        }
        EvalUnaryOp::LogicalNot => {
            let truthy = eval_boolean_condition(value, context, values)?;
            values.bool_value(!truthy)
        }
        EvalUnaryOp::BitNot => {
            if let Some(result) = runtime_ops::object_operators::bit_not(values, value)? { return Ok(result); }
            numeric_domains::require_unary(elephc_semantics::numeric_domains::NumericOperator::BitNot, value, context, values)?;
            values.bit_not(value)
        }
    }
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
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        return mixed_eval::binary(op, left, right, context, scope, values).map(|result| result.0);
    }
    with_eval_operands(&[left, right], context, scope, values, |args, context, _, values| {
        eval_binary_result(op, args[0], args[1], context, values)
    })
}

/// Releases an expression result only when it carries an owner rather than a storage borrow.
pub(in crate::interpreter) fn release_expr_result(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if value.is_borrowed() { Ok(()) } else { eval_release_value(context, values, value) }
}

/// Retires synthesized caller operands after a borrowing method binder has finished.
pub(in crate::interpreter) fn finish_synthesized_method_arguments<I>(
    result: Result<RuntimeCellHandle, EvalStatus>, owners: I,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus>
where I: IntoIterator<Item = RuntimeCellHandle>, I::IntoIter: DoubleEndedIterator,
{
    finish_eval_argument_values(result, owners.into_iter().rev(), context, values)
}

/// Compares borrowed values and releases the comparison cell after reading its truthiness.
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

/// Retains a borrowed result before its source owner is released.
pub(in crate::interpreter) fn promote_borrowed_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if value.is_borrowed() { values.retain(value) } else { Ok(value) }
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
    with_eval_call_arguments_with_modes(args, &CallArgumentModes::default(), context, scope, values, consume)
}

/// Applies resolved parameter passing modes without changing source-order argument ownership.
pub(in crate::interpreter) fn with_eval_call_arguments_with_modes<V: RuntimeValueOps>(
    args: &[EvalCallArg], modes: &CallArgumentModes,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (arguments, receiver_owners) = eval_owned_call_arg_values(args, modes, context, scope, values)?;
    let (borrowed_arguments, argument_owners) = own_evaluated_call_arguments(arguments,
        |argument, index| modes.needs_evaluation_owner(argument.name.as_deref(), index));
    let receiver_owners = receiver_owners.into_iter().map(RuntimeOwnerSlot::new).collect::<Vec<_>>();
    with_runtime_owners(&(&argument_owners, &receiver_owners), || {
        let result = consume(borrowed_arguments, context, scope, values);
        let result = finish_eval_argument_owners(result, &argument_owners, context, values);
        finish_synthesized_method_arguments(result, receiver_owners.iter().map(RuntimeOwnerSlot::take), context, values)
    })
}

/// Retains borrowed returns before releasing argument owners, preserving the original call error.
pub(in crate::interpreter) fn finish_eval_argument_values(
    result: Result<RuntimeCellHandle, EvalStatus>,
    arguments: impl IntoIterator<Item = RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = result.and_then(|value| promote_borrowed_result(value, values))
        .map(RuntimeOwnerSlot::new);
    let arguments = arguments.into_iter().map(RuntimeOwnerSlot::new).collect::<Vec<_>>();
    // Materialization moves each cleanup obligation before any release can reenter PHP.
    let inventory = (&arguments, result.as_ref().ok());
    with_runtime_owners(&inventory, || {
        let mut released = Ok(());
        for argument in &arguments {
            let cleanup = release_expr_result(argument.take(), context, values);
            if released.is_ok() { released = cleanup; }
        }
        match (&result, released) {
            (Err(status), _) => Err(*status),
            (Ok(value), Err(status)) => {
                let _ = eval_release_value(context, values, value.take());
                Err(status)
            }
            (Ok(value), Ok(())) => Ok(value.take()),
        }
    })
}

/// Retires rooted check operands while preserving the original failure without creating a PHP result.
pub(in crate::interpreter) fn finish_eval_void_arguments(
    result: Result<(), EvalStatus>, arguments: impl IntoIterator<Item = RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let arguments = arguments.into_iter().map(RuntimeOwnerSlot::new).collect::<Vec<_>>();
    with_runtime_owners(&arguments, || {
        let mut released = Ok(());
        for argument in &arguments {
            let cleanup = release_expr_result(argument.take(), context, values);
            if released.is_ok() { released = cleanup; }
        }
        result.and(released)
    })
}

#[cfg(test)]
pub(in crate::interpreter) use with_eval_call_arguments as with_eval_method_arguments;

/// Uses the selected instance method's flags for discarded calls and nullsafe chain steps.
pub(in crate::interpreter) fn with_eval_object_method_arguments<V: RuntimeValueOps>(
    args: &[EvalCallArg], object: RuntimeCellHandle, method: &str,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_abstract_instance_method_guard(object, method, context, values)?;
    let modes = CallArgumentModes::for_method(object, method, context, values).unwrap_or_default();
    with_eval_call_arguments_with_modes(args, &modes, context, scope, values, consume)
}

/// Owns extracted call-array values until dispatch and reference writeback have completed.
pub(in crate::interpreter) fn with_eval_array_call_arguments<V: RuntimeValueOps>(
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    consume: impl FnOnce(
        Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_array_call_arguments_with_sources(array, None, context, values, consume)
}

/// Keeps tuple entry evidence beside the established extraction, normalization and owner boundary.
pub(in crate::interpreter) fn with_eval_array_call_arguments_with_sources<V: RuntimeValueOps>(
    array: RuntimeCellHandle, source: Option<&ArgumentSource>, context: &mut ElephcEvalContext,
    values: &mut V, consume: impl FnOnce(Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let arguments = eval_array_call_arg_values(array, context, values)?;
    let (mut borrowed_arguments, argument_owners) = own_evaluated_call_arguments(arguments,
        |argument, _| argument.ref_target.is_some());
    with_runtime_owners(&argument_owners, || {
        let result = array_argument_sources::apply(array, source, &mut borrowed_arguments, context, values)
            .and_then(|()| consume(borrowed_arguments, context, values));
        finish_eval_argument_owners(result, &argument_owners, context, values)
    })
}

/// Evaluates a condition and consumes its temporary owner after reading PHP truthiness.
pub(in crate::interpreter) fn eval_condition(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let projection = mixed_guards::prepare(expr, context, scope, values)?;
    let (value,source)=if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        mixed_eval::evaluate(expr,context,scope,values)?
    } else {(eval_owned_expr(expr,context,scope,values)?,None)};
    let source=source.map(mixed_source_proofs::value_source).unwrap_or_else(||elephc_generics::Type::atom("mixed"));
    let result=mixed_source_proofs::require_condition(&source,context,values)
        .and_then(|()|eval_boolean_condition(value,context,values));
    let released = eval_release_value(context, values, value);
    let result = result.and_then(|result| released.map(|()| result))?;
    mixed_guards::record(expr, result, context, scope);
    mixed_guards::record_projection(projection, result, context, scope);
    Ok(result)
}

/// Keeps a quiet probe's receiver alive and retires its lease on success or failure.
pub(in crate::interpreter) fn with_eval_probe_operand<V: RuntimeValueOps>(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut V,
    probe: impl FnOnce(
        RuntimeCellHandle, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V,
    ) -> Result<bool, EvalStatus>,
) -> Result<bool, EvalStatus> {
    let owner = probe_reads::eval_owned(expr, context, scope, values)?;
    let result = probe(owner.borrowed(), context, scope, values);
    let cleanup = eval_release_value(context, values, owner);
    result.and_then(|answer| cleanup.map(|()| answer))
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
    with_value_operands(args, false, context, scope, values, consume)
}

/// Reads a quiet receiver chain followed by ordinary source operands with shared cleanup.
pub(in crate::interpreter) fn with_eval_probe_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr], context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_value_operands(args, true, context, scope, values, consume)
}

/// Shares operand preparation and returned-value cleanup across ordinary and quiet roots.
fn with_value_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr], probe_first: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut operands = Vec::with_capacity(args.len());
    let prepared = (|| {
        for (index, arg) in args.iter().enumerate() {
            let value = with_runtime_owners(&operands, || if probe_first && index == 0 {
                probe_reads::eval_owned(arg, context, scope, values)
            } else { eval_owned_expr(arg, context, scope, values) })?;
            operands.push(value);
        }
        Ok(())
    })();
    let result = prepared.and_then(|()| with_runtime_owners(&operands, ||
        consume(&operands, context, scope, values)));
    finish_eval_argument_values(result, operands, context, values)
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
    with_void_operands(args, false, context, scope, values, consume)
}

/// Gives a quiet receiver and ordinary keys to a statement consumer with shared owner cleanup.
pub(in crate::interpreter) fn with_eval_probe_void_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr], context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    with_void_operands(args, true, context, scope, values, consume)
}

/// Shares statement-operand preparation without confusing a quiet root with its ordinary key.
fn with_void_operands<V: RuntimeValueOps>(
    args: &[&EvalExpr], probe_first: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let mut operands = Vec::with_capacity(args.len());
    let prepared = (|| {
        for (index, arg) in args.iter().enumerate() {
            let value = with_runtime_owners(&operands, || if probe_first && index == 0 {
                probe_reads::eval_owned(arg, context, scope, values)
            } else { eval_owned_expr(arg, context, scope, values) })?;
            operands.push(value);
        }
        Ok(())
    })();
    let mut result = prepared.and_then(|()| with_runtime_owners(&operands, ||
        consume(&operands, context, scope, values)));
    let operands = operands.into_iter().map(RuntimeOwnerSlot::new).collect::<Vec<_>>();
    with_runtime_owners(&operands, || {
        for operand in &operands {
            let released = eval_release_value(context, values, operand.take());
            if result.is_ok() { result = released; }
        }
        result
    })
}

/// Gives a statement consumer one owned lease for a materialized value and releases it on every exit.
pub(in crate::interpreter) fn with_eval_value_lease<V: RuntimeValueOps>(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    consume: impl FnOnce(RuntimeCellHandle, &mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let value = if value.is_borrowed() { values.retain(value)? } else { value };
    let owner = RuntimeOwnerSlot::new(value);
    with_runtime_owners(&owner, || {
        let result = consume(value, context, values);
        let released = eval_release_value(context, values, owner.take());
        result.and(released)
    })
}

/// Applies condition semantics separately from explicit casts and builtin data filtering.
pub(in crate::interpreter) fn eval_boolean_condition(
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Bools)
        && values.type_tag(value)? != EVAL_TAG_BOOL {
        return throwables::eval_throw_type_error("--strict-bools requires bool in a condition", context, values);
    }
    values.truthy(value)
}
