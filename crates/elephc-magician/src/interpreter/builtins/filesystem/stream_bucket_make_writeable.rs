//! Purpose:
//! Declarative eval registry entry and implementation for `stream_bucket_make_writeable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Returns the first queued bucket from an eval brigade object.

eval_builtin! {
    name: "stream_bucket_make_writeable",
    area: Filesystem,
    params: [brigade],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_bucket_make_writeable($brigade)`.
pub(in crate::interpreter) fn eval_stream_bucket_make_writeable_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [brigade] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let brigade = eval_expr(brigade, context, scope, values)?;
    eval_stream_bucket_make_writeable_result(brigade, values)
}

/// Returns the first bucket from an already evaluated brigade argument.
pub(in crate::interpreter) fn eval_stream_bucket_make_writeable_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [brigade] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_bucket_make_writeable_result(*brigade, values)
}

/// Returns the first bucket in a brigade, or null when none exists.
pub(in crate::interpreter) fn eval_stream_bucket_make_writeable_result(
    brigade: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let buckets = values.property_get(brigade, "_buckets")?;
    if !values.is_array_like(buckets)? || values.array_len(buckets)? == 0 {
        return values.null();
    }
    let key = values.int(0)?;
    values.array_get(buckets, key)
}
