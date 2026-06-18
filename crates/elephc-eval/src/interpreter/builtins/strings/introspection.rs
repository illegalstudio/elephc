//! Purpose:
//! Hash algorithm, SPL class, and stream introspection list builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and string bytes are obtained through `RuntimeValueOps`.

use super::super::super::*;

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

/// Builds one indexed PHP array from a static string slice.
pub(in crate::interpreter) fn eval_static_string_array_result(
    items: &[&str],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(items.len())?;
    for (index, item) in items.iter().enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string(item)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `spl_classes()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_spl_classes(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_spl_classes_result(values)
}

/// Builds the static class-name list returned by eval `spl_classes()`.
pub(in crate::interpreter) fn eval_spl_classes_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_SPL_CLASS_NAMES, values)
}

/// Evaluates PHP stream introspection list builtins with no arguments.
pub(in crate::interpreter) fn eval_builtin_stream_introspection(
    name: &str,
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_introspection_result(name, values)
}

/// Builds the static list returned by one eval stream introspection builtin.
pub(in crate::interpreter) fn eval_stream_introspection_result(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let items = match name {
        "stream_get_filters" => EVAL_STREAM_FILTERS,
        "stream_get_transports" => EVAL_STREAM_TRANSPORTS,
        "stream_get_wrappers" => EVAL_STREAM_WRAPPERS,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_static_string_array_result(items, values)
}
