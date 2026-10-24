//! Purpose:
//! Emits the shared `__rt_digest_to_string` runtime helper that turns a raw
//! digest (pointer + length + binary flag) into a `_concat_buf`-backed PHP
//! string. Shared by `__rt_hash`, `__rt_md5`, and `__rt_sha1`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Length-driven loops: any digest size works. A zero binary flag writes a
//!   lowercase hex string (two chars per byte); a non-zero flag copies the raw
//!   bytes verbatim.
//! - Reserves its result through `__rt_concat_reserve` and publishes it with
//!   `__rt_concat_publish`, like the other concat-backed string helpers: a nearly
//!   full scratch hands back a heap block instead of letting the digest run past
//!   the end of `_concat_buf` into the adjacent globals.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the shared `__rt_digest_to_string` runtime helper.
///
/// Converts a raw digest into a concat-backed PHP string (scratch when it fits,
/// a transient heap block otherwise). When the binary flag is zero it writes a lowercase hex string
/// (two chars per byte); otherwise it copies the raw bytes verbatim. The loops
/// are length-driven so any digest size works.
///
/// Input registers:
///   AArch64: x0 = raw digest ptr, x1 = length, x2 = binary flag.
///   x86_64:  rdi = raw digest ptr, rsi = length, rdx = binary flag.
///
/// Output registers (PHP string ptr/len pair):
///   AArch64: x1 = ptr, x2 = len.
///   x86_64:  rax = ptr, rdx = len.
pub fn emit_digest_to_string(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_digest_to_string_x86_64(emitter);
        return;
    }
    emit_digest_to_string_aarch64(emitter);
}

/// Emits the AArch64 variant of `__rt_digest_to_string`.
fn emit_digest_to_string_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: digest_to_string ---");
    emitter.label_global("__rt_digest_to_string");

    // -- reserve two bytes per digest byte, enough for both the hex and raw forms --
    emitter.instruction("sub sp, sp, #48");                                     // reserve spill slots for the digest arguments and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across the reservation
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("stp x0, x1, [sp]");                                    // keep the digest pointer and length across the reservation
    emitter.instruction("str x2, [sp, #16]");                                   // keep the binary flag across the reservation
    emitter.instruction("lsl x0, x1, #1");                                      // a hex digest needs two characters per byte
    emitter.instruction("bl __rt_concat_reserve");                              // scratch when it fits, an owned heap block otherwise
    emitter.instruction("mov x9, x0");                                          // destination cursor = the reservation
    emitter.instruction("mov x10, x0");                                         // preserve the result start pointer across the write loop
    emitter.instruction("ldp x11, x12, [sp]");                                  // source = raw digest bytes, remaining = its length
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the binary flag

    // -- binary flag chooses raw copy vs lowercase hex --
    emitter.instruction("cbnz x2, __rt_digest_raw_loop");                       // a non-zero binary flag copies the raw bytes verbatim

    // -- lowercase hex loop: two chars per digest byte --
    emitter.label("__rt_digest_hex_loop");
    emitter.instruction("cbz x12, __rt_digest_done");                           // all digest bytes converted to hex
    emitter.instruction("ldrb w13, [x11], #1");                                 // load one digest byte and advance the source cursor
    emitter.instruction("sub x12, x12, #1");                                    // decrement the remaining-byte counter
    // -- high nibble --
    emitter.instruction("lsr w14, w13, #4");                                    // extract the high 4 bits of the digest byte
    emitter.instruction("cmp w14, #10");                                        // does the high nibble need an 'a'-'f' digit?
    emitter.instruction("b.ge __rt_digest_hi_af");                              // map 10-15 to 'a'-'f'
    emitter.instruction("add w14, w14, #48");                                   // map 0-9 to '0'-'9'
    emitter.instruction("b __rt_digest_hi_st");                                 // store the high hex digit
    emitter.label("__rt_digest_hi_af");
    emitter.instruction("add w14, w14, #87");                                   // map 10-15 to 'a'-'f'
    emitter.label("__rt_digest_hi_st");
    emitter.instruction("strb w14, [x9], #1");                                  // write the high hex character and advance the destination
    // -- low nibble --
    emitter.instruction("and w14, w13, #0xf");                                  // extract the low 4 bits of the digest byte
    emitter.instruction("cmp w14, #10");                                        // does the low nibble need an 'a'-'f' digit?
    emitter.instruction("b.ge __rt_digest_lo_af");                              // map 10-15 to 'a'-'f'
    emitter.instruction("add w14, w14, #48");                                   // map 0-9 to '0'-'9'
    emitter.instruction("b __rt_digest_lo_st");                                 // store the low hex digit
    emitter.label("__rt_digest_lo_af");
    emitter.instruction("add w14, w14, #87");                                   // map 10-15 to 'a'-'f'
    emitter.label("__rt_digest_lo_st");
    emitter.instruction("strb w14, [x9], #1");                                  // write the low hex character and advance the destination
    emitter.instruction("b __rt_digest_hex_loop");                              // process the next digest byte

    // -- raw-bytes loop: copy each digest byte verbatim --
    emitter.label("__rt_digest_raw_loop");
    emitter.instruction("cbz x12, __rt_digest_done");                           // all raw digest bytes copied
    emitter.instruction("ldrb w13, [x11], #1");                                 // load one digest byte and advance the source cursor
    emitter.instruction("sub x12, x12, #1");                                    // decrement the remaining-byte counter
    emitter.instruction("strb w13, [x9], #1");                                  // write the raw digest byte and advance the destination
    emitter.instruction("b __rt_digest_raw_loop");                              // process the next raw digest byte

    // -- publish the result string and advance the concat offset --
    emitter.label("__rt_digest_done");
    emitter.instruction("mov x1, x10");                                         // result pointer = formatted-digest start
    emitter.instruction("sub x2, x9, x10");                                     // result length = bytes written
    emitter.instruction("bl __rt_concat_publish");                              // advance _concat_off only for a scratch-backed result
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the PHP string ptr/len in x1/x2
}

