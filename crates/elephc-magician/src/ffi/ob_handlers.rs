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
//! - Registry ids are never reused. Buffer closure and context teardown detach owners
//!   before releasing them outside the registry lock; closed entries do not accumulate.

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
use std::{collections::HashMap, sync::{Mutex, OnceLock}};

/// One registered eval output handler: the owning context and the retained
/// handler callable, both stored as raw addresses.
#[derive(Clone, Copy)]
struct ObHandlerEntry {
    context: usize,
    callback: usize,
}

/// Active registrations and a monotonic identity counter independent of storage indices.
#[derive(Default)]
struct ObHandlers {
    next: u64,
    entries: HashMap<u64, ObHandlerEntry>,
}

impl ObHandlers {
    /// Registers an owner with an identity representable by the native signed environment word.
    fn register(&mut self, entry: ObHandlerEntry) -> Option<u64> {
        if self.next > i64::MAX as u64 { return None; }
        let id = self.next;
        self.next += 1;
        self.entries.insert(id, entry);
        Some(id)
    }

    /// Detaches every owner of a retiring context without retaining tombstones.
    fn take_context(&mut self, context: usize) -> Vec<ObHandlerEntry> {
        let mut detached = Vec::new();
        self.entries.retain(|_, entry| {
            if entry.context != context { return true; }
            detached.push(*entry);
            false
        });
        detached
    }
}

static OB_HANDLERS: OnceLock<Mutex<ObHandlers>> = OnceLock::new();

/// Returns the process-local eval output-handler registry.
fn ob_handlers() -> &'static Mutex<ObHandlers> {
    OB_HANDLERS.get_or_init(|| Mutex::new(ObHandlers::default()))
}

/// Installs the eval output-handler callback into the generated runtime.
#[cfg(not(test))]
pub(crate) fn install_ob_handler_hook() {
    unsafe {
        runtime_hooks::install_ob_handler_hook(__elephc_eval_ob_handler_v1 as *const () as usize,
            __elephc_eval_ob_release_handler as *const () as usize);
    }
}

/// Registers one already-retained handler callable and returns its registry id.
pub(crate) fn register_ob_handler(
    context: *mut ElephcEvalContext,
    callback: RuntimeCellHandle,
) -> Option<u64> {
    if context.is_null() || callback.as_ptr().is_null() { return None; }
    let mut handlers = ob_handlers().lock().ok()?;
    handlers.register(ObHandlerEntry {
        context: context as usize,
        callback: callback.as_ptr() as usize,
    })
}

/// Returns a failed start's retained owner after checking its original eval context.
pub(crate) fn unregister_ob_handler(id: u64, context: *mut ElephcEvalContext) -> Option<RuntimeCellHandle> {
    let mut handlers = ob_handlers().lock().ok()?;
    if handlers.entries.get(&id)?.context != context as usize { return None; }
    let entry = handlers.entries.remove(&id)?;
    Some(RuntimeCellHandle::from_raw(entry.callback as *mut crate::value::RuntimeCell))
}

/// Invalidates a retiring context's entries and transfers their retained owners for protected cleanup.
pub(crate) fn unregister_ob_handlers_for_context(context: *mut ElephcEvalContext) -> Vec<RuntimeCellHandle> {
    if context.is_null() { return Vec::new(); }
    let Ok(mut handlers) = ob_handlers().lock() else { return Vec::new(); };
    handlers.take_context(context as usize).into_iter().map(|entry|
        RuntimeCellHandle::from_raw(entry.callback as *mut crate::value::RuntimeCell)).collect()
}

/// Looks up one registered handler's owning context and callable cell.
#[cfg(not(test))]
fn ob_handler_entry(id: u64) -> Option<(usize, usize)> {
    let handlers = ob_handlers().lock().ok()?;
    let entry = handlers.entries.get(&id)?;
    Some((entry.context, entry.callback))
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
        let entry = {
            let Ok(mut handlers) = ob_handlers().lock() else { return 1; };
            handlers.entries.remove(&id)
        };
        let Some(entry) = entry else { return 0; };
        let Some(context) = (unsafe { (entry.context as *mut ElephcEvalContext).as_mut() }) else { return 1; };
        if context.abi_version() != ABI_VERSION { return 1; }
        let callback = RuntimeCellHandle::from_raw(entry.callback as *mut RuntimeCell);
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        match crate::interpreter::release_ob_handler_callbacks(vec![callback], context, &mut values) {
            Ok(()) => 0,
            Err(_) => match context.take_pending_throw() {
                Some(owner) => { unsafe { *thrown = owner.as_ptr(); } 2 },
                None => 1,
            },
        }
    })).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Closed registrations discard their storage without ever reusing a stale buffer identity.
    #[test]
    fn output_handler_registry_retires_entries_without_reusing_ids() {
        let mut handlers = ObHandlers::default();
        for expected in 0..1024 {
            let id = handlers.register(ObHandlerEntry { context: 1, callback: 2 }).unwrap();
            assert_eq!(id, expected);
            assert_eq!(handlers.entries.remove(&id).unwrap().callback, 2);
            assert!(handlers.entries.is_empty());
            assert!(!handlers.entries.contains_key(&id));
        }
        assert!(handlers.entries.capacity() < 16);
    }

    /// Context retirement removes only its own live registrations and transfers each owner once.
    #[test]
    fn output_handler_registry_detaches_context_owners_once() {
        let mut handlers = ObHandlers::default();
        handlers.register(ObHandlerEntry { context: 1, callback: 11 });
        handlers.register(ObHandlerEntry { context: 2, callback: 22 });
        handlers.register(ObHandlerEntry { context: 1, callback: 33 });
        let mut detached: Vec<_> = handlers.take_context(1).into_iter().map(|entry| entry.callback).collect();
        detached.sort_unstable();
        assert_eq!(detached, [11, 33]);
        assert!(handlers.take_context(1).is_empty());
        assert_eq!(handlers.entries.len(), 1);
        assert_eq!(handlers.take_context(2)[0].callback, 22);
        assert!(handlers.entries.is_empty());
    }
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
    let Some((context, callback)) = ob_handler_entry(request.id) else {
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
