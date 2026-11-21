//! Purpose:
//! Joins small and staged string producers to checked concat result storage.
//!
//! Called from:
//! - Compiler-emitted string and platform query routines.
//!
//! Key details:
//! - The fallback owner is adoptable when the result escapes scratch lifetime.

use crate::{abi, Arch, Emitter};

use super::concat_scratch::emit_mark_reserved_result_temporary;

/// Returns a two-byte literal through the ordinary string result ABI.
pub fn emit_return_two_bytes(emitter: &mut impl Emitter, bytes: [u8; 2]) {
    emitter.emit_frame_prologue(16);
    let word = u16::from_le_bytes(bytes);
    let result = emitter.int_result_reg();
    emitter.emit_load_int_immediate(result, 2);
    emitter.emit_call_label("__rt_strbuf_reserve_temp");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov w9, #{word}"));                   // load both literal bytes in native byte order
            emitter.instruction("strh w9, [x0]");                               // write exactly the reserved bytes
            emitter.emit_reg_move("x1", "x0");
            emitter.emit_load_int_immediate("x2", 2);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov WORD PTR [rax], {word}"));        // write exactly the reserved bytes
            emitter.emit_load_int_immediate("rdx", 2);
        }
    }
    emitter.emit_call_label("__rt_concat_publish");
    emitter.emit_frame_restore(16);
    emitter.emit_return();
}

/// Reserves one output byte while keeping the incoming integer in x0 or rdi.
///
/// The returned destination is x1 on AArch64 or rax on x86-64. Heap fallback is marked
/// for the ordinary string persistence path. This runs at the producer's entry.
pub fn emit_reserve_byte_for_integer(emitter: &mut impl Emitter, ready: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("stp x0, x30, [sp, #-16]!");                    // preserve the incoming integer and return link
            abi::emit_load_int_immediate(emitter, "x0", 1);
            abi::emit_call_label(emitter, "__rt_concat_reserve");
            emit_mark_reserved_result_temporary(emitter, ready, ["r8", "r9", "r10"]);
            abi::emit_reg_move(emitter, "x1", "x0");
            emitter.instruction("ldp x0, x30, [sp], #16");                      // restore the input and caller return link
        }
        Arch::X86_64 => {
            emitter.instruction("push rdi");                                    // preserve the incoming integer and align the call
            abi::emit_load_int_immediate(emitter, "rax", 1);
            abi::emit_call_label(emitter, "__rt_concat_reserve");
            emit_mark_reserved_result_temporary(emitter, ready, ["r8", "r9", "r10"]);
            emitter.instruction("pop rdi");                                     // restore the producer's integer and entry stack
        }
    }
}

/// Copies a fully staged string to a checked transient destination.
///
/// Input and output use the ordinary string result registers. The caller's temporary
/// source may be on the stack and must live through the copy.
pub fn emit_copy_staged_result(emitter: &mut impl Emitter, ready: &str) {
    abi::emit_call_label(emitter, "__rt_strcopy");
    if emitter.arch() == Arch::AArch64 {
        abi::emit_reg_move(emitter, "x0", "x1");
    }
    emit_mark_reserved_result_temporary(emitter, ready, ["r8", "r9", "r10"]);
}

/// Allocates an exact adoptable string with room for one prefix byte, saving its base in the frame.
///
/// AArch64 slots are relative to sp. x86-64 slots are distances below rbp.
pub fn emit_reserve_with_prefix(emitter: &mut impl Emitter, length: usize, reservation: usize, ready: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [sp, #{length}]"));           // load the requested slice length
            emitter.instruction("adds x0, x0, #1");                             // leave one byte before the mutable slice
            emitter.instruction(&format!("b.cc {ready}"));                      // reject wrapped capacity before allocating
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {length}]")); // load the requested slice length
            emitter.instruction("add rax, 1");                                  // leave one byte before the mutable slice
            emitter.instruction(&format!("jnc {ready}"));                       // reject wrapped capacity before allocating
        }
    }
    abi::emit_jump(emitter, "__rt_alloc_overflow");
    emitter.label(ready);
    abi::emit_call_label(emitter, "__rt_str_new_owned");
    crate::heap_alloc::emit_stamp_result_kind(
        emitter, crate::HeapKind::new(crate::layout::CONCAT_TEMP_HEAP_KIND), "r10",
    );
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("str x0, [sp, #{reservation}]")), // save the exact base for retirement
        Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rbp - {reservation}], rax")), // save the exact base for retirement
    }
}

