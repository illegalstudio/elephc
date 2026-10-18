//! Purpose:
//! Emits Serializable's opaque C wire envelope after invoking its legacy serializer.
//!
//! Called from:
//! - The ordinary object serializer when no __serialize hook supersedes Serializable.
//!
//! Key details:
//! - User methods return either an owned string pair or a boxed nullable string.
//! - Callback concat scratch is discarded before writing the final envelope.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::data::SerializableReturnAbi;

/// Branches to the custom serializer before any O envelope has been written.
pub(super) fn emit_probe(emitter: &mut Emitter) {
    let (class, table, target) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x9", "x10"),
        Arch::X86_64 => ("rax", "r10", "r11"),
    };
    abi::emit_symbol_address(emitter, table, "_class_serializable_serialize_ptrs");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("ldr {target}, [{table}, {class}, lsl #3]")), // select the legacy hook for this class
        Arch::X86_64 => emitter.instruction(&format!("mov {target}, QWORD PTR [{table} + {class} * 8]")), // select the legacy hook for this class
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x10, __rt_serialize_ordinary_object");     // ordinary and modern hooks keep the O wire path
            emitter.instruction("ldr x0, [sp]");                                // recover the object before invoking its legacy serializer
        }
        Arch::X86_64 => {
            emitter.instruction("test r11, r11");                               // zero means no active Serializable hook
            emitter.instruction("jz __rt_serialize_ordinary_object");           // ordinary and modern hooks keep the O wire path
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // recover the receiver for the C ABI
        }
    }
    abi::emit_call_label(emitter, "__rt_serialize_serializable");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp, #80]");                     // restore the enclosing object serializer's frame
            emitter.instruction("add sp, sp, #96");                             // release its scratch after writing the custom envelope
        }
        Arch::X86_64 => emitter.instruction("leave"),                           // restore the enclosing object serializer's frame
    }
    abi::emit_return(emitter);
    emitter.label("__rt_serialize_ordinary_object");
}

