//! Purpose:
//! Dispatches array access expression lowering across indexed arrays, buffers, and match-specific reads.
//! Keeps container-specific addressing details out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::arrays`
//!
//! Key details:
//! - Access paths must agree on nullable, boxed Mixed, and borrowed-result ownership conventions.

mod buffer;
mod indexed;
mod match_expr;
mod object;
mod string_offset;

pub(crate) use buffer::emit_buffer_new;
pub(crate) use indexed::{emit_array_access, emit_array_access_with_loaded_base};
pub(crate) use match_expr::emit_match_expr;
pub(crate) use object::{
    emit_offset_exists as emit_array_access_offset_exists,
    emit_offset_set as emit_array_access_offset_set,
    emit_offset_unset as emit_array_access_offset_unset,
    type_is_array_access_object,
};
