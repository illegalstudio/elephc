//! Purpose:
//! Registers packed splice insertion and selects the scalar conversion callback.
//!
//! Called from:
//! - Native array runtime emission.
//!
//! Key details:
//! - Memoir owns the backing storage and publication routines.

use crate::codegen_support::emit::Emitter;
pub use elephc_memoir::values::array_splice_insert::{
    emit_array_splice_insert, emit_array_splice_insert_boxed, emit_array_splice_insert_refcounted,
};

/// Inserts unboxed words using the compiler's boxed-to-integer conversion.
pub fn emit_array_splice_insert_unboxed(emitter: &mut Emitter) {
    elephc_memoir::values::array_splice_insert::emit_array_splice_insert_unboxed(
        emitter, "__rt_mixed_cast_int",
    );
}
