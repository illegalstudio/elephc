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
mod resource_ids;
/// Standard PHP library constants, functions, and classes.
pub(crate) mod spl;
mod strings;
mod system;
/// zval pack/unpack bridge helpers (elephc values ↔ PHP zval structs).
mod zval;

pub(crate) use data::emit_runtime_data_fixed;
/// The PHP object-handle pool: binding a handle at allocation and reading one back.
/// Every object-allocation site in codegen calls `emit_acquire_object_handle`.
pub(crate) use objects::{
    emit_acquire_object_handle, object_handle_free_slots, object_handle_index_slots,
};
/// The PHP RESOURCE-id registry — a numbering space entirely separate from the
/// object handles above. `RESOURCE_ID_TABLE_SLOTS` sizes its two side tables in
/// the fixed data section.
pub(crate) use resource_ids::RESOURCE_ID_TABLE_SLOTS;
pub(crate) use data::{
    OB_CLOSURE_INVOKE_NAME, OB_DEFAULT_HANDLER_NAME, OB_NTC_CREATE_FAIL,
    OB_WARN_BAD_CALLBACK_GENERIC, OB_WARN_BAD_CALLBACK_PREFIX, OB_WARN_BAD_CALLBACK_SUFFIX,
};
/// Emit fixed runtime data section (symbols, constants, type metadata).
pub(crate) use data::emit_runtime_data_user;
pub(crate) use data::{is_user_filter_contract_method, is_user_wrapper_contract_method};
/// Emit user-program-specific runtime data section.
pub(crate) use emitters::emit_runtime;
/// The PHP 8.5 NAN-to-bool coercion probe, reached from `src/codegen/lower_inst` float
/// truthiness lowering as well as from the boxed-Mixed runtime helpers.
pub(crate) use arrays::{emit_nan_bool_coercion_probe, nan_bool_coercion_warning_enabled};
pub(crate) use arrays::array_offset_on_null_warning;
/// The `__rt_hash_map` callback result-kind selector, chosen by the `array_map()` lowering.
pub(crate) use arrays::HashMapResultKind;
/// Emit full runtime helpers (orchestrates all runtime sections).
pub(crate) use fibers::{
    FIBER_CALLABLE_OFFSET, FIBER_PENDING_THROW_OFFSET, FIBER_STACK_BASE_OFFSET,
    FIBER_STACK_SIZE_OFFSET, FIBER_START_ARGS_MAX, FIBER_START_ARGS_OFFSET,
    FIBER_START_ARG_COUNT_OFFSET, FIBER_STATE_NOT_STARTED, FIBER_STATE_RUNNING,
    FIBER_STATE_SUSPENDED, FIBER_STATE_TERMINATED, FIBER_TRANSFER_VALUE_OFFSET,
    FIBER_USER_ARG_MAX_OFFSET,
};
