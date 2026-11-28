//! Purpose:
//! Transfers protected native Throwable results into the interpreter's existing catch state.
//!
//! Called from:
//! - Checked arithmetic, unary operations and explicit object-cast adapters.
//!
//! Key details:
//! - Only the Throwable failure status consumes the native pending owner.
//! - Successful values and unsupported requests retain their original ownership contracts.

use super::*;

/// Adopts one protected native failure while preserving successful results and other statuses.
pub(super) fn finish<T>(result: Result<T, EvalStatus>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<T, EvalStatus> {
    match result {
        Err(EvalStatus::UncaughtThrowable) => {
            if let Some(thrown) = values.take_pending_runtime_throwable()? { context.set_pending_throw(thrown); }
            Err(EvalStatus::UncaughtThrowable)
        }
        result => result,
    }
}
