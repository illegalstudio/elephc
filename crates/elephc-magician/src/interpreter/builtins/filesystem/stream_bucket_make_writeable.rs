//! Purpose:
//! Declarative eval registry entry and implementation for `stream_bucket_make_writeable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Returns the first queued bucket from an eval brigade object.

eval_builtin! {
    contract: "stream_bucket_make_writeable",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};

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
    let owners = StagedRuntimeOwners::default();
    let output = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &output), || {
        let operation = (|| {
            let buckets = owners.stage(values.property_get(brigade, "_buckets")?);
            let result = if !values.is_array_like(buckets.borrowed())?
                || values.array_len(buckets.borrowed())? == 0
            {
                values.null()?
            } else {
                let key = owners.stage(values.int(0)?);
                values.array_get(buckets.borrowed(), key.borrowed())?
            };
            output.replace(Some(result));
            Ok(())
        })();
        owners.finish(operation, &output, |cell| values.release(cell))
    })
}
