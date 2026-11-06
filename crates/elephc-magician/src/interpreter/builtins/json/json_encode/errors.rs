//! Purpose:
//! Separates recoverable JSON encoding failures from eval runtime failures.
//!
//! Called from:
//! - The parent JSON encoder and its object serializer helpers.
//!
//! Key details:
//! - Partial output retains encoded bytes and records the JSON error.
//! - Container depth is checked on exit, after any child encoding failure.
//! - Runtime statuses remain unchanged across the eval bridge ABI.

use super::{EvalJsonEncodeError, EvalStatus, EVAL_JSON_ERROR_DEPTH, EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR};

/// Identifies a JSON failure without converting a runtime or user exception into one.
pub(super) enum EvalJsonEncodeFailure {
    Runtime(EvalStatus),
    Json(EvalJsonEncodeError),
}

/// Carries either an encoded value or the precise reason traversal stopped.
pub(super) type EvalJsonEncodeResult<T = ()> = Result<T, EvalJsonEncodeFailure>;

impl From<EvalStatus> for EvalJsonEncodeFailure {
    /// Preserves runtime and user exception statuses independently of JSON errors.
    fn from(status: EvalStatus) -> Self {
        Self::Runtime(status)
    }
}

/// Records a JSON error and stops traversal unless partial output is requested.
pub(super) fn eval_json_encode_record_error(
    flags: i64,
    error: &mut Option<EvalJsonEncodeError>,
    failure: EvalJsonEncodeError,
) -> EvalJsonEncodeResult {
    *error = Some(failure);
    if flags & EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR == 0 {
        Err(EvalJsonEncodeFailure::Json(failure))
    } else {
        Ok(())
    }
}

/// Applies PHP's depth failure after children, including empty and partial containers.
pub(super) fn eval_json_encode_check_depth(
    depth_limit: usize,
    depth: usize,
    flags: i64,
    error: &mut Option<EvalJsonEncodeError>,
) -> EvalJsonEncodeResult {
    if depth >= depth_limit {
        eval_json_encode_record_error(flags, error, EvalJsonEncodeError {
            code: EVAL_JSON_ERROR_DEPTH,
            message: "Maximum stack depth exceeded",
        })
    } else {
        Ok(())
    }
}
