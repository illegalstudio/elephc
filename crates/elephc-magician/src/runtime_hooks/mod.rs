//! Purpose:
//! Bridges EvalIR value operations to elephc runtime values.
//! The module wires the stateless adapter type while focused submodules own
//! generated C-ABI symbols, trait operations, and opcode mappings.
//!
//! Called from:
//! - `crate::ffi::execute::__elephc_eval_execute()` in non-test builds.
//!
//! Key details:
//! - The wrapper symbols adapt to elephc's target-specific internal helper ABI.
//! - Unit tests do not link the generated runtime object, so real hooks compile
//!   only outside `cfg(test)`.

#[cfg(not(test))]
mod externs;
#[cfg(not(test))]
mod ops;
#[cfg(not(test))]
mod tags;

#[cfg(not(test))]
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::abi::ElephcEvalContext;
#[cfg(not(test))]
use crate::value::{RuntimeCell, RuntimeCellHandle};
#[cfg(not(test))]
use externs::{
    __elephc_eval_install_dynamic_object_destructor_hook, __elephc_eval_value_array_new,
    __elephc_eval_value_array_set, __elephc_eval_value_int, __elephc_eval_value_object_from_raw,
    __elephc_eval_value_release,
};

/// Runtime hook adapter that produces and consumes boxed elephc Mixed cells.
#[cfg(not(test))]
pub struct ElephcRuntimeOps {
    context: *const ElephcEvalContext,
}

#[cfg(not(test))]
impl ElephcRuntimeOps {
    /// Creates a runtime hook adapter without caller-sensitive eval context.
    pub const fn new() -> Self {
        Self {
            context: std::ptr::null(),
        }
    }

    /// Creates a runtime hook adapter that can expose the active class scope to generated helpers.
    pub const fn with_context(context: *const ElephcEvalContext) -> Self {
        Self { context }
    }

    /// Converts a runtime wrapper result into an interpreter handle.
    pub(crate) fn handle(ptr: *mut RuntimeCell) -> Result<RuntimeCellHandle, EvalStatus> {
        if ptr.is_null() {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(RuntimeCellHandle::from_raw(ptr))
        }
    }

    /// Boxes one borrowed raw object payload for eval `$this` dispatch.
    pub(crate) fn object_from_raw(
        object: *mut RuntimeCell,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_object_from_raw(object) })
    }

    /// Packs borrowed arguments into an owned array, releasing temporary keys and partial arrays on failure.
    fn arg_array(args: Vec<RuntimeCellHandle>) -> Result<RuntimeCellHandle, EvalStatus> {
        let arg_array = unsafe { __elephc_eval_value_array_new(args.len() as u64) };
        let arg_array = Self::handle(arg_array)?;
        let result = (|| {
            for (index, value) in args.into_iter().enumerate() {
                let index = Self::handle(unsafe { __elephc_eval_value_int(index as i64) })?;
                let updated = unsafe {
                    __elephc_eval_value_array_set(arg_array.as_ptr(), index.as_ptr(), value.as_ptr())
                };
                unsafe { __elephc_eval_value_release(index.as_ptr()); }
                // The setter mutates and returns the same box without creating another owner.
                Self::handle(updated)?;
            }
            Ok(arg_array)
        })();
        if result.is_err() {
            unsafe { __elephc_eval_value_release(arg_array.as_ptr()); }
        }
        result
    }

    /// Returns the active eval class-scope bytes in the generated helper ABI shape.
    fn current_class_scope_abi(&self) -> (*const u8, u64) {
        let Some(context) = (unsafe { self.context.as_ref() }) else {
            return (std::ptr::null(), 0);
        };
        let Some(class_scope) = context.current_class_scope() else {
            return (std::ptr::null(), 0);
        };
        (class_scope.as_ptr(), class_scope.len() as u64)
    }
}

/// Installs the eval dynamic object destructor callback into runtime data.
#[cfg(not(test))]
pub(crate) unsafe fn install_dynamic_object_destructor_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_dynamic_object_destructor_hook(callback);
    }
}

/// Installs the eval output-buffering handler callback into the generated runtime.
///
/// # Safety
/// `callback` accepts a writable `OutputHandlerCallV1` pointer and returns a status.
/// The runtime calls it on buffer flushes and consumes returned owners after Rust exits.
/// `release` accepts a registry id and boxed-Throwable output pointer and returns a status.
#[cfg(not(test))]
pub(crate) unsafe fn install_ob_handler_hook(callback: usize, release: usize) {
    unsafe {
        externs::install_ob_handler_hook_raw(callback, release);
    }
}
