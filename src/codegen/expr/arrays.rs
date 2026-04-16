mod access;
mod assoc;
mod indexed;

pub(super) use access::{emit_array_access, emit_buffer_new, emit_match_expr};
pub(super) use assoc::emit_assoc_array_literal;
pub(super) use indexed::{emit_array_literal, emit_array_value_type_stamp};
