//! Purpose:
//! Allocates an object payload and exposes the physical declared-slot initializer.
//!
//! Called from:
//! - Direct object lowering, boxed clone, stdClass and eval Reflection emitters.
//!
//! Key details:
//! - The caller determines size, class id, reference bindings and property defaults.
//! - The allocated payload has one owner and an assigned native object handle.

use crate::{Arch, Emitter, layout::{OBJECT_HEAP_KIND, x86_64_heap_kind_word}};

/// Allocates the requested byte count in the native integer-result register.
pub fn emit_allocate_object(emitter: &mut impl Emitter) {
    emitter.emit_call_label("__rt_heap_alloc");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{OBJECT_HEAP_KIND}"));       // encode the heap kind of the new object
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the managed allocator header
        }
        Arch::X86_64 => {
            let kind = x86_64_heap_kind_word(OBJECT_HEAP_KIND);
            emitter.instruction(&format!("mov r10, 0x{kind:x}"));               // encode the managed x86_64 object kind
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
