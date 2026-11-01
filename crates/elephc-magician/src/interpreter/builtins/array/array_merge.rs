//! Purpose:
//! Declarative eval registry entry for `array_merge`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    contract: "array_merge",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_merge` array builtin.
pub(in crate::interpreter) fn eval_array_merge_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_merge(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_merge` array builtin.
pub(in crate::interpreter) fn eval_array_merge_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_array_merge_all(evaluated_args, values)
}

/// Evaluates PHP `array_merge()` over any number of array expressions.
pub(in crate::interpreter) fn eval_builtin_array_merge(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut operands = Vec::with_capacity(args.len());
    for arg in args {
        operands.push(eval_expr(arg, context, scope, values)?);
    }
    eval_array_merge_all(&operands, values)
}

/// Builds an `array_merge()` result with PHP numeric reindexing and string-key overwrites.
///
/// `array_merge()` is VARIADIC -- `array_merge(...$arrays)` -- and both entry points used to
/// destructure exactly `[left, right]`, so a third argument failed the pattern and the call
/// surfaced as `unsupported NamespacedCall array_merge()` with no further explanation. MEASURED
/// against php 8.5.10 inside `eval()`: two arguments matched, three did not. Symfony's
/// `TwigEnvironmentPass` writes `array_merge($a, $b, $currentMethodCalls)`, and that one line
/// stopped the console's container build.
///
/// No arguments is `[]` in php 8, which falls out of the empty fold.
pub(in crate::interpreter) fn eval_array_merge_all(
    operands: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut capacity: usize = 0;
    for operand in operands {
        capacity = capacity
            .checked_add(values.array_len(*operand)?)
            .ok_or(EvalStatus::RuntimeFatal)?;
    }
    let mut result = values.assoc_new(capacity)?;
    let mut next_numeric_key = 0_i64;
    for operand in operands {
        result = eval_array_merge_append_operand(result, *operand, &mut next_numeric_key, values)?;
    }
    Ok(result)
}

/// Appends one source array to an `array_merge()` result using PHP key handling.
pub(in crate::interpreter) fn eval_array_merge_append_operand(
    mut result: RuntimeCellHandle,
    source: RuntimeCellHandle,
    next_numeric_key: &mut i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(source)?;
    for position in 0..len {
        let source_key = values.array_iter_key(source, position)?;
        let source_value = values.array_get(source, source_key)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_STRING {
            source_key
        } else {
            let target_key = values.int(*next_numeric_key)?;
            *next_numeric_key = (*next_numeric_key)
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            target_key
        };
        result = values.array_set(result, target_key, source_value)?;
    }
    Ok(result)
}
