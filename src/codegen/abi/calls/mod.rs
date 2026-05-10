//! Purpose:
//! Groups ABI call helpers for incoming parameters, outgoing materialization, invocation, and stack scratch space.
//! Offers the call-lowering surface used by PHP, method, wrapper, and extern emitters.
//!
//! Called from:
//! - `crate::codegen::abi` and call expression emitters
//!
//! Key details:
//! - Source-order evaluation is handled above this layer; this module materializes already-planned ABI order.

mod incoming;
mod invoke;
mod outgoing;
mod stack;

pub use incoming::emit_store_incoming_param;
pub use invoke::{emit_call_label, emit_call_reg};
pub use outgoing::{build_outgoing_arg_assignments_for_target, materialize_outgoing_args};
pub use stack::{
    emit_load_temporary_stack_slot, emit_pop_float_reg, emit_pop_reg, emit_pop_reg_pair,
    emit_push_float_reg, emit_push_reg, emit_push_reg_pair, emit_push_result_value,
    emit_release_temporary_stack, emit_reserve_temporary_stack, emit_temporary_stack_address,
};
