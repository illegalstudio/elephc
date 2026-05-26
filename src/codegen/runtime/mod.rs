//! Purpose:
//! Defines the runtime module boundary and re-exports the runtime emission entry points.
//! This is the narrow public surface used by codegen to attach helper assembly and data sections.
//!
//! Called from:
//! - `crate::codegen::driver_support::generate_runtime()` while building the cached runtime object.
//! - `crate::codegen::main_emission::finish_user_asm()` when appending user-specific runtime data.
//!
//! Key details:
//! - Keep this surface small: runtime codegen imports these re-exports instead of reaching into leaf emitters directly.

mod arrays;
mod buffers;
mod callables;
mod data;
mod diagnostics;
mod emitters;
mod exceptions;
mod fibers;
/// Runtime helpers for generator state management (yield, resume, stack frames).
pub(crate) mod generators;
mod io;
mod objects;
mod pointers;
mod strings;
/// Standard PHP library constants, functions, and classes.
pub(crate) mod spl;
mod system;
mod x86_minimal;

pub(crate) use data::emit_runtime_data_fixed;
/// Emit fixed runtime data section (symbols, constants, type metadata).
pub(crate) use data::emit_runtime_data_user;
/// Emit user-program-specific runtime data section.
pub(crate) use emitters::emit_runtime;
/// Emit full runtime helpers (orchestrates all runtime sections).
pub(crate) use fibers::{
    FIBER_CALLABLE_OFFSET, FIBER_FLOAT_ARGS_MAX, FIBER_FLOAT_ARGS_OFFSET,
    FIBER_PENDING_THROW_OFFSET, FIBER_STACK_BASE_OFFSET, FIBER_STACK_SIZE_OFFSET,
    FIBER_START_ARGS_MAX, FIBER_START_ARGS_OFFSET, FIBER_TRANSFER_VALUE_OFFSET,
    FIBER_USER_ARG_MAX_OFFSET,
};
