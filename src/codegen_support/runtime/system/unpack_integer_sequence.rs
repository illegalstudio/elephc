//! Purpose:
//! Emits the runtime loop for PHP `unpack()` unnamed integer sequences such as
//! `C*` and `n*`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::unpack` after literal-format planning.
//!
//! Key details:
//! - Inputs are the source byte string, offset, element width, and byte order;
//!   output is a fresh associative hash with one-based integer keys.
//! - A trailing partial element is ignored for `*`, matching PHP, while an
//!   offset outside the input returns a null pointer for the caller to box as false.
//! - Every loop-carried value lives in a stack spill because hash allocation and
//!   insertion helpers freely clobber caller-saved registers on both targets.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the target-specific unnamed integer-sequence unpacker.
pub(crate) fn emit_unpack_integer_sequence(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 `C*`/`n*`/`N*`/`V*` runtime loop.
///
/// Input: x0=string pointer, x1=length, x2=offset, x3=width, x4=big-endian flag.
/// Output: x0=fresh hash, or zero when the offset is outside the input.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unpack integer sequence ---");
    emitter.label_global("__rt_unpack_integer_sequence");
    emitter.instruction("stp x29, x30, [sp, #-96]!");                         // preserve the caller and reserve loop spill slots
    emitter.instruction("mov x29, sp");                                       // establish the helper frame
    emitter.instruction("cmp x2, #0");                                       // reject a negative source offset
    emitter.instruction("b.lt __rt_unpack_integer_sequence_invalid");         // negative offsets are outside the input
    emitter.instruction("cmp x2, x1");                                       // compare offset with the string length
    emitter.instruction("b.hi __rt_unpack_integer_sequence_invalid");         // offset > length is invalid; offset == length yields []
    emitter.instruction("add x5, x0, x2");                                   // cursor = string pointer + offset
    emitter.instruction("sub x6, x1, x2");                                   // bytes remaining after the offset
    emitter.instruction("udiv x7, x6, x3");                                  // count = floor(remaining / element width)
    emitter.instruction("str x5, [sp, #16]");                                // spill the byte cursor
    emitter.instruction("str x3, [sp, #24]");                                // spill the element width
    emitter.instruction("str x4, [sp, #32]");                                // spill the endian flag
    emitter.instruction("str x7, [sp, #40]");                                // spill remaining element count
    emitter.instruction("mov x8, #1");                                       // PHP unnamed unpack keys start at one
    emitter.instruction("str x8, [sp, #48]");                                // spill the next integer key
    emitter.instruction("lsl x9, x7, #1");                                   // capacity candidate = count * 2
    emitter.instruction("mov x10, #16");                                     // minimum associative-hash capacity
    emitter.instruction("cmp x9, x10");                                      // clamp small result hashes
    emitter.instruction("csel x0, x10, x9, lo");                             // x0 = max(count * 2, 16)
    emitter.instruction("mov x1, #0");                                       // homogeneous hash value tag = integer
    emitter.instruction("bl __rt_hash_new");                                 // allocate the result hash
    emitter.instruction("str x0, [sp, #56]");                                // spill the current hash pointer
    emitter.instruction("b __rt_unpack_integer_sequence_check");             // enter at the remaining-count test

    emitter.label("__rt_unpack_integer_sequence_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                // reload current input cursor
    emitter.instruction("ldr x10, [sp, #24]");                               // reload element width
    emitter.instruction("cmp x10, #1");                                      // select the byte decoder
    emitter.instruction("b.eq __rt_unpack_integer_sequence_width1");          // one-byte unsigned integer
    emitter.instruction("cmp x10, #2");                                      // select the 16-bit decoder
    emitter.instruction("b.eq __rt_unpack_integer_sequence_width2");          // two-byte unsigned integer
    emitter.instruction("ldr w3, [x9]");                                     // load a native 32-bit unsigned integer
    emitter.instruction("ldr x11, [sp, #32]");                               // reload big-endian flag
    emitter.instruction("cbz x11, __rt_unpack_integer_sequence_value_ready"); // native/little-endian input is already host ordered
    emitter.instruction("rev w3, w3");                                       // convert network-order 32-bit input
    emitter.instruction("b __rt_unpack_integer_sequence_value_ready");        // join the decoded-value path

    emitter.label("__rt_unpack_integer_sequence_width2");
    emitter.instruction("ldrh w3, [x9]");                                    // load a native 16-bit unsigned integer
    emitter.instruction("ldr x11, [sp, #32]");                               // reload big-endian flag
    emitter.instruction("cbz x11, __rt_unpack_integer_sequence_value_ready"); // native/little-endian input is already host ordered
    emitter.instruction("rev16 w3, w3");                                     // convert network-order 16-bit input
    emitter.instruction("b __rt_unpack_integer_sequence_value_ready");        // join the decoded-value path

    emitter.label("__rt_unpack_integer_sequence_width1");
    emitter.instruction("ldrb w3, [x9]");                                    // load one unsigned byte

    emitter.label("__rt_unpack_integer_sequence_value_ready");
    emitter.instruction("add x9, x9, x10");                                  // advance by the selected field width
    emitter.instruction("str x9, [sp, #16]");                                // persist the advanced cursor
    emitter.instruction("ldr x1, [sp, #48]");                                // integer key payload
    emitter.instruction("mov x2, #-1");                                      // key high-word sentinel marks an integer key
    emitter.instruction("mov x4, #0");                                       // integer value high word is zero
    emitter.instruction("mov x5, #0");                                       // hash value tag 0 = integer
    emitter.instruction("ldr x0, [sp, #56]");                                // reload current result hash
    emitter.instruction("bl __rt_hash_set");                                 // insert the decoded value
    emitter.instruction("str x0, [sp, #56]");                                // preserve possible hash growth
    emitter.instruction("ldr x8, [sp, #48]");                                // reload the numeric key
    emitter.instruction("add x8, x8, #1");                                   // advance to the next one-based key
    emitter.instruction("str x8, [sp, #48]");                                // persist next key
    emitter.instruction("ldr x7, [sp, #40]");                                // reload remaining count
    emitter.instruction("sub x7, x7, #1");                                   // consume one element
    emitter.instruction("str x7, [sp, #40]");                                // persist remaining count

    emitter.label("__rt_unpack_integer_sequence_check");
    emitter.instruction("ldr x7, [sp, #40]");                                // reload remaining element count
    emitter.instruction("cbnz x7, __rt_unpack_integer_sequence_loop");        // decode until all complete elements are consumed
    emitter.instruction("ldr x0, [sp, #56]");                                // return the completed hash
    emitter.instruction("ldp x29, x30, [sp], #96");                           // restore the caller frame
    emitter.instruction("ret");                                               // return the fresh result owner

    emitter.label("__rt_unpack_integer_sequence_invalid");
    emitter.instruction("mov x0, #0");                                       // null pointer signals the false result
    emitter.instruction("ldp x29, x30, [sp], #96");                           // restore the caller frame
    emitter.instruction("ret");                                               // return without allocating a hash
}

/// Emits the x86_64 `C*`/`n*`/`N*`/`V*` runtime loop.
///
/// Input: rdi=string pointer, rsi=length, rdx=offset, rcx=width, r8=big-endian flag.
/// Output: rax=fresh hash, or zero when the offset is outside the input.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unpack integer sequence ---");
    emitter.label_global("__rt_unpack_integer_sequence");
    emitter.instruction("push rbp");                                          // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                      // establish the helper frame
    emitter.instruction("sub rsp, 80");                                       // reserve aligned loop spill slots
    emitter.instruction("cmp rdx, 0");                                       // reject a negative source offset
    emitter.instruction("jl __rt_unpack_integer_sequence_invalid_x");         // negative offsets are outside the input
    emitter.instruction("cmp rdx, rsi");                                     // compare offset with the string length
    emitter.instruction("ja __rt_unpack_integer_sequence_invalid_x");         // offset > length is invalid; offset == length yields []
    emitter.instruction("add rdi, rdx");                                     // cursor = string pointer + offset
    emitter.instruction("sub rsi, rdx");                                     // bytes remaining after the offset
    emitter.instruction("mov rax, rsi");                                     // dividend for complete-element count
    emitter.instruction("xor edx, edx");                                     // clear the unsigned-division high word
    emitter.instruction("div rcx");                                          // rax = floor(remaining / element width)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                     // spill the byte cursor
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                    // spill the element width
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                     // spill the endian flag
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                    // spill remaining element count
    emitter.instruction("mov QWORD PTR [rbp - 40], 1");                      // PHP unnamed unpack keys start at one
    emitter.instruction("lea rdi, [rax + rax]");                             // capacity candidate = count * 2
    emitter.instruction("cmp rdi, 16");                                      // clamp small result hashes
    emitter.instruction("jae __rt_unpack_integer_sequence_capacity_x");       // keep the computed capacity when large enough
    emitter.instruction("mov rdi, 16");                                      // minimum associative-hash capacity
    emitter.label("__rt_unpack_integer_sequence_capacity_x");
    emitter.instruction("xor esi, esi");                                     // homogeneous hash value tag = integer
    emitter.instruction("call __rt_hash_new");                               // allocate the result hash
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                    // spill the current hash pointer
    emitter.instruction("jmp __rt_unpack_integer_sequence_check_x");          // enter at the remaining-count test

    emitter.label("__rt_unpack_integer_sequence_loop_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                     // reload current input cursor
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                    // reload element width
    emitter.instruction("cmp r11, 1");                                       // select the byte decoder
    emitter.instruction("je __rt_unpack_integer_sequence_width1_x");          // one-byte unsigned integer
    emitter.instruction("cmp r11, 2");                                       // select the 16-bit decoder
    emitter.instruction("je __rt_unpack_integer_sequence_width2_x");          // two-byte unsigned integer
    emitter.instruction("mov ecx, DWORD PTR [r10]");                         // load a native 32-bit unsigned integer
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                      // is network byte order requested?
    emitter.instruction("je __rt_unpack_integer_sequence_value_ready_x");     // native/little-endian input is already host ordered
    emitter.instruction("bswap ecx");                                        // convert network-order 32-bit input
    emitter.instruction("jmp __rt_unpack_integer_sequence_value_ready_x");    // join the decoded-value path

    emitter.label("__rt_unpack_integer_sequence_width2_x");
    emitter.instruction("movzx ecx, WORD PTR [r10]");                        // load a native 16-bit unsigned integer
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                      // is network byte order requested?
    emitter.instruction("je __rt_unpack_integer_sequence_value_ready_x");     // native/little-endian input is already host ordered
    emitter.instruction("rol cx, 8");                                        // convert network-order 16-bit input
    emitter.instruction("jmp __rt_unpack_integer_sequence_value_ready_x");    // join the decoded-value path

    emitter.label("__rt_unpack_integer_sequence_width1_x");
    emitter.instruction("movzx ecx, BYTE PTR [r10]");                        // load one unsigned byte

    emitter.label("__rt_unpack_integer_sequence_value_ready_x");
    emitter.instruction("add r10, r11");                                     // advance by the selected field width
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                     // persist the advanced cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                    // integer key payload
    emitter.instruction("mov rdx, -1");                                      // key high-word sentinel marks an integer key
    emitter.instruction("xor r8d, r8d");                                    // integer value high word is zero
    emitter.instruction("xor r9d, r9d");                                    // hash value tag 0 = integer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                    // reload current result hash
    emitter.instruction("call __rt_hash_set");                               // insert the decoded value
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                    // preserve possible hash growth
    emitter.instruction("inc QWORD PTR [rbp - 40]");                         // advance to the next one-based key
    emitter.instruction("dec QWORD PTR [rbp - 32]");                         // consume one element

    emitter.label("__rt_unpack_integer_sequence_check_x");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                      // any complete elements left?
    emitter.instruction("jne __rt_unpack_integer_sequence_loop_x");           // decode the next element
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                    // return the completed hash
    emitter.instruction("add rsp, 80");                                       // release spill storage
    emitter.instruction("pop rbp");                                           // restore the caller frame pointer
    emitter.instruction("ret");                                               // return the fresh result owner

    emitter.label("__rt_unpack_integer_sequence_invalid_x");
    emitter.instruction("xor eax, eax");                                     // null pointer signals the false result
    emitter.instruction("add rsp, 80");                                       // release spill storage
    emitter.instruction("pop rbp");                                           // restore the caller frame pointer
    emitter.instruction("ret");                                               // return without allocating a hash
}
