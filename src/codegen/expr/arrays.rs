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
pub(super) use assoc::emit_assoc_array_literal;
pub(super) use indexed::emit_array_literal;
pub(crate) use indexed::emit_array_value_type_stamp;
