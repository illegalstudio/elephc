//! Purpose:
//! Declarative eval registry entry and implementation for `stream_bucket_append`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Appends bucket objects to the brigade `_buckets` array used by eval filters.

eval_builtin! {
    contract: "stream_bucket_append",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use crate::interpreter::builtins::collection_builder::{with_collection_cells, with_collection_result};
use super::super::super::*;

/// Evaluates `stream_bucket_append($brigade, $bucket)`.
pub(in crate::interpreter) fn eval_stream_bucket_append_declared_call(
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
    eval_stream_bucket_append_result(brigade, bucket, values)
}

/// Appends an already evaluated bucket to an already evaluated brigade.
pub(in crate::interpreter) fn eval_stream_bucket_append_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [brigade, bucket] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_bucket_append_result(*brigade, *bucket, values)
}

/// Adds a bucket object to the end of the brigade's `_buckets` array.
pub(in crate::interpreter) fn eval_stream_bucket_append_result(
    brigade: RuntimeCellHandle,
    bucket: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_collection_cells(values, |values, owners| {
        let buckets = owners.stage(eval_brigade_buckets(brigade, values)?);
        let len = values.array_len(buckets)?;
        let index = owners.stage(values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)?);
        let buckets = values.array_set(buckets, index, bucket)?;
        values.property_set(brigade, "_buckets", buckets)
    })?;
    values.null()
}

/// Returns an existing brigade bucket array or creates an empty one.
pub(in crate::interpreter) fn eval_brigade_buckets(
    brigade: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_collection_result(values, |values, result| {
        result.replace(Some(values.property_get(brigade, "_buckets")?));
        if !values.is_array_like(result.borrowed())? {
            values.release(result.take())?;
            result.replace(Some(values.array_new(0)?));
        }
        Ok(())
    })
}
