//! Purpose:
//! Defines the runtime module boundary and re-exports the runtime emission entry points.
//! This is the narrow public surface used by codegen to attach helper assembly and data sections.
//!
//! Called from:
//! - `crate::codegen_support::driver_support::generate_runtime()` while building the cached runtime object.
//! - `crate::codegen::finalize_user_asm()` when appending user-specific runtime data.
//!
//! Key details:
//! - Keep this surface small: runtime codegen imports these re-exports instead of reaching into leaf emitters directly.

mod arrays;
mod buffers;
mod callables;
mod data;
mod diagnostics;
mod emitters;
mod eval_bridge;
mod eval_scope;
mod exceptions;
mod fibers;
/// Runtime helpers for generator state management (yield, resume, stack frames).
pub(crate) mod generators;
mod io;
mod objects;
mod pointers;
/// Standard PHP library constants, functions, and classes.
pub(crate) mod spl;
mod strings;
mod system;
/// zval pack/unpack bridge helpers (elephc values ↔ PHP zval structs).
mod zval;

pub(crate) use data::emit_runtime_data_fixed;
/// Emit fixed runtime data section (symbols, constants, type metadata).
pub(crate) use data::emit_runtime_data_user;
pub(crate) use data::{is_user_filter_contract_method, is_user_wrapper_contract_method};
/// Emit user-program-specific runtime data section.
pub(crate) use emitters::emit_runtime;
/// Emit full runtime helpers (orchestrates all runtime sections).
pub(crate) use fibers::{
    FIBER_CALLABLE_OFFSET, FIBER_PENDING_THROW_OFFSET, FIBER_STACK_BASE_OFFSET,
    FIBER_STACK_SIZE_OFFSET, FIBER_START_ARGS_MAX, FIBER_START_ARGS_OFFSET,
    FIBER_START_ARG_COUNT_OFFSET, FIBER_STATE_NOT_STARTED, FIBER_STATE_RUNNING,
    FIBER_STATE_SUSPENDED, FIBER_STATE_TERMINATED, FIBER_TRANSFER_VALUE_OFFSET,
    FIBER_USER_ARG_MAX_OFFSET,
};
