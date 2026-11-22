//! Purpose:
//! Owns the process-local registry that lets the generated runtime invoke
//! eval-registered `ob_start()` output handlers on buffer flush/clean events.
//!
//! Called from:
//! - The eval `ob_start` builtin when it registers a handler callable.
//! - The generated runtime through the installed ob-handler hook pointer
//!   (`__rt_ob_eval_trampoline` → `__elephc_eval_ob_handler_v1`).
//!
//! Key details:
//! - Entries store the owning context and the retained handler cell as integer
//!   addresses so the global mutex stays `Sync`; the hook revalidates the
//!   pointers and the ABI version before re-entering the interpreter.
//! - The hook transfers a retained Mixed result or pending Throwable through separate
//!   request fields; native code propagates PHP exceptions only after Rust returns.
//! - Registry ids are never reused. Buffer closure detaches and releases both owners
//!   outside the lock, even when the buffer outlives the eval frame.

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
#[cfg(not(test))]
use elephc_builtin_contract::output_abi::OutputHandlerCallV1;
use elephc_memoir::output_handlers::{self, OutputHandler};

/// Installs the eval output-handler callback into the generated runtime.
#[cfg(not(test))]
pub(crate) fn install_ob_handler_hook() {
    unsafe {
        runtime_hooks::install_ob_handler_hook(__elephc_eval_ob_handler_v1 as *const () as usize,
            __elephc_eval_ob_release_handler as *const () as usize);
    }
}

/// Registers an already-retained callable and keeps its eval context alive for the buffer.
pub(crate) fn register_ob_handler(
    context: &mut ElephcEvalContext,
    callback: RuntimeCellHandle,
) -> Option<u64> {
    if callback.is_null() {
        return None;
    }
    context.retain_abi_owner().ok()?;
    let id = output_handlers::register(OutputHandler {
        context: std::ptr::from_mut(context) as usize,
        callback: callback.as_ptr() as usize,
    });
    if id.is_none() {
        context.release_abi_owner();
    }
    id
}

/// Rolls back a failed start and returns the detached callable owner for explicit cleanup.
pub(crate) fn unregister_ob_handler(id: u64, context: &mut ElephcEvalContext) -> Option<RuntimeCellHandle> {
    let entry = output_handlers::take(id, Some(std::ptr::from_mut(context) as usize)).ok()??;
    context.release_abi_owner();
    Some(RuntimeCellHandle::from_raw(entry.callback as *mut crate::value::RuntimeCell))
}

/// Detaches and releases a closed buffer's callback, returning any owned PHP Throwable to native code.
///
/// # Safety
/// `thrown` points to writable boxed-owner storage. The runtime has removed this buffer
/// and released its byte/name storage before entering Rust. Registered contexts remain
/// live until their entries have been detached. PHP unwinding occurs only after return.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_ob_release_handler(id: u64, thrown: *mut *mut RuntimeCell) -> u64 {
    if thrown.is_null() { return 1; }
    unsafe { *thrown = std::ptr::null_mut(); }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Ok(entry) = output_handlers::take(id, None) else { return 1; };
        let Some(entry) = entry else { return 0; };
        let context_ptr = entry.context as *mut ElephcEvalContext;
        let Some(context) = (unsafe { context_ptr.as_mut() }) else { return 1; };
        if context.abi_version() != ABI_VERSION { return 1; }
        let callback = RuntimeCellHandle::from_raw(entry.callback as *mut RuntimeCell);
        let mut values = ElephcRuntimeOps::with_context(context_ptr);
        let failed = crate::interpreter::release_ob_handler_callbacks(vec![callback], context, &mut values).is_err();
        let pending = if failed { context.take_pending_throw() } else { None };
        // The buffer owns this lease even if the function that registered it already returned.
        let pending = unsafe { super::context::release_eval_context(context_ptr, pending) };
        if let Some(owner) = pending {
            unsafe { *thrown = owner.as_ptr(); }
            2
        } else {
            u64::from(failed)
        }
    })).unwrap_or(1)
}

/// Runs one eval output handler and transfers its result or Throwable without unwinding through Rust.
///
/// # Safety
/// `request` points to an exclusive writable version-one record. Its byte span remains
/// readable for this call, and registered contexts remain live until detached. Native
/// code sets the output-handler guard and owns both returned cells after this call.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_ob_handler_v1(request: *mut OutputHandlerCallV1) -> u64 {
    let Some(request) = (unsafe { request.as_mut() }) else { return 1; };
    request.result = std::ptr::null_mut();
    request.thrown = std::ptr::null_mut();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { ob_handler_inner(request) }))
        .unwrap_or(1)
}

/// Executes the hook body after the exported ABI shim installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_ob_handler_v1`.
#[cfg(not(test))]
unsafe fn ob_handler_inner(request: &mut OutputHandlerCallV1) -> u64 {
    let Some(OutputHandler { context, callback }) = output_handlers::get(request.id) else {
        return 0;
    };
    let context = context as *mut ElephcEvalContext;
    let Some(context) = (unsafe { context.as_mut() }) else {
        return 1;
    };
    if context.abi_version() != ABI_VERSION {
        return 1;
    }
    if callback == 0 {
        return 1;
    }
    let callback = RuntimeCellHandle::from_raw(callback as *mut RuntimeCell).borrowed();
    let bytes = if request.length == 0 {
        &[]
    } else {
        let Ok(len) = usize::try_from(request.length) else { return 1; };
        if request.bytes.is_null() || len > isize::MAX as usize { return 1; }
        unsafe { std::slice::from_raw_parts(request.bytes, len) }
    };
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match eval_ob_handler_callback(callback, bytes, request.phase, context, &mut values) {
        Ok(result) => { request.result = result.as_ptr().cast(); 0 },
        Err(_) => match context.take_pending_throw() {
            Some(owner) => { request.thrown = owner.as_ptr().cast(); 2 },
            None => 1,
        },
    }
}
