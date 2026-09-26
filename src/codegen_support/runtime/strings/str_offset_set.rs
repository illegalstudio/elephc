//! Purpose:
//! Emits the `__rt_str_offset_set` runtime helper behind PHP's string offset write
//! `$s[$i] = $v`, which builds the updated string as a fresh value.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//! - `crate::codegen::lower_inst::runtime_calls` for `RuntimeCallTarget::StringOffsetSet`.
//!
//! Key details:
//! - The subject is never mutated: the result is built in storage reserved through
//!   `__rt_concat_reserve`, so every other holder of the old string (a copy made by `$t = $s`)
//!   keeps its bytes. That is the whole copy-on-write contract for this write.
//! - Order of checks matches php 8.5: an offset before the start of the string warns
//!   `Illegal string offset N` and writes nothing (even for an empty value); only then does an
//!   empty value throw `Error`, and a longer value warn that only its first byte is used.
//! - Writing past the end pads the gap with spaces.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::value_error;

const FIRST_BYTE_MSG_LEN: usize =
    "Warning: Only the first byte will be assigned to the string offset\n".len();
const EMPTY_ASSIGN_MSG_LEN: usize = "Cannot assign an empty string to a string offset".len();

/// Emits `__rt_str_offset_set` for the active target.
///
/// # ABI (ARM64)
/// - Input: `x1`/`x2` = subject pointer/length, `x0` = offset, `x3`/`x4` = value pointer/length.
/// - Output: `x1`/`x2` = the updated string (concat scratch, or an owned heap block when large).
///
/// # ABI (x86_64)
/// - Input: `rax`/`rdx` = subject, `rcx` = offset, `rdi`/`rsi` = value.
/// - Output: `rax`/`rdx` = the updated string.
///
/// # Behavior
/// A negative offset counts from the end. One that still lands before the start warns and
/// returns an unchanged copy of the subject. An empty value throws PHP's `Error`; otherwise the
/// value's first byte is written at the offset, padding any gap past the end with spaces.
/// Clobbers every caller-saved register, because the reservation can reach `__rt_heap_alloc`.
pub fn emit_str_offset_set(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_offset_set_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_offset_set ---");
    emitter.label_global("__rt_str_offset_set");

    // Stack (80 bytes): [0] subject ptr, [8] subject len, [16] value ptr, [24] value len,
    // [32] resolved offset, [40] result length, [48] write flag, [64] frame linkage.
    emitter.instruction("sub sp, sp, #80");                                     // reserve the operand spill slots and frame linkage
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the subject pointer and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save the value pointer and length
    emitter.instruction("str x0, [sp, #32]");                                   // save the requested offset as the resolved one for now

    // -- resolve a negative offset against the subject length --
    emitter.instruction("cmp x0, #0");                                          // is the requested offset negative?
    emitter.instruction("b.ge __rt_str_offset_set_offset_ok");                  // a non-negative offset is already an index
    emitter.instruction("add x9, x2, x0");                                      // count a negative offset back from the end
    emitter.instruction("cmp x9, #0");                                          // does it still point before the first byte?
    emitter.instruction("b.lt __rt_str_offset_set_illegal");                    // php warns and writes nothing
    emitter.instruction("str x9, [sp, #32]");                                   // keep the resolved non-negative index

    // -- validate the value: empty throws, longer than one byte warns --
    emitter.label("__rt_str_offset_set_offset_ok");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the value length
    emitter.instruction("cbz x4, __rt_str_offset_set_empty");                   // an empty value cannot be written to an offset
    emitter.instruction("cmp x4, #1");                                          // is the value exactly one byte?
    emitter.instruction("b.eq __rt_str_offset_set_sized");                      // a single byte is written without a warning
    abi::emit_symbol_address(emitter, "x1", "_diag_string_offset_first_byte_msg");
    emitter.instruction(&format!("mov x2, #{}", FIRST_BYTE_MSG_LEN));           // pass the first-byte warning length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // warn that only the first byte is assigned

    // -- the result is the subject, widened to reach the offset --
    emitter.label("__rt_str_offset_set_sized");
    emitter.instruction("mov x9, #1");                                          // this path writes one byte
    emitter.instruction("str x9, [sp, #48]");                                   // remember that the byte write happens
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the resolved index
    emitter.instruction("add x9, x9, #1");                                      // the string must hold index + 1 bytes
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the subject length
    emitter.instruction("cmp x9, x10");                                         // is the index past the current end?
    emitter.instruction("csel x9, x9, x10, hi");                                // result length = max(length, index + 1)
    emitter.instruction("b __rt_str_offset_set_build");                         // reserve and fill the result

    // -- an index before the start: warn with the requested offset, keep the bytes --
    emitter.label("__rt_str_offset_set_illegal");
    abi::emit_call_label(emitter, "__rt_warn_illegal_string_offset");           // x0 still holds the requested offset
    emitter.instruction("str xzr, [sp, #48]");                                  // no byte is written on this path
    emitter.instruction("ldr x9, [sp, #8]");                                    // the result keeps the subject length

    // -- reserve the result and copy the subject into it --
    emitter.label("__rt_str_offset_set_build");
    emitter.instruction("str x9, [sp, #40]");                                   // save the result length across the reservation
    emitter.instruction("mov x0, x9");                                          // request storage for the whole result
    abi::emit_call_label(emitter, "__rt_concat_reserve");                       // reserve concat scratch or an owned heap block
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload the subject pointer and length
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_str_offset_set_copy");
    emitter.instruction("cmp x12, x2");                                         // has every subject byte been copied?
    emitter.instruction("b.hs __rt_str_offset_set_pad");                        // move on to the padding
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load one subject byte
    emitter.instruction("strb w13, [x0, x12]");                                 // store it into the result
    emitter.instruction("add x12, x12, #1");                                    // next byte
    emitter.instruction("b __rt_str_offset_set_copy");                          // continue copying

    // -- pad a gap past the old end with spaces, then write the byte --
    emitter.label("__rt_str_offset_set_pad");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the write flag
    emitter.instruction("cbz x9, __rt_str_offset_set_done");                    // an illegal offset leaves the copy untouched
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the resolved index
    emitter.instruction("mov w13, #32");                                        // php pads with ASCII spaces
    emitter.label("__rt_str_offset_set_pad_loop");
    emitter.instruction("cmp x12, x9");                                         // has the gap before the index been filled?
    emitter.instruction("b.hs __rt_str_offset_set_write");                      // yes: write the byte
    emitter.instruction("strb w13, [x0, x12]");                                 // store one padding space
    emitter.instruction("add x12, x12, #1");                                    // next padding byte
    emitter.instruction("b __rt_str_offset_set_pad_loop");                      // continue padding
    emitter.label("__rt_str_offset_set_write");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the value pointer
    emitter.instruction("ldrb w13, [x3]");                                      // load the value's first byte
    emitter.instruction("strb w13, [x0, x9]");                                  // write it at the resolved index

    emitter.label("__rt_str_offset_set_done");
    emitter.instruction("mov x1, x0");                                          // result pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // result length
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // advance the concat cursor for a scratch-backed result
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the updated string in x1/x2

    // -- an empty value: php throws a catchable Error --
    emitter.label("__rt_str_offset_set_empty");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame before throwing
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame before unwinding
    value_error::emit_throw_static_message_aarch64(
        emitter,
        "_spl_error_class_id",
        "_string_offset_empty_assign_msg",
        EMPTY_ASSIGN_MSG_LEN,
    );
}

