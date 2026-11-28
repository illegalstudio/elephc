//! Purpose:
//! Declarative eval registry entry and implementation for `stream_bucket_prepend`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Prepends bucket objects to the brigade `_buckets` array used by eval filters.

eval_builtin! {
    contract: "stream_bucket_prepend",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use crate::interpreter::builtins::collection_builder::{with_collection_cells, EvalArrayBuilder};
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
    with_collection_cells(values, |values, owners| {
        let buckets = owners.stage(super::stream_bucket_append::eval_brigade_buckets(brigade, values)?);
        let buckets = owners.stage(eval_bucket_prepend(buckets, bucket, values)?);
        values.property_set(brigade, "_buckets", buckets)
    })?;
    values.null()
}

/// Builds a new bucket array with the provided bucket at index zero.
fn eval_bucket_prepend(
    buckets: RuntimeCellHandle,
    bucket: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(buckets)?;
    let mut result = EvalArrayBuilder::indexed(values, len + 1)?;
    result.index(0, |_| Ok(bucket.borrowed()))?;
    for index in 0..len {
        result.update(|values, owners, _| {
            let old_key = owners.stage(values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?);
            let value = owners.stage(values.array_get(buckets, old_key)?);
            let key = owners.stage(values.int(i64::try_from(index + 1).map_err(|_| EvalStatus::RuntimeFatal)?)?);
            Ok(Some((key, value)))
        })?;
    }
    Ok(result.finish())
}
