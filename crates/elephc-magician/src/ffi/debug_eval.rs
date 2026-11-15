//! Purpose:
//! Exports the opt-in native-debugger eval renderer. The debug runtime executes
//! a Magician fragment and asks this bridge to render the owned result cell as
//! bounded PHP-style debug text.
//!
//! Called from:
//! - The generated debugger eval hook table in `--debug --allow-eval` builds.
//!
//! Key details:
//! - The result cell is consumed: this function releases it after rendering,
//!   including on formatting failure.
//! - Output is truncated to the caller's buffer; the return value is the byte
//!   count actually written.

use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::interpreter::{debug_eval_render_value, RuntimeValueOps};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::RuntimeCellHandle;

/// Renders one eval result cell into `out_ptr` and releases it.
///
/// # Safety
/// `ctx` must be a live eval context, `result` must point to writable result
/// storage, and `out_ptr` must be writable for `out_len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_debug_eval_render(
    ctx: *mut ElephcEvalContext,
    result: *mut ElephcEvalResult,
    out_ptr: *mut u8,
    out_len: u64,
) -> u64 {
    std::panic::catch_unwind(|| unsafe {
        let Some(context) = ctx.as_mut() else {
            return 0;
        };
        let Some(result) = result.as_mut() else {
            return 0;
        };
        if context.abi_version() != ABI_VERSION {
            return 0;
        }
        let cell = if result.kind == 3 {
            result.error
        } else {
            result.value_cell
        };
        if cell.is_null() {
            return 0;
        }
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        let handle = RuntimeCellHandle::from_raw(cell.cast());
        let rendered = if result.kind == 3 {
            match throwable_text(&mut values, handle) {
                Some(text) => Ok(text),
                None => debug_eval_render_value(handle, context, &mut values),
            }
        } else {
            debug_eval_render_value(handle, context, &mut values)
        };
        let _ = values.release(handle);
        result.value_cell = std::ptr::null_mut();
        result.error = std::ptr::null_mut();
        let Ok(bytes) = rendered else {
            return 0;
        };
        let Some(out_len) = usize::try_from(out_len).ok() else {
            return 0;
        };
        let written = bytes.len().min(out_len);
        if !out_ptr.is_null() && written > 0 {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, written);
        }
        written as u64
    })
    .unwrap_or(0)
}

/// Formats one thrown eval value as `Class: message` when its internal fields are readable.
fn throwable_text(
    values: &mut ElephcRuntimeOps,
    throwable: RuntimeCellHandle,
) -> Option<Vec<u8>> {
    let class_cell = values.object_class_name(throwable).ok()?;
    let class = read_string(values, class_cell)?;
    let message_cell = values.property_get(throwable, "message").ok()?;
    let message = read_string(values, message_cell)?;
    let mut text = class;
    if !message.is_empty() {
        text.extend_from_slice(b": ");
        text.extend_from_slice(&message);
    }
    Some(text)
}

/// Reads and releases one runtime string cell.
fn read_string(values: &mut ElephcRuntimeOps, cell: RuntimeCellHandle) -> Option<Vec<u8>> {
    let bytes = values.string_bytes(cell).ok();
    let _ = values.release(cell);
    bytes
}
