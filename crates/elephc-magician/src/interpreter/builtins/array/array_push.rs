//! Purpose:
//! Declarative eval registry entry for `array_push`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

mod builders;

eval_builtin! {
    contract: "array_push",
    area: Array,
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_push` array mutator.
pub(in crate::interpreter) fn eval_array_push_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((array, inserted)) = evaluated_args.split_first() else { return Err(EvalStatus::RuntimeFatal); };
    super::array_pop::eval_warn_array_by_value("array_push", values)?;
    eval_array_push_unshift_count_result(*array, inserted.len(), values)
}

/// Returns the resulting element count for by-value push/unshift dynamic calls.
pub(in crate::interpreter) fn eval_array_push_unshift_count_result(
    array: RuntimeCellHandle,
    inserted_len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let total = values
        .array_len(array)?
        .checked_add(inserted_len)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let total = i64::try_from(total).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(total)
}

/// Builds the replacement array for direct push/unshift write-back.
pub(in crate::interpreter) fn eval_array_push_unshift_replacement(
    name: &str, array: RuntimeCellHandle, inserted: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    builders::build(name, array, inserted, context, values)
}

/// Evaluates direct by-reference `array_push()` / `array_unshift()` calls and writes back the array.
pub(in crate::interpreter) fn eval_array_push_unshift_declared_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 || !eval_call_args_are_plain_positional(args) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (array, target) = super::mutation::eval_array_mutation_lvalue_arg(&args[0], context, scope, values)?;
    let mut inserted = Vec::with_capacity(args.len() - 1);
    for arg in &args[1..] {
        inserted.push(eval_expr(arg.value(), context, scope, values)?);
    }

    let replacement = eval_array_push_unshift_replacement(name, array, &inserted, context, values)?;
    let result = eval_array_push_unshift_count_result(array, inserted.len(), values)?;
    eval_write_direct_ref_target(&target, replacement, context, values, None)?;
    Ok(result)
}
