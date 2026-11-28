//! Purpose:
//! Declarative eval registry entry for `hash_algos`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - The static string-array helper is shared by list-returning string builtins.

eval_builtin! {
    contract: "hash_algos",
    area: String,
    direct: HashAlgos,
    values: HashAlgos,
}

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

/// Evaluates PHP `hash_algos()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_hash_algos(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_hash_algos_result(values)
}

/// Builds the indexed array returned by eval `hash_algos()`.
pub(in crate::interpreter) fn eval_hash_algos_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_HASH_ALGOS, values)
}

/// Dispatches evaluated `hash_algos()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_hash_algos_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_hash_algos_result(values)
}

/// Builds one indexed PHP array from a static string slice.
pub(in crate::interpreter) fn eval_static_string_array_result(
    items: &[&str],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::indexed(values, items.len())?;
    for (index, item) in items.iter().enumerate() {
        result.index(index, |values| values.string(item))?;
    }
    Ok(result.finish())
}