/// Emits the x86_64 variant of `__rt_digest_to_string`.
fn emit_digest_to_string_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: digest_to_string ---");
    emitter.label_global("__rt_digest_to_string");

    // -- reserve two bytes per digest byte, enough for both the hex and raw forms --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the digest arguments
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the digest pointer across the reservation
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the digest length across the reservation
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // keep the binary flag across the reservation
    emitter.instruction("lea rax, [rsi + rsi]");                                // a hex digest needs two characters per byte
    emitter.instruction("call __rt_concat_reserve");                            // scratch when it fits, an owned heap block otherwise
    emitter.instruction("mov r11, rax");                                        // destination cursor = the reservation
    emitter.instruction("mov r8, rax");                                         // preserve the result start pointer across the write loop
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // source = raw digest bytes
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // remaining digest bytes to consume
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the binary flag

    // -- binary flag chooses raw copy vs lowercase hex --
    emitter.instruction("test rdx, rdx");                                       // is the binary flag set?
    emitter.instruction("jnz __rt_digest_raw_loop_linux_x86_64");               // a non-zero binary flag copies the raw bytes verbatim

    // -- lowercase hex loop: two chars per digest byte --
    emitter.label("__rt_digest_hex_loop_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // any remaining digest bytes to format?
    emitter.instruction("jz __rt_digest_done_linux_x86_64");                    // all digest bytes converted to hex
    emitter.instruction("movzx edx, BYTE PTR [rcx]");                           // load one digest byte before splitting it into nibbles
    emitter.instruction("add rcx, 1");                                          // advance the source cursor past the consumed byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining-byte counter
    emitter.instruction("mov eax, edx");                                        // copy the digest byte before extracting its high nibble
    emitter.instruction("shr al, 4");                                           // isolate the high 4 bits of the digest byte
    emitter.instruction("cmp al, 10");                                          // does the high nibble need an 'a'-'f' digit?
    emitter.instruction("jae __rt_digest_hi_af_linux_x86_64");                  // map 10-15 to 'a'-'f'
    emitter.instruction("add al, 48");                                          // map 0-9 to '0'-'9'
    emitter.instruction("jmp __rt_digest_hi_store_linux_x86_64");               // store the high hex digit
    emitter.label("__rt_digest_hi_af_linux_x86_64");
    emitter.instruction("add al, 87");                                          // map 10-15 to 'a'-'f'
    emitter.label("__rt_digest_hi_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the high hex character
    emitter.instruction("add r11, 1");                                          // advance the destination past the high hex digit
    emitter.instruction("mov eax, edx");                                        // reload the digest byte before extracting its low nibble
    emitter.instruction("and al, 15");                                          // isolate the low 4 bits of the digest byte
    emitter.instruction("cmp al, 10");                                          // does the low nibble need an 'a'-'f' digit?
    emitter.instruction("jae __rt_digest_lo_af_linux_x86_64");                  // map 10-15 to 'a'-'f'
    emitter.instruction("add al, 48");                                          // map 0-9 to '0'-'9'
    emitter.instruction("jmp __rt_digest_lo_store_linux_x86_64");               // store the low hex digit
    emitter.label("__rt_digest_lo_af_linux_x86_64");
    emitter.instruction("add al, 87");                                          // map 10-15 to 'a'-'f'
    emitter.label("__rt_digest_lo_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the low hex character
    emitter.instruction("add r11, 1");                                          // advance the destination past the low hex digit
    emitter.instruction("jmp __rt_digest_hex_loop_linux_x86_64");               // process the next digest byte

    // -- raw-bytes loop: copy each digest byte verbatim --
    emitter.label("__rt_digest_raw_loop_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // any remaining raw digest bytes to copy?
    emitter.instruction("jz __rt_digest_done_linux_x86_64");                    // all raw digest bytes copied
    emitter.instruction("movzx eax, BYTE PTR [rcx]");                           // load one raw digest byte
    emitter.instruction("add rcx, 1");                                          // advance the source cursor past the consumed byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining-byte counter
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the raw digest byte verbatim
    emitter.instruction("add r11, 1");                                          // advance the destination past the raw byte
    emitter.instruction("jmp __rt_digest_raw_loop_linux_x86_64");               // process the next raw digest byte

    // -- publish the result string and advance the concat offset --
    emitter.label("__rt_digest_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // result pointer = formatted-digest start
    emitter.instruction("mov rdx, r11");                                        // copy the final destination cursor before computing the length
    emitter.instruction("sub rdx, r8");                                         // result length = bytes written
    emitter.instruction("call __rt_concat_publish");                            // advance _concat_off only for a scratch-backed result
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the PHP string ptr/len in rax/rdx
}
