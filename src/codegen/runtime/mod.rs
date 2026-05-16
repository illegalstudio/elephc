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
pub(crate) mod generators;
mod io;
mod objects;
mod pointers;
mod strings;
mod system;
mod x86_minimal;

pub(crate) use data::emit_runtime_data_fixed;
pub(crate) use data::emit_runtime_data_user;
pub(crate) use emitters::emit_runtime;
pub(crate) use fibers::{
    FIBER_CALLABLE_OFFSET, FIBER_FLOAT_ARGS_MAX, FIBER_FLOAT_ARGS_OFFSET,
    FIBER_STACK_BASE_OFFSET, FIBER_STACK_SIZE_OFFSET, FIBER_START_ARGS_MAX, FIBER_START_ARGS_OFFSET,
    FIBER_USER_ARG_MAX_OFFSET,
};
