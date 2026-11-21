//! Purpose:
//! Supplies compiler-owned class identity to Memoir's weak observer cache emitters.
//!
//! Called from:
//! - Runtime orchestration for typed Core calls and object reclamation.
//!
//! Key details:
//! - The semantic class-id symbol stays here; Memoir owns non-owning cache storage lifetimes.

use crate::codegen_support::emit::Emitter;

/// Emits non-owning cache operations with the compiler's PHP wrapper class identity.
pub(crate) fn emit_weak_reference(emitter: &mut Emitter) {
    elephc_memoir::weak_cache::emit(emitter, "_weak_reference_class_id");
}
