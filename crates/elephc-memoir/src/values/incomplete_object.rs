//! Purpose:
//! Defines the owned class-name and property-hash layout of an incomplete object.
//!
//! Called from:
//! - The compiler's unserializer and Memoir's object destruction and tracing paths.
//!
//! Key details:
//! - The class name is a persisted byte pair; only the opaque hash is a GC graph edge.
//! - The hash starts null and can be installed or replaced as the decoder reads properties.

use crate::{abi, Arch, Emitter, HeapKind};

/// Reserved class identity for the preserved wire object layout.
pub const CLASS_ID: i64 = -2;
/// Offset of the persisted original class-name pointer.
pub const NAME_PTR: usize = 8;
/// Offset of the persisted original class-name length.
pub const NAME_LEN: usize = 16;
/// Offset of the owned opaque property hash.
pub const PROPERTY_HASH: usize = 24;
/// Payload extent through the opaque property hash.
pub const BYTES: usize = PROPERTY_HASH + 8;

/// Allocates an incomplete object from a staged borrowed name and an object spill slot.
///
/// ARM64 slots are stack-relative. x86_64 slots are below the frame pointer.
pub fn emit_new(emitter: &mut impl Emitter, object_slot: usize, name_ptr: usize, name_len: usize) {
    let result = emitter.int_result_reg();
    abi::emit_load_int_immediate(emitter, result, BYTES as i64);
    match emitter.arch() {
        Arch::AArch64 => {
            super::object_storage::emit_allocate_object(emitter);
            emitter.instruction(&format!("mov x9, #{CLASS_ID}"));               // select the reserved incomplete-object layout
            emitter.instruction("str x9, [x0]");                                // publish the synthetic class identity
            emitter.instruction(&format!("str x0, [sp, #{object_slot}]"));      // preserve the object across class-name persistence
            emitter.instruction(&format!("ldr x1, [sp, #{name_ptr}]"));         // load the borrowed class-name bytes
            emitter.instruction(&format!("ldr x2, [sp, #{name_len}]"));         // load the borrowed class-name length
            abi::emit_call_label(emitter, "__rt_str_persist");
            emitter.instruction(&format!("ldr x0, [sp, #{object_slot}]"));      // recover the incomplete-object owner
            emitter.instruction(&format!("str x1, [x0, #{NAME_PTR}]"));         // install the persisted class-name pointer
            emitter.instruction(&format!("str x2, [x0, #{NAME_LEN}]"));         // install the persisted class-name length
            emitter.instruction(&format!("str xzr, [x0, #{PROPERTY_HASH}]"));   // the decoder has not created the property hash yet
        }
        Arch::X86_64 => {
            crate::heap_alloc::emit_fresh_with_kind(emitter, HeapKind::new(crate::layout::OBJECT_HEAP_KIND));
            emitter.instruction("mov rdi, rax");                                // preserve the decoder's handle-call argument setup
            crate::object_handles::emit_acquire_object_handle(emitter);
            emitter.instruction(&format!("mov QWORD PTR [rax], {CLASS_ID}"));   // publish the synthetic class identity
            abi::store_at_offset(emitter, "rax", object_slot);
            abi::load_at_offset(emitter, "rax", name_ptr);
            abi::load_at_offset(emitter, "rdx", name_len);
            abi::emit_call_label(emitter, "__rt_str_persist");
            abi::load_at_offset(emitter, "r10", object_slot);
            emitter.instruction(&format!("mov QWORD PTR [r10 + {NAME_PTR}], rax")); // install the persisted class-name pointer
            emitter.instruction(&format!("mov QWORD PTR [r10 + {NAME_LEN}], rdx")); // install the persisted class-name length
            emitter.instruction(&format!("mov QWORD PTR [r10 + {PROPERTY_HASH}], 0")); // the decoder has not created the property hash yet
            emitter.instruction("mov rax, r10");                                // return the incomplete-object owner in the result register
        }
    }
}
