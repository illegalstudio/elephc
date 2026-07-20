//! Purpose:
//! Owns the process-local registry that lets the generated runtime invoke
//! eval-registered `ob_start()` output handlers on buffer flush/clean events.
//!
//! Called from:
//! - The eval `ob_start` builtin when it registers a handler callable.
//! - The generated runtime through the installed ob-handler hook pointer
//!   (`__rt_ob_eval_trampoline` → `__elephc_eval_ob_handler`).
//!
//! Key details:
//! - Entries store the owning context and the retained handler cell as integer
//!   addresses so the global mutex stays `Sync`; the hook revalidates the
//!   pointers and the ABI version before re-entering the interpreter.
//! - The hook returns a retained Mixed result cell (the runtime unboxes it,
//!   maps `false` to pass-through, and releases it) or null for pass-through.
//! - Registry entries live for the process lifetime: buffers can be popped from
//!   either side of the bridge, so the id → callable mapping is never reused.

#[cfg(not(test))]
use crate::abi::ABI_VERSION;
use crate::abi::ElephcEvalContext;
#[cfg(not(test))]
use crate::interpreter::eval_ob_handler_callback;
#[cfg(not(test))]
use crate::runtime_hooks::{self, ElephcRuntimeOps};
#[cfg(not(test))]
use crate::value::RuntimeCell;
use crate::value::RuntimeCellHandle;
use std::sync::{Mutex, OnceLock};

/// One registered eval output handler: the owning context and the retained
/// handler callable, both stored as raw addresses.
struct ObHandlerEntry {
    context: usize,
    callback: usize,
}

static OB_HANDLERS: OnceLock<Mutex<Vec<ObHandlerEntry>>> = OnceLock::new();

/// Returns the process-local eval output-handler registry.
fn ob_handlers() -> &'static Mutex<Vec<ObHandlerEntry>> {
    OB_HANDLERS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Installs the eval output-handler callback into the generated runtime.
#[cfg(not(test))]
pub(crate) fn install_ob_handler_hook() {
    unsafe {
        runtime_hooks::install_ob_handler_hook(__elephc_eval_ob_handler as *const () as usize);
    }
}

/// Registers one already-retained handler callable and returns its registry id.
pub(crate) fn register_ob_handler(
    context: *mut ElephcEvalContext,
    callback: RuntimeCellHandle,
) -> Option<u64> {
    let mut handlers = ob_handlers().lock().ok()?;
    handlers.push(ObHandlerEntry {
        context: context as usize,
        callback: callback.as_ptr() as usize,
    });
    Some(handlers.len() as u64 - 1)
}

/// Invalidates every handler registered by a soon-to-be-freed context so the
/// runtime hook can never re-enter the interpreter through a dangling pointer.
pub(crate) fn unregister_ob_handlers_for_context(context: *mut ElephcEvalContext) {
    if context.is_null() {
        return;
    }
    let context = context as usize;
    if let Ok(mut handlers) = ob_handlers().lock() {
        for entry in handlers.iter_mut() {
            if entry.context == context {
                entry.context = 0;
                entry.callback = 0;
            }
        }
    }
}

/// Looks up one registered handler's owning context and callable cell.
#[cfg(not(test))]
fn ob_handler_entry(id: u64) -> Option<(usize, usize)> {
    let handlers = ob_handlers().lock().ok()?;
    let entry = handlers.get(usize::try_from(id).ok()?)?;
    Some((entry.context, entry.callback))
}

/// Runs one eval-registered output handler from the runtime flush path.
///
/// # Safety
/// `buf` must point to `len` readable bytes (the raw buffer contents). The
/// runtime calls this only through `__rt_ob_eval_trampoline` while
/// `_ob_in_handler` is set, so handler output is discarded and nested
/// `ob_start()` calls are fatal.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_ob_handler(
    id: i64,
    buf: *const u8,
    len: i64,
    phase: i64,
) -> *mut RuntimeCell {
    std::panic::catch_unwind(|| unsafe { ob_handler_inner(id, buf, len, phase) })
        .unwrap_or(std::ptr::null_mut())
}

/// Executes the hook body after the exported ABI shim installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_ob_handler`.
#[cfg(not(test))]
unsafe fn ob_handler_inner(id: i64, buf: *const u8, len: i64, phase: i64) -> *mut RuntimeCell {
    let Ok(id) = u64::try_from(id) else {
        return std::ptr::null_mut();
    };
    let Some((context, callback)) = ob_handler_entry(id) else {
        return std::ptr::null_mut();
    };
    let context = context as *mut ElephcEvalContext;
    let Some(context) = (unsafe { context.as_mut() }) else {
        return std::ptr::null_mut();
    };
    if context.abi_version() != ABI_VERSION {
        return std::ptr::null_mut();
    }
    if callback == 0 {
        return std::ptr::null_mut();
    }
    let callback = RuntimeCellHandle::from_raw(callback as *mut RuntimeCell);
    let bytes = if len <= 0 || buf.is_null() {
        &[]
    } else {
        let Ok(len) = usize::try_from(len) else {
            return std::ptr::null_mut();
        };
        unsafe { std::slice::from_raw_parts(buf, len) }
    };
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match eval_ob_handler_callback(callback, bytes, phase, context, &mut values) {
        Ok(result) => result.as_ptr(),
        Err(_) => std::ptr::null_mut(),
    }
}
