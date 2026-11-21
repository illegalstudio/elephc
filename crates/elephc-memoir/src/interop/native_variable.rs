//! Purpose:
//! Gives native-variable bridges opaque queries and owner updates for the generated heap.
//!
//! Called from:
//! - Compiler emission for the mbstring live-variable C ABI.
//!
//! Key details:
//! - Heap offsets and transferred-owner writes are private to emitted native code.

use crate::{layout::{ARRAY_HEAP_KIND, HASH_HEAP_KIND}, Arch, Emitter};

/// Emits the C ABI accessors used while inspecting and updating a borrowed variable slot.
pub fn emit_mbstring_accessors(emitter: &mut impl Emitter) {
    emit_heap_shape(emitter);
    crate::heap_refcount::emit_c_owner_count(emitter, "elephc_mbstring_variable_owner_count_v1", false);
    emit_replace_owner(emitter, "elephc_mbstring_variable_replace_pointer_v1", false);
    emit_replace_owner(emitter, "elephc_mbstring_variable_replace_string_v1", true);
    emit_make_unique(emitter);
    super::native_array_slots::emit(emitter);
    super::native_object_slot::emit(emitter);
    emit_value_projection(emitter);
    emit_initialize_reference(emitter);
}

/// Returns the allocation kind and value tag as separate words in a C result record.
fn emit_heap_shape(emitter: &mut impl Emitter) {
    emitter.label_global(&emitter.extern_symbol("elephc_mbstring_variable_heap_shape_v1"));
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #-8]");                           // read the managed allocation's metadata
            emitter.instruction("and x0, x9, #255");                            // return the allocation kind as the first word
            emitter.instruction("ubfx x1, x9, #8, #7");                         // return the value tag as the second word
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, QWORD PTR [rdi - 8]");                // read the managed allocation's metadata
            emitter.instruction("movzx eax, dl");                               // return the allocation kind as the first word
            emitter.instruction("shr rdx, 8");                                  // position the value tag
            emitter.instruction("and edx, 127");                                // return the value tag as the second word
        }
    }
    emitter.emit_return();
}

/// Transfers the incoming owner into a slot before releasing the previous owner.
fn emit_replace_owner(emitter: &mut impl Emitter, name: &str, string: bool) {
    emitter.label_global(&emitter.extern_symbol(name));
    let slot = emitter.int_arg_reg_name(0);
    let pointer = emitter.int_arg_reg_name(1);
    let length = emitter.int_arg_reg_name(2);
    let replaced = emitter.secondary_scratch_reg();
    emitter.emit_load_from_address(replaced, slot, 0);
    emitter.emit_store_to_address(pointer, slot, 0);
    if string {
        emitter.emit_store_to_address(length, slot, 8);
    }
    emitter.emit_reg_move(emitter.int_result_reg(), replaced);
    emitter.emit_jump("__rt_decref_any");
}

/// Separates an attached array and stores its replacement before returning to the bridge.
fn emit_make_unique(emitter: &mut impl Emitter) {
    let hash = "__rt_mbstring_variable_unique_hash";
    let publish = "__rt_mbstring_variable_unique_publish";
    let failure = "__rt_mbstring_variable_unique_failure";
    emitter.label_global(&emitter.extern_symbol("elephc_mbstring_variable_make_unique_v1"));
    emitter.emit_frame_prologue(32);
    let slot = emitter.int_arg_reg_name(0);
    let result = emitter.int_result_reg();
    emitter.store_at_offset(slot, 8);
    emitter.emit_load_from_address(slot, slot, 0);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldrb w9, [{slot}, #-8]"));            // inspect the existing native container kind
            emitter.instruction(&format!("cmp w9, #{HASH_HEAP_KIND}"));         // associative storage has its own COW helper
            emitter.instruction(&format!("b.eq {hash}"));                       // separate the hash with its table layout
            emitter.instruction(&format!("cmp w9, #{ARRAY_HEAP_KIND}"));        // require indexed storage for the other helper
            emitter.instruction(&format!("b.ne {failure}"));                    // reject a payload that is no longer an array
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movzx eax, BYTE PTR [{slot} - 8]"));  // inspect the existing native container kind
            emitter.instruction(&format!("cmp eax, {HASH_HEAP_KIND}"));         // associative storage has its own COW helper
            emitter.instruction(&format!("je {hash}"));                         // separate the hash with its table layout
            emitter.instruction(&format!("cmp eax, {ARRAY_HEAP_KIND}"));        // require indexed storage for the other helper
            emitter.instruction(&format!("jne {failure}"));                     // reject a payload that is no longer an array
        }
    }
    emitter.emit_call_label("__rt_array_ensure_unique");
    emitter.emit_jump(publish);
    emitter.label(hash);
    emitter.emit_call_label("__rt_hash_ensure_unique");
    emitter.label(publish);
    emitter.emit_branch_if_int_result_zero(failure);
    emitter.load_at_offset(emitter.secondary_scratch_reg(), 8);
    emitter.emit_store_to_address(result, emitter.secondary_scratch_reg(), 0);
    emitter.emit_frame_restore(32);
    emitter.emit_return();
    emitter.label(failure);
    emitter.emit_load_int_immediate(result, 0);
    emitter.emit_frame_restore(32);
    emitter.emit_return();
}

/// Exposes borrowed native value words without sharing offsets with the bridge.
fn emit_value_projection(emitter: &mut impl Emitter) {
    let source = emitter.int_arg_reg_name(0);
    let output = emitter.int_arg_reg_name(1);
    let scratch = emitter.secondary_scratch_reg();
    let result = emitter.int_result_reg();
    let arm = emitter.arch() == Arch::AArch64;

    emitter.label_global(&emitter.extern_symbol("elephc_mbstring_variable_cell_v1"));
    for offset in [0, 8, 16] {
        emitter.emit_load_from_address(scratch, source, offset);
        emitter.emit_store_to_address(scratch, output, offset);
    }
    emitter.instruction(&if arm { format!("add {scratch}, {source}, #8") }
        else { format!("lea {scratch}, [{source} + 8]") }); // provide the owner slot alongside the decoded value
    emitter.emit_store_to_address(scratch, output, 24);
    emitter.emit_return();

    emitter.label_global(&emitter.extern_symbol("elephc_mbstring_variable_slot_value_v1"));
    // Return the two-word C record in x0/x1 or rax/rdx. The flag selects an available high word.
    emitter.emit_load_from_address(scratch, source, 0);
    emitter.instruction(if arm { "cbz x1, __rt_mbstring_variable_no_high" } else { "test rsi, rsi" }); // one-word slots have no second payload
    if !arm {
        emitter.instruction("jz __rt_mbstring_variable_no_high");              // avoid reading past a single-word slot
    }
    let high = if arm { "x1" } else { "rdx" };
    emitter.emit_load_from_address(high, source, 8);
    emitter.emit_jump("__rt_mbstring_variable_slot_value_done");
    emitter.label("__rt_mbstring_variable_no_high");
    emitter.emit_load_int_immediate(high, 0);
    emitter.label("__rt_mbstring_variable_slot_value_done");
    emitter.emit_reg_move(result, scratch);
    emitter.emit_return();
}

/// Publishes the child of a fresh, still-private native reference cell.
fn emit_initialize_reference(emitter: &mut impl Emitter) {
    emitter.label_global(&emitter.extern_symbol("elephc_mbstring_variable_initialize_reference_v1"));
    emitter.emit_store_to_address(emitter.int_arg_reg_name(1), emitter.int_arg_reg_name(0), 0);
    emitter.emit_return();
}
