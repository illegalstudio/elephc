//! Purpose:
//! Emits the shared `__rt_digest_to_string` runtime helper that turns a raw
//! digest (pointer + length + binary flag) into a `_concat_buf`-backed PHP
//! string. Shared by `__rt_hash`, `__rt_md5`, and `__rt_sha1`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Length-driven loops: any digest size works. A zero binary flag writes a
//!   lowercase hex string (two chars per byte); a non-zero flag copies the raw
//!   bytes verbatim.
//! - Writes into the global `_concat_buf` and advances `_concat_off`, matching
//!   the ownership contract of the other concat-backed string helpers.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the shared `__rt_digest_to_string` runtime helper.
///
/// Converts a raw digest into a `_concat_buf`-backed PHP string and advances
/// `_concat_off`. When the binary flag is zero it writes a lowercase hex string
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

    // -- resolve the concat-buffer destination cursor --
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load the current concat-buffer write offset
    abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute the destination pointer for the formatted digest
    emitter.instruction("mov x10, x9");                                         // preserve the result start pointer across the write loop
    emitter.instruction("mov x11, x0");                                         // source = raw digest bytes
    emitter.instruction("mov x12, x1");                                         // remaining digest bytes to consume

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
    emitter.instruction("ldr x8, [x6]");                                        // reload the concat-buffer write offset
    emitter.instruction("add x8, x8, x2");                                      // advance it past the formatted digest
    emitter.instruction("str x8, [x6]");                                        // persist the updated concat-buffer write offset
    emitter.instruction("ret");                                                 // return the PHP string ptr/len in x1/x2
}

/// Emits the x86_64 variant of `__rt_digest_to_string`.
fn emit_digest_to_string_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: digest_to_string ---");
    emitter.label_global("__rt_digest_to_string");

    // -- resolve the concat-buffer destination cursor --
    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer write offset
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r8]");                                 // compute the destination pointer for the formatted digest
    emitter.instruction("mov r8, r11");                                         // preserve the result start pointer across the write loop
    emitter.instruction("mov rcx, rdi");                                        // source = raw digest bytes
    emitter.instruction("mov r9, rsi");                                         // remaining digest bytes to consume

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
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset
    emitter.instruction("add rcx, rdx");                                        // advance it past the formatted digest
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset
    emitter.instruction("ret");                                                 // return the PHP string ptr/len in rax/rdx
}