/// Emits the x86_64 Linux variant of `__rt_str_offset_set`.
///
/// Same semantics as the ARM64 body. Inputs: `rax`/`rdx` subject, `rcx` offset, `rdi`/`rsi`
/// value; output `rax`/`rdx`.
fn emit_str_offset_set_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_offset_set ---");
    emitter.label_global("__rt_str_offset_set");

    // Frame: [rbp-8] subject ptr, [rbp-16] subject len, [rbp-24] value ptr, [rbp-32] value len,
    // [rbp-40] resolved offset, [rbp-48] result length, [rbp-56] write flag, [rbp-64] result ptr.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve the operand spill slots, keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the subject pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the subject length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the value pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the value length
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the requested offset as the resolved one for now

    // -- resolve a negative offset against the subject length --
    emitter.instruction("test rcx, rcx");                                       // is the requested offset negative?
    emitter.instruction("jge __rt_str_offset_set_offset_ok");                   // a non-negative offset is already an index
    emitter.instruction("mov r9, rdx");                                         // start from the subject length
    emitter.instruction("add r9, rcx");                                         // count a negative offset back from the end
    emitter.instruction("jl __rt_str_offset_set_illegal");                      // still before the first byte: php warns and writes nothing
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // keep the resolved non-negative index

    // -- validate the value: empty throws, longer than one byte warns --
    emitter.label("__rt_str_offset_set_offset_ok");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the value length
    emitter.instruction("test r9, r9");                                         // is the value empty?
    emitter.instruction("jz __rt_str_offset_set_empty");                        // an empty value cannot be written to an offset
    emitter.instruction("cmp r9, 1");                                           // is the value exactly one byte?
    emitter.instruction("je __rt_str_offset_set_sized");                        // a single byte is written without a warning
    abi::emit_symbol_address(emitter, "rdi", "_diag_string_offset_first_byte_msg");
    emitter.instruction(&format!("mov esi, {}", FIRST_BYTE_MSG_LEN));           // pass the first-byte warning length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // warn that only the first byte is assigned

    // -- the result is the subject, widened to reach the offset --
    emitter.label("__rt_str_offset_set_sized");
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // remember that the byte write happens
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the resolved index
    emitter.instruction("add rax, 1");                                          // the string must hold index + 1 bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the subject length
    emitter.instruction("cmp rax, r10");                                        // is the index past the current end?
    emitter.instruction("cmovb rax, r10");                                      // result length = max(length, index + 1)
    emitter.instruction("jmp __rt_str_offset_set_build");                       // reserve and fill the result

    // -- an index before the start: warn with the requested offset, keep the bytes --
    emitter.label("__rt_str_offset_set_illegal");
    emitter.instruction("mov rax, rcx");                                        // pass the requested offset to the warning helper
    abi::emit_call_label(emitter, "__rt_warn_illegal_string_offset");           // warn `Illegal string offset N`
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // no byte is written on this path
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the result keeps the subject length

    // -- reserve the result and copy the subject into it --
    emitter.label("__rt_str_offset_set_build");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the result length across the reservation
    abi::emit_call_label(emitter, "__rt_concat_reserve");                       // reserve concat scratch or an owned heap block
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the result pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the subject pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the subject length
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_str_offset_set_copy");
    emitter.instruction("cmp rcx, r9");                                         // has every subject byte been copied?
    emitter.instruction("jae __rt_str_offset_set_pad");                         // move on to the padding
    emitter.instruction("mov r10b, BYTE PTR [r8 + rcx]");                       // load one subject byte
    emitter.instruction("mov BYTE PTR [rax + rcx], r10b");                      // store it into the result
    emitter.instruction("add rcx, 1");                                          // next byte
    emitter.instruction("jmp __rt_str_offset_set_copy");                        // continue copying

    // -- pad a gap past the old end with spaces, then write the byte --
    emitter.label("__rt_str_offset_set_pad");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // is a byte written on this path?
    emitter.instruction("je __rt_str_offset_set_done");                         // an illegal offset leaves the copy untouched
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the resolved index
    emitter.label("__rt_str_offset_set_pad_loop");
    emitter.instruction("cmp rcx, r9");                                         // has the gap before the index been filled?
    emitter.instruction("jae __rt_str_offset_set_write");                       // yes: write the byte
    emitter.instruction("mov BYTE PTR [rax + rcx], 32");                        // store one padding space, as php does
    emitter.instruction("add rcx, 1");                                          // next padding byte
    emitter.instruction("jmp __rt_str_offset_set_pad_loop");                    // continue padding
    emitter.label("__rt_str_offset_set_write");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the value pointer
    emitter.instruction("mov r10b, BYTE PTR [r8]");                             // load the value's first byte
    emitter.instruction("mov BYTE PTR [rax + r9], r10b");                       // write it at the resolved index

    emitter.label("__rt_str_offset_set_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // result length
    abi::emit_call_label(emitter, "__rt_concat_publish");                       // advance the concat cursor for a scratch-backed result
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the updated string in rax/rdx

    // -- an empty value: php throws a catchable Error --
    emitter.label("__rt_str_offset_set_empty");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before unwinding
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before throwing
    value_error::emit_throw_static_message_x86_64(
        emitter,
        "_spl_error_class_id",
        "_string_offset_empty_assign_msg",
        EMPTY_ASSIGN_MSG_LEN,
    );
}
