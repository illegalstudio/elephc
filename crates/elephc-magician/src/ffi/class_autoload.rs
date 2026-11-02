//! Purpose:
//! Runs the SPL autoload chain for a class name the generated runtime could not resolve.
//!
//! Called from:
//! - The generated runtime, through the pointer installed in `_elephc_eval_class_autoload_fn`.
//!
//! Key details:
//! - `unserialize()` is the caller that needs it. PHP does not degrade an unknown class to
//!   `__PHP_Incomplete_Class` on sight: it calls `unserialize_callback_func` first, whose whole
//!   purpose is to give the autoloader a chance, and Symfony points that ini at its own
//!   autoloader — which is why its `.meta` files deserialize into real `FileResource` and
//!   `GlobResource` objects rather than into class-less stand-ins.
//! - The compiled decoder cannot call `__elephc_eval_dynamic_class_exists` directly: the
//!   unserialize runtime is emitted for EVERY program while that symbol exists only when the
//!   bridge is linked. A hook slot is what keeps the feature pay-for-use, the same shape
//!   `_elephc_eval_dynamic_object_destruct_fn` and `_elephc_eval_generator_protocol_fn` use.

use crate::ffi::util::abi_name_to_string;
use crate::interpreter::{eval_class_name_is_a_bridge, eval_spl_autoload_class_bridge};
use crate::interpreter::RuntimeValueOps;
use crate::runtime_hooks::{self, ElephcRuntimeOps};

/// Installs the eval class-autoload callback into the generated runtime.
pub(crate) fn install_class_autoload_hook() {
    unsafe {
        runtime_hooks::install_class_autoload_hook(
            __elephc_eval_class_autoload as *const () as usize,
        );
    }
}

/// Runs the registered autoloaders for one class name and reports whether it now exists.
///
/// Returns 1 when the class is declared after the chain has run, and 0 otherwise — including
/// when the name is unreadable, so an unusable argument degrades to "not found" rather than to
/// a panic crossing the ABI.
///
/// # Safety
/// `name_ptr` must be readable for `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_class_autoload(name_ptr: *const u8, name_len: u64) -> u64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_class_autoload");
    std::panic::catch_unwind(|| unsafe { class_autoload_inner(name_ptr, name_len) }).unwrap_or(0)
}

/// Answers a class-name `is_a()` / `is_subclass_of()` the compiled class table said no to.
///
/// Compiled `is_a($name, $target, true)` over two runtime strings consults
/// `__rt_class_name_is_a`, which knows the compiled classes only; a class the interpreter
/// declared -- an autoloaded file the compiler never saw -- reads as "no" there. Compiled code
/// calls this for that "no" only when the bridge is linked. Returns 1 or 0, and 0 on an
/// unreadable name or a failure, so the compiled answer stands.
///
/// # Safety
/// Each pointer must be readable for its length when the length is non-zero.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_class_name_is_a(
    source_ptr: *const u8,
    source_len: u64,
    target_ptr: *const u8,
    target_len: u64,
    exclude_self: u64,
) -> u64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_class_name_is_a");
    std::panic::catch_unwind(|| unsafe {
        let (Ok(source), Ok(target)) = (
            abi_name_to_string(source_ptr, source_len),
            abi_name_to_string(target_ptr, target_len),
        ) else {
            return 0;
        };
        if source.is_empty() || target.is_empty() {
            return 0;
        }
        // Same reasoning as the autoload hook below: no context handle reaches here, and the
        // shared null-handle context is the one holding the request's declarations.
        let context = crate::ffi::context::shared_null_handle_context();
        let context_ptr = context as *mut crate::abi::ElephcEvalContext;
        context.sync_global_eval_classes();
        let mut values = ElephcRuntimeOps::with_context(context_ptr);
        let context = &mut *context_ptr;
        let result =
            eval_class_name_is_a_bridge(&source, &target, exclude_self != 0, context, &mut values)
                .unwrap_or(false);
        u64::from(result)
    })
    .unwrap_or(0)
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_class_autoload`.
unsafe fn class_autoload_inner(name_ptr: *const u8, name_len: u64) -> u64 {
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    if name.is_empty() {
        return 0;
    }
    // The runtime reaches this with no context handle of its own, so the shared null-handle
    // context is the one that holds the request's declarations and its autoload registry.
    let context = crate::ffi::context::shared_null_handle_context();
    let context_ptr = context as *mut crate::abi::ElephcEvalContext;
    let mut values = ElephcRuntimeOps::with_context(context_ptr);
    let loaded = eval_spl_autoload_class_bridge(&name, context, &mut values).unwrap_or(false);
    let context = unsafe { &mut *context_ptr };
    context.sync_global_eval_classes();
    let declared =
        loaded || context.class(&name).is_some() || values.class_exists(&name).unwrap_or(false);
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=class_autoload name={name:?} loaded={loaded} declared={declared}"
        );
    }
    u64::from(declared)
}
