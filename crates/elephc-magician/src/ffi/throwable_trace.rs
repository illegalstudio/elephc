//! Purpose:
//! Supplies the `getTrace()` array of a Throwable that COMPILED code creates while interpreter
//! frames are live.
//!
//! Called from:
//! - The generated runtime, through the pointer installed in `_elephc_eval_throwable_trace_fn`,
//!   from `__rt_throwable_capture_trace` right after a compiled `new` allocates a Throwable.
//!
//! Key details:
//! - A compiled Throwable's `getTrace()` used to be a constant `[]`. Code that locates its caller
//!   through a fresh exception's trace -- Twig's `Error::guessTemplateInfo()` and Symfony UX's
//!   `DeterministicTwigIdCalculator`, which match a frame's `file` against a compiled template's
//!   file and map its `line` through `getDebugInfo()` -- found nothing and threw. Twig templates
//!   are exactly the code the interpreter runs, so their frames are the ones it records.
//! - The callback returns NULL when no interpreter frame is live. That is decided from the
//!   thread-local frame stack alone, before any context is touched, so a program that creates
//!   exceptions in compiled code only pays one call and one borrow per `new`.

use crate::context::eval_call_frames_are_empty;
use crate::interpreter::eval_throwable_trace_value;
use crate::runtime_hooks::{self, ElephcRuntimeOps};
use crate::abi::ElephcEvalContext;
use crate::value::RuntimeCell;
use std::sync::OnceLock;

/// Address of the leaked context `trace_context` hands out.
static TRACE_CONTEXT: OnceLock<usize> = OnceLock::new();

/// Returns the process-lifetime context the trace builder anchors its value adapter on.
fn trace_context() -> &'static ElephcEvalContext {
    let address = *TRACE_CONTEXT
        .get_or_init(|| Box::into_raw(Box::new(ElephcEvalContext::new())) as usize);
    // SAFETY: the box above is leaked for the life of the process and only read here.
    unsafe { &*(address as *const ElephcEvalContext) }
}

/// Installs the eval Throwable-trace callback into the generated runtime.
pub(crate) fn install_throwable_trace_hook() {
    unsafe {
        runtime_hooks::install_throwable_trace_hook(
            __elephc_eval_throwable_trace as *const () as usize,
        );
    }
}

/// Builds the trace array for a Throwable being created now.
///
/// Returns a boxed Mixed array the caller owns and must release, or null when there is nothing
/// to record -- in which case the Throwable keeps its declared empty trace.
#[no_mangle]
pub extern "C" fn __elephc_eval_throwable_trace() -> *mut RuntimeCell {
    std::panic::catch_unwind(|| {
        if eval_call_frames_are_empty() {
            return std::ptr::null_mut();
        }
        // The trace reads the thread-local frame stack and builds plain arrays of strings and
        // integers, so it needs a context only as the value adapter's anchor -- never its class
        // or function tables. A private one avoids re-publishing the AOT metadata into the
        // shared null-handle context on every `new`.
        let context = trace_context();
        let mut values = ElephcRuntimeOps::with_context(context as *const _);
        match eval_throwable_trace_value(context, &mut values) {
            Ok(Some(trace)) => trace.as_ptr(),
            _ => std::ptr::null_mut(),
        }
    })
    .unwrap_or(std::ptr::null_mut())
}
