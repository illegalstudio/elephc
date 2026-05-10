//! Purpose:
//! Groups object dispatch helpers for methods, static calls, interfaces, enums, fibers, and vtables.
//! Keeps receiver preparation and call target selection isolated from object expression dispatch.
//!
//! Called from:
//! - `crate::codegen::expr::objects`
//!
//! Key details:
//! - Dispatch paths must share receiver ownership and ABI argument conventions with normal call lowering.

mod enums;
mod fiber;
mod interface;
mod method;
mod prep;
mod static_call;
mod vtable;

pub(crate) use interface::emit_dispatch_interface_method;
pub(crate) use vtable::emit_dispatch_instance_method;
pub(super) use method::{
    emit_method_call, emit_method_call_with_pushed_args,
    emit_method_call_with_saved_receiver_below_args, emit_pushed_method_args,
};
pub(super) use static_call::{
    emit_forwarded_called_class_id, emit_immediate_class_id, emit_static_method_call,
};