/// Boxes a result at its reservation base so persistence can adopt a heap fallback.
///
/// An absorbed carry leaves the first byte unused. Move that slice down before boxing
/// to keep the reservation adoptable, including when its source and destination overlap.
pub fn emit_box_reserved_slice(emitter: &mut impl Emitter, reservation: usize, ready: &str) {
    let move_bytes = emitter.extern_symbol("memmove");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [sp, #{reservation}]"));      // recover the original reservation base
            emitter.instruction("cmp x0, x1");                                  // did an absorbed carry leave the first byte unused?
            emitter.instruction(&format!("b.eq {ready}"));                      // a grown result already begins at the reservation base
            abi::emit_push_reg(emitter, "x2");
            abi::emit_call_label(emitter, &move_bytes);
            abi::emit_pop_reg(emitter, "x2");
            abi::emit_reg_move(emitter, "x1", "x0");
            emitter.label(ready);
            abi::emit_load_int_immediate(emitter, "x0", 1);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {reservation}]")); // recover the original reservation base
            emitter.instruction("cmp rax, rdi");                                // did an absorbed carry leave the first byte unused?
            emitter.instruction(&format!("je {ready}"));                        // a grown result already begins at the reservation base
            abi::emit_push_reg(emitter, "rsi");
            abi::emit_reg_move(emitter, "rdx", "rsi");
            abi::emit_reg_move(emitter, "rsi", "rdi");
            abi::emit_reg_move(emitter, "rdi", "rax");
            abi::emit_call_label(emitter, &move_bytes);
            abi::emit_pop_reg(emitter, "rsi");
            abi::emit_reg_move(emitter, "rdi", "rax");
            emitter.label(ready);
            abi::emit_load_int_immediate(emitter, "rax", 1);
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Copies a staged string into the bounded scratch tail and returns a borrowed string pair.
///
/// The caller has a frame and supplies the source in the usual string pair.
pub fn emit_copy_staged_to_scratch(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the staged source during the capacity check
            super::concat_cursor::emit_read(emitter, "x10");
            super::concat_cursor::emit_base_address(emitter, "x0");
            emitter.instruction("add x0, x0, x10");                             // address the first unused scratch byte
            abi::emit_reg_move(emitter, "x1", "x2");
            abi::emit_call_label(emitter, "__rt_concat_headroom");
            emitter.instruction("ldp x1, x2, [sp]");                            // reload the staged source for the copy
            emitter.bl_c("memcpy");
            emitter.instruction("ldr x2, [sp, #8]");                            // recover the initialized byte count
            emitter.instruction("add sp, sp, #16");                             // release the staged pair spill
            abi::emit_reg_move(emitter, "x1", "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");
            abi::emit_push_reg(emitter, "rdx");
            super::concat_cursor::emit_read(emitter, "r10");
            super::concat_cursor::emit_base_address(emitter, "rdi");
            emitter.instruction("add rdi, r10");                                // address the first unused scratch byte
            abi::emit_reg_move(emitter, "rsi", "rdx");
            abi::emit_call_label(emitter, "__rt_concat_headroom");
            emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");               // reload the staged source for the copy
            emitter.instruction("mov rdx, QWORD PTR [rsp]");                    // reload the source length
            emitter.bl_c("memcpy");
            abi::emit_pop_reg(emitter, "rdx");
            abi::emit_pop_reg(emitter, "r10");
        }
    }
    abi::emit_call_label(emitter, "__rt_concat_publish");
}
