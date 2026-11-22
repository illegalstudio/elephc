//! Purpose:
//! Adapts Magician's PHP buffer builtins to the Memoir eval registry.
//!
//! Called from:
//! - The sibling `buffer_new`, `buffer_len`, `buffer_ptr`, and `buffer_free` builtins.
//!
//! Key details:
//! - Magician decodes runtime cells and reports PHP diagnostics.
//! - Memoir owns allocation, lookup and retirement for the thread's eval handles.

use elephc_memoir::eval_buffers::{self, BufferError, EvalBufferView};

use super::super::super::*;

/// Fatal diagnostic compiled code prints for every use of a freed or invalid buffer handle.
const EVAL_BUFFER_USE_AFTER_FREE: &str = "Fatal error: use of buffer after buffer_free()\n";

/// Maps registry failures to the existing PHP behavior.
fn eval_buffer_result<T>(
    result: Result<T, BufferError>, values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    match result {
        Ok(value) => Ok(value),
        Err(BufferError::InvalidHandle) => {
            values.fatal(EVAL_BUFFER_USE_AFTER_FREE)?;
            Err(EvalStatus::RuntimeFatal)
        }
        Err(BufferError::Unavailable) => Err(EvalStatus::RuntimeFatal),
    }
}

/// Allocates a zeroed payload and returns its eval handle.
pub(super) fn eval_buffer_allocate(length: usize) -> Result<i64, EvalStatus> {
    eval_buffers::allocate(length).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Answers the live buffer behind an eval buffer value, or dies with the compiled-code
/// use-after-free fatal for a freed, stale, null, or foreign handle.
pub(super) fn eval_buffer_resolve(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalBufferView, EvalStatus> {
    let handle = eval_int_value(buffer, values)?;
    eval_buffer_result(eval_buffers::resolve(handle), values)
}

/// Releases an eval buffer or reports the same fatal for an invalid handle.
pub(super) fn eval_buffer_release(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let handle = eval_int_value(buffer, values)?;
    eval_buffer_result(eval_buffers::release(handle), values)
}
