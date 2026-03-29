use crate::codegen::emit::Emitter;

use super::decref_array::emit_decref_array;
use super::decref_hash::emit_decref_hash;
use super::decref_object::emit_decref_object;
use super::incref::emit_incref;

/// Reference counting runtime functions for the garbage collector.
/// Refcount is stored as a 32-bit value at [user_ptr - 12] inside the uniform 16-byte heap header.
pub fn emit_refcount(emitter: &mut Emitter) {
    emit_incref(emitter);
    emit_decref_array(emitter);
    emit_decref_hash(emitter);
    emit_decref_object(emitter);
}
