//! Purpose:
//! Declarative eval registry entry and implementation for `stream_bucket_prepend`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Prepends bucket objects to the brigade `_buckets` array used by eval filters.

eval_builtin! {
    name: "stream_bucket_prepend",
    area: Filesystem,
    params: [brigade, bucket],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_bucket_prepend($brigade, $bucket)`.
pub(in crate::interpreter) fn eval_stream_bucket_prepend_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [brigade, bucket] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let brigade = eval_expr(brigade, context, scope, values)?;
    let bucket = eval_expr(bucket, context, scope, values)?;
    eval_stream_bucket_prepend_result(brigade, bucket, values)
}

/// Prepends an already evaluated bucket to an already evaluated brigade.
pub(in crate::interpreter) fn eval_stream_bucket_prepend_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [brigade, bucket] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_bucket_prepend_result(*brigade, *bucket, values)
}

/// Adds a bucket object to the front of the brigade's `_buckets` array.
pub(in crate::interpreter) fn eval_stream_bucket_prepend_result(
    brigade: RuntimeCellHandle,
    bucket: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let buckets = super::stream_bucket_append::eval_brigade_buckets(brigade, values)?;
    let buckets = eval_bucket_prepend(buckets, bucket, values)?;
    values.property_set(brigade, "_buckets", buckets)?;
    values.null()
}

/// Builds a new bucket array with the provided bucket at index zero.
fn eval_bucket_prepend(
    buckets: RuntimeCellHandle,
    bucket: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(buckets)?;
    let mut result = values.array_new(len + 1)?;
    let zero = values.int(0)?;
    result = values.array_set(result, zero, bucket)?;
    for index in 0..len {
        let old_key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = values.array_get(buckets, old_key)?;
        let new_key =
            values.int(i64::try_from(index + 1).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, new_key, value)?;
    }
    Ok(result)
}
