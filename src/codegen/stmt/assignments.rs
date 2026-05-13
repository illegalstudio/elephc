//! Purpose:
//! Groups assignment statement lowering for locals, object properties, and static properties.
//! Provides a single statement-dispatch surface for PHP write operations.
//!
//! Called from:
//! - `crate::codegen::stmt`
//!
//! Key details:
//! - Assignment paths must preserve write side effects and update ownership for overwritten storage.

mod locals;
mod properties;
mod static_properties;

pub(crate) use locals::emit_assign_stmt;
pub(crate) use properties::{
    emit_dynamic_property_get,
    emit_property_array_assign_stmt,
    emit_property_array_push_stmt,
    emit_property_assign_stmt,
};
pub(crate) use static_properties::{
    emit_static_property_array_assign_stmt,
    emit_static_property_array_push_stmt,
    emit_static_property_assign_stmt,
};
