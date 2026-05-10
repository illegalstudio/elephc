mod buffer;
mod indexed;
mod match_expr;

pub(crate) use buffer::emit_buffer_new;
pub(crate) use indexed::{emit_array_access, emit_array_access_with_loaded_base};
pub(crate) use match_expr::emit_match_expr;
