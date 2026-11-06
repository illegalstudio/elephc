//! Purpose:
//! Retires owned JSON callback, property and collection-read results.
//!
//! Called from:
//! - The parent JSON encoder and its object traversal helpers.
//!
//! Key details:
//! - Cleanup runs even when encoding stops with a recoverable JSON error.
//! - Cleanup exceptions remain runtime failures and borrowed cells add no release.

use super::{
    ElephcEvalContext, EvalJsonEncodeResult, RuntimeCellHandle, RuntimeValueOps, eval_release_value,
};

/// Releases one owned read after its result is captured, preserving cleanup exceptions.
pub(super) fn release_encoded_value<T>(
    value: RuntimeCellHandle,
    encoded: EvalJsonEncodeResult<T>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> EvalJsonEncodeResult<T> {
    if !value.is_borrowed() {
        eval_release_value(context, values, value)?;
    }
    encoded
}
