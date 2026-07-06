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

pub(crate) use callback::{emit_callback_wrapper, emit_extern_callback_trampoline};
pub(crate) use fiber::emit_fiber_wrapper;