/// Emits the target-neutral ownership and envelope-writing sequence.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    emitter.blank();
    emitter.label_global("__rt_serialize_serializable");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, arg0, 8);
    abi::emit_load_from_address(emitter, result, arg0, 0);
    abi::store_at_offset(emitter, result, 16);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_concat_off", 0);
    abi::store_at_offset(emitter, scratch, 24);
    abi::emit_store_zero_to_local_slot(emitter, 48);
    load_table_entry(emitter, "_class_serializable_serialize_ptrs");
    abi::emit_reg_move(emitter, arg1, scratch);
    abi::load_at_offset(emitter, arg0, 8);
    abi::emit_call_label(emitter, "__rt_serialize_invoke_legacy");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::store_at_offset(emitter, "x0", 72);
            abi::store_at_offset(emitter, "x1", 56);
            abi::store_at_offset(emitter, "x2", 64);
        }
        Arch::X86_64 => {
            abi::store_at_offset(emitter, "rax", 72);
            abi::store_at_offset(emitter, "rax", 56);
            abi::store_at_offset(emitter, "rdx", 64);
        }
    }
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_reg_to_symbol(emitter, result, "_concat_off", 0);
    load_table_entry(emitter, "_class_serializable_serialize_return_abi");
    abi::emit_reg_move(emitter, result, scratch);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_serializable_string_ready");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{}", SerializableReturnAbi::Null as u8)); // void callbacks return PHP null without a payload owner
            emitter.instruction("b.eq __rt_serializable_void");                 // serialize an omitted legacy return as null
            emitter.instruction(&format!("cmp x0, #{}", SerializableReturnAbi::Scalar as u8)); // known scalar results violate the tentative runtime contract
            emitter.instruction("b.eq __rt_serializable_invalid");              // reject scalar payloads before treating them as string pointers
            emitter.instruction(&format!("cmp x0, #{}", SerializableReturnAbi::Heap as u8)); // known heap results require owner retirement before rejection
            emitter.instruction("b.eq __rt_serializable_invalid_heap");         // consume the rejected native heap owner
            emitter.instruction(&format!("cmp x0, #{}", SerializableReturnAbi::TaggedScalar as u8)); // nullable integers carry their null tag inline
            emitter.instruction("b.eq __rt_serializable_tagged");               // validate the inline null branch without reading a heap box
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {}", SerializableReturnAbi::Null as u8)); // void callbacks return PHP null without a payload owner
            emitter.instruction("je __rt_serializable_void");                   // serialize an omitted return as null
            emitter.instruction(&format!("cmp rax, {}", SerializableReturnAbi::Scalar as u8)); // known scalar results violate the tentative contract
            emitter.instruction("je __rt_serializable_invalid");                // avoid interpreting a scalar as a string pointer
            emitter.instruction(&format!("cmp rax, {}", SerializableReturnAbi::Heap as u8)); // known heap results own storage requiring cleanup
            emitter.instruction("je __rt_serializable_invalid_heap");           // retire the rejected heap owner
            emitter.instruction(&format!("cmp rax, {}", SerializableReturnAbi::TaggedScalar as u8)); // nullable integers carry their null tag inline
            emitter.instruction("je __rt_serializable_tagged");                 // validate the inline null branch without reading a heap box
        }
    }
    abi::load_at_offset(emitter, result, 72);
    abi::store_at_offset(emitter, result, 48);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #8");                                  // a null result serializes the whole object as PHP null
            emitter.instruction("b.eq __rt_serializable_null");                 // skip the C envelope for a null hook result
            emitter.instruction("cmp x0, #1");                                  // every other accepted result must be a PHP string
            emitter.instruction("b.ne __rt_serializable_invalid_box");          // release a boxed invalid result before throwing
            abi::store_at_offset(emitter, "x1", 56);
            abi::store_at_offset(emitter, "x2", 64);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 8");                                  // a null result serializes the whole object as PHP null
            emitter.instruction("je __rt_serializable_null");                   // skip the C envelope for a null hook result
            emitter.instruction("cmp rax, 1");                                  // every other accepted result must be a PHP string
            emitter.instruction("jne __rt_serializable_invalid_box");           // release a boxed invalid result before throwing
            abi::store_at_offset(emitter, "rdi", 56);
            abi::store_at_offset(emitter, "rdx", 64);
        }
    }
    emitter.label("__rt_serializable_string_ready");
    abi::load_at_offset(emitter, result, 16);
    let name = abi::tertiary_scratch_reg(emitter);
    abi::emit_symbol_address(emitter, scratch, "_class_name_entries");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("add x10, x10, x0, lsl #4");                    // select the class name entry by runtime class id
            abi::emit_load_from_address(emitter, name, scratch, 0);
            abi::store_at_offset(emitter, name, 32);
            abi::emit_load_from_address(emitter, name, scratch, 8);
        }
        Arch::X86_64 => {
            emitter.instruction("shl rax, 4");                                  // scale the runtime id to the class name entry stride
            emitter.instruction("add r10, rax");                                // select the class name entry
            abi::emit_load_from_address(emitter, name, scratch, 0);
            abi::store_at_offset(emitter, name, 32);
            abi::emit_load_from_address(emitter, name, scratch, 8);
        }
    }
    abi::store_at_offset(emitter, name, 40);
    literal(emitter, b"C:");
    abi::load_at_offset(emitter, result, 40);
    abi::emit_call_label(emitter, "__rt_serialize_uint");
    literal(emitter, b":\"");
    abi::load_at_offset(emitter, arg0, 32);
    abi::load_at_offset(emitter, arg1, 40);
    abi::emit_call_label(emitter, "__rt_concat_append");
    literal(emitter, b"\":");
    abi::load_at_offset(emitter, result, 64);
    abi::emit_call_label(emitter, "__rt_serialize_uint");
    literal(emitter, b":{");
    abi::load_at_offset(emitter, arg0, 56);
    abi::load_at_offset(emitter, arg1, 64);
    abi::emit_call_label(emitter, "__rt_concat_append");
    literal(emitter, b"}");
    abi::emit_jump(emitter, "__rt_serializable_cleanup");
    emitter.label("__rt_serializable_tagged");
    let tag_slot = if emitter.target.arch == Arch::AArch64 { 56 } else { 64 };
    abi::load_at_offset(emitter, result, tag_slot);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #8");                                  // only the nullable integer's null arm satisfies Serializable
            emitter.instruction("b.ne __rt_serializable_invalid");              // reject an integer payload without treating it as a heap owner
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 8");                                  // only the nullable integer's null arm satisfies Serializable
            emitter.instruction("jne __rt_serializable_invalid");               // reject an integer payload without heap cleanup
        }
    }
    emitter.label("__rt_serializable_void");
    abi::emit_store_zero_to_local_slot(emitter, 56);
    emitter.label("__rt_serializable_null");
    literal(emitter, b"N;");
    emitter.label("__rt_serializable_cleanup");
    abi::load_at_offset(emitter, result, 48);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_serializable_raw_cleanup");
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_jump(emitter, "__rt_serializable_done");
    emitter.label("__rt_serializable_raw_cleanup");
    abi::load_at_offset(emitter, result, 56);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_serializable_done");
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
    emitter.label("__rt_serializable_invalid_box");
    abi::load_at_offset(emitter, result, 48);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_jump(emitter, "__rt_serializable_invalid");
    emitter.label("__rt_serializable_invalid_heap");
    abi::load_at_offset(emitter, result, 72);
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_serializable_invalid");
    abi::load_at_offset(emitter, arg0, 8);
    abi::emit_call_label(emitter, "__rt_throw_serializable_result");
}

/// Loads a dense class-indexed table entry into the secondary scratch register.
fn load_table_entry(emitter: &mut Emitter, table: &str) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_symbol_address(emitter, scratch, table);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldr x10, [x10, x0, lsl #3]"),     // read the class-indexed custom hook entry
        Arch::X86_64 => emitter.instruction("mov r10, QWORD PTR [r10 + rax * 8]"), // read the class-indexed custom hook entry
    }
}

/// Appends envelope bytes through the ordinary serializer's bounded output convention.
fn literal(emitter: &mut Emitter, bytes: &[u8]) {
    match emitter.target.arch {
        Arch::AArch64 => super::emit_append_literal_aarch64(emitter, bytes, "Serializable envelope"),
        Arch::X86_64 => super::emit_append_literal_x86_64(emitter, bytes, "Serializable envelope"),
    }
}
