//! Purpose:
//! Allocates an object payload and exposes the physical declared-slot initializer.
//!
//! Called from:
//! - Compiler object, compact Throwable and boxed-clone emitters.
//!
//! Key details:
//! - The caller determines size, class id, reference bindings and property defaults.
//! - The allocated payload has one owner and an assigned native object handle.

use crate::layout::{OBJECT_HEAP_KIND, THROWABLE_HEAP_KIND, object_declared_slot_offset};
use crate::{Emitter, HeapKind};

/// Allocates an ordinary object with the requested size in the native integer-result register.
pub fn emit_allocate_object(emitter: &mut impl Emitter) {
    emit_allocate_with_handle(emitter, OBJECT_HEAP_KIND);
}

/// Allocates a compact Throwable with the requested size in the native integer-result register.
pub(super) fn emit_allocate_throwable(emitter: &mut impl Emitter) {
    emit_allocate_with_handle(emitter, THROWABLE_HEAP_KIND);
}

/// Allocates a compact Throwable through the x86-64 cold-path instruction form.
pub(super) fn emit_allocate_throwable_x86_64(emitter: &mut impl Emitter) {
    emitter.emit_call_label("__rt_heap_alloc");
    let kind = crate::layout::x86_64_heap_kind_word(THROWABLE_HEAP_KIND);
    emitter.emit_load_int_immediate("r10", kind as i64);
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the Throwable kind before acquiring its handle
    crate::object_handles::emit_acquire_object_handle(emitter);
}

/// Stamps the supplied heap kind before binding the allocation to a native object handle.
fn emit_allocate_with_handle(emitter: &mut impl Emitter, heap_kind: u32) {
    crate::heap_alloc::emit_fresh_with_kind(emitter, HeapKind::new(heap_kind));
    crate::object_handles::emit_acquire_object_handle(emitter);
}

/// Zeroes consecutive two-word declared slots after the class-id word.
pub fn emit_zero_declared_slots(emitter: &mut impl Emitter, object: &str, count: usize) {
    for index in 0..count {
        let offset = object_declared_slot_offset(index);
        emitter.emit_store_zero_to_address(object, offset);
        emitter.emit_store_zero_to_address(object, offset + 8);
    }
}
