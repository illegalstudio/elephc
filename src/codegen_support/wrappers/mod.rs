//! Purpose:
//! Shared native wrapper emitters for callables, callbacks, extern trampolines, and fibers.
//! Keeps wrapper generation available to EIR codegen without depending on legacy function emission.
//!
//! Called from:
//! - `crate::codegen` lowerers and `crate::codegen_support::driver_support`.
//!
//! Key details:
//! - Wrapper ABI shapes must stay synchronized with callable descriptors and runtime fiber layout.

mod callback;
mod fiber;

use crate::types::{FunctionSig, PhpType};

pub(crate) use callback::{emit_callback_wrapper, emit_extern_callback_trampoline};
pub(crate) use fiber::emit_fiber_wrapper;

/// Metadata for a generated native callback wrapper.
pub(crate) struct DeferredCallbackWrapper {
    pub(crate) label: String,
    pub(crate) visible_arg_types: Vec<PhpType>,
    pub(crate) target_visible_arg_types: Option<Vec<PhpType>>,
    pub(crate) capture_types: Vec<PhpType>,
    pub(crate) descriptor_prefix_types: Vec<PhpType>,
    pub(crate) descriptor_return_type: Option<PhpType>,
}

/// Metadata for a C-ABI callback trampoline backed by a callable descriptor slot.
pub(crate) struct DeferredExternCallbackTrampoline {
    pub(crate) label: String,
    pub(crate) descriptor_slot_label: String,
    pub(crate) visible_arg_types: Vec<PhpType>,
    pub(crate) return_type: PhpType,
}

/// Metadata for a generated runtime Fiber wrapper around a PHP callable.
pub(crate) struct DeferredFiberWrapper {
    pub(crate) label: String,
    pub(crate) sig: FunctionSig,
    pub(crate) visible_param_count: usize,
    pub(crate) hidden_arg_types: Vec<PhpType>,
    /// Whether descriptor captures must be retained for a closure frame that owns hidden params.
    pub(crate) retain_hidden_args_for_closure_call: bool,
    pub(crate) use_descriptor_invoker: bool,
}
