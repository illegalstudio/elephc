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
use crate::{Arch, Emitter, HeapKind};

/// Emits a C entry that projects the class id of a live borrowed native object.
pub fn emit_c_class_id(emitter: &mut impl Emitter) {
    let symbol = emitter.extern_symbol("elephc_phpext_native_object_class_id");
    emitter.label_global(&symbol);
    emitter.emit_load_from_address(emitter.int_result_reg(), emitter.int_arg_reg_name(0), 0);
    emitter.emit_return();
}

/// Computes payload bytes from the declared slots and optional trailing owner and metadata words.
pub const fn declared_object_bytes(properties: usize, dynamic_hash: bool, creation_line: bool) -> usize {
    object_declared_slot_offset(properties) + (dynamic_hash as usize + creation_line as usize) * 8
}

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

/// Loads the final payload word from an object whose total payload size is already known.
pub fn emit_load_trailing_word(
    emitter: &mut impl Emitter,
    object: &str,
    payload_size: &str,
    destination: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("sub {payload_size}, {payload_size}, #8")); // locate the final owned payload word
            emitter.instruction(&format!("ldr {destination}, [{object}, {payload_size}]")); // load the trailing object owner
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {destination}, QWORD PTR [{object} + {payload_size} - 8]")); // load the trailing object owner
        }
    }
}

/// Initializes the frame-pointer-relative slot that owns a fallible raw object construction.
///
/// The offset is measured below `x29` on AArch64 and below `rbp` on x86_64. It is
/// not an SP-relative displacement, even when the caller documents its frame via SP.
pub fn emit_begin_object_construction(emitter: &mut impl Emitter, owner_offset: usize) {
    emitter.emit_store_zero_to_local_slot(owner_offset);
}

/// Publishes the newly allocated raw object in its frame-pointer-relative rollback slot.
pub fn emit_record_object_construction(emitter: &mut impl Emitter, owner_offset: usize) {
    let object = emitter.int_result_reg();
    emitter.store_at_offset(object, owner_offset);
}

/// Transfers a successfully boxed object's raw allocation owner into the Mixed result.
pub fn emit_commit_object_construction(emitter: &mut impl Emitter, owner_offset: usize) {
    let result = emitter.int_result_reg();
    emitter.emit_push_reg(result);
    emitter.load_at_offset(result, owner_offset);
    emitter.emit_store_zero_to_local_slot(owner_offset);
    emitter.emit_call_label("__rt_decref_object");
    emitter.emit_pop_reg(result);
}

/// Retires a partially populated object and all children retained in its initialized slots.
pub fn emit_rollback_object_construction(
    emitter: &mut impl Emitter,
    owner_offset: usize,
    done_label: &str,
) {
    let object = emitter.int_result_reg();
    emitter.load_at_offset(object, owner_offset);
    emitter.emit_branch_if_int_result_zero(done_label);
    emitter.emit_store_zero_to_local_slot(owner_offset);
    emitter.emit_call_label("__rt_decref_object");
    emitter.label(done_label);
}
