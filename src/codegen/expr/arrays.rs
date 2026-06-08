//! Purpose:
//! Groups array expression lowering for literals, associative arrays, spreads, and element access.
//! Keeps array construction and read paths behind one expression-module interface.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Array values are refcounted heap objects and must preserve ownership across literal and access results.

mod access;
mod assoc;
mod indexed;

pub(super) use access::{
    emit_array_access, emit_array_access_with_loaded_base, emit_buffer_new, emit_match_expr,
};
pub(crate) use assoc::{
    emit_array_literal_as_assoc_target, emit_assoc_array_literal, emit_empty_assoc_array_literal,
};
pub(super) use indexed::emit_array_literal;
pub(crate) use access::{
    emit_array_access_offset_exists, emit_array_access_offset_set,
    emit_array_access_offset_unset, type_is_array_access_object,
};
pub(crate) use indexed::emit_array_value_type_stamp;
