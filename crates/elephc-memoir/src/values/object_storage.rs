//! Purpose:
//! Allocates an object payload and exposes the physical declared-slot initializer.
//!
//! Called from:
//! - Compiler object, compact Throwable and boxed-clone emitters.
//!
//! Key details:
//! - The caller determines size, class id, reference bindings and property defaults.
//! - The allocated payload has one owner and an assigned native object handle.

use crate::layout::{OBJECT_HEAP_KIND, THROWABLE_HEAP_KIND, x86_64_heap_kind_word};
use crate::{Arch, Emitter};

/// Allocates an ordinary object with the requested size in the native integer-result register.
pub fn emit_allocate_object(emitter: &mut impl Emitter) {
    emit_allocate_with_handle(emitter, OBJECT_HEAP_KIND);
}

/// Allocates a compact Throwable with the requested size in the native integer-result register.
pub fn emit_allocate_throwable(emitter: &mut impl Emitter) {
    emit_allocate_with_handle(emitter, THROWABLE_HEAP_KIND);
}

/// Stamps the supplied heap kind before binding the allocation to a native object handle.
fn emit_allocate_with_handle(emitter: &mut impl Emitter, heap_kind: u32) {
    emitter.emit_call_label("__rt_heap_alloc");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{heap_kind}"));              // encode the heap kind of the new object
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the managed allocator header
        }
        Arch::X86_64 => {
            let kind = x86_64_heap_kind_word(heap_kind);
            emitter.instruction(&format!("mov r10, 0x{kind:x}"));               // encode the x86_64 managed heap kind
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the managed allocator header
        }
    }
    crate::object_handles::emit_acquire_object_handle(emitter);
}

/// Zeroes consecutive two-word declared slots after the class-id word.
pub fn emit_zero_declared_slots(emitter: &mut impl Emitter, object: &str, count: usize) {
    for index in 0..count {
        let offset = 8 + index * 16;
        emitter.emit_store_zero_to_address(object, offset);
        emitter.emit_store_zero_to_address(object, offset + 8);
    }
}
