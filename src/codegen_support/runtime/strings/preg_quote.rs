//! Purpose:
//! Emits the `__rt_preg_quote` runtime helper assembly for PHP's `preg_quote`: prefixes every
//! PCRE metacharacter (and an optional delimiter byte) with a backslash.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The escaped set is php-src's `preg_quote` switch verbatim: `.` `\` `+` `*` `?` `[` `^`
//!   `]` `$` `(` `)` `{` `}` `=` `!` `>` `<` `|` `:` `-` `#`, plus the first byte of a
//!   non-empty `$delimiter`. A NUL byte becomes the four bytes `\000`.
//! - Every escaped character is ASCII below 128, so membership is one bit of a 128-bit
//!   bitmap held in two 64-bit registers (`PREG_QUOTE_MASK_LOW` for bytes 0..=63,
//!   `PREG_QUOTE_MASK_HIGH` for bytes 64..=127).
//! - The worst case is four output bytes per input byte (`\000`), so `4 * len` is reserved
//!   through `__rt_concat_reserve` before the first store and the actual written length is
//!   handed to `__rt_concat_publish`.
//! - The helper never touches the regex engine, so it links without the optional PCRE2 package.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Bytes `preg_quote()` always escapes (NUL is handled separately as `\000`).
pub(crate) const PREG_QUOTE_ESCAPED: &[u8] = b".\\+*?[^]$(){}=!<>|:-#";

/// Builds one half of the 128-bit escape bitmap: bit `n` is set when byte `base + n` is escaped.
const fn preg_quote_mask(base: u8) -> u64 {
    let mut mask = 0u64;
    let mut index = 0;
    while index < PREG_QUOTE_ESCAPED.len() {
        let byte = PREG_QUOTE_ESCAPED[index];
        if byte >= base && byte < base + 64 {
            mask |= 1u64 << (byte - base);
        }
        index += 1;
    }
    mask
}

/// Escape bits for bytes 0..=63.
const PREG_QUOTE_MASK_LOW: u64 = preg_quote_mask(0);

/// Escape bits for bytes 64..=127.
const PREG_QUOTE_MASK_HIGH: u64 = preg_quote_mask(64);

/// Delimiter register value meaning "no delimiter": never equal to a loaded byte.
pub(crate) const PREG_QUOTE_NO_DELIMITER: i64 = 256;

/// Emits the `__rt_preg_quote` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x1` = source pointer, `x2` = source length, `x3` = delimiter byte or
///           `PREG_QUOTE_NO_DELIMITER`.
///   Output: `x1` = result pointer, `x2` = result length.
///
/// ABI (x86_64 System V):
///   Input:  `rax` = source pointer, `rdx` = source length, `rdi` = delimiter byte or
///           `PREG_QUOTE_NO_DELIMITER`.
///   Output: `rax` = result pointer, `rdx` = result length.
pub fn emit_preg_quote(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_quote_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: preg_quote ---");
    emitter.label_global("__rt_preg_quote");

    // -- reserve the worst-case four-bytes-per-input-byte result before writing anything --
    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for the borrowed source string and delimiter
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save the frame pointer and return address across the reservation call
    emitter.instruction("add x29, sp, #32");                                    // establish the preg_quote helper frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the source pointer and length across the reservation call
    emitter.instruction("str x3, [sp, #16]");                                   // save the delimiter byte across the reservation call
    emitter.instruction("lsr x9, x2, #62");                                     // isolate the bits a 4x size would shift out
    emitter.instruction("cbnz x9, __rt_preg_quote_size_overflow");              // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("lsl x0, x2, #2");                                      // compute the worst-case result size as 4 * source length
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov x9, x0");                                          // destination cursor
    emitter.instruction("mov x10, x0");                                         // save the result start for the published pointer
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the borrowed source pointer and length
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the delimiter byte
    emitter.instruction("mov x11, x2");                                         // remaining source byte count
    abi::emit_load_int_immediate(emitter, "x14", PREG_QUOTE_MASK_LOW as i64);
    abi::emit_load_int_immediate(emitter, "x15", PREG_QUOTE_MASK_HIGH as i64);

    emitter.label("__rt_preg_quote_loop");
    emitter.instruction("cbz x11, __rt_preg_quote_done");                       // finish once every source byte has been consumed
    emitter.instruction("ldrb w12, [x1], #1");                                  // load the next source byte and advance the source cursor
    emitter.instruction("sub x11, x11, #1");                                    // record that one source byte has been consumed
    emitter.instruction("cbz w12, __rt_preg_quote_nul");                        // a NUL byte is written as the four bytes \000
    emitter.instruction("cmp x12, x3");                                         // is this byte the caller's delimiter?
    emitter.instruction("b.eq __rt_preg_quote_escape");                         // the delimiter is escaped like a metacharacter
    emitter.instruction("cmp w12, #128");                                       // every metacharacter is plain ASCII
    emitter.instruction("b.hs __rt_preg_quote_store");                          // high-bit bytes are copied through untouched
    emitter.instruction("cmp w12, #64");                                        // select the bitmap half that covers this byte
    emitter.instruction("b.hs __rt_preg_quote_high");                           // bytes 64..=127 are tested against the high half
    emitter.instruction("lsr x13, x14, x12");                                   // move this byte's escape bit into position 0
    emitter.instruction("tbz x13, #0, __rt_preg_quote_store");                  // bytes without an escape bit are copied through untouched
    emitter.instruction("b __rt_preg_quote_escape");                            // write the escape prefix ahead of the metacharacter

    emitter.label("__rt_preg_quote_high");
    emitter.instruction("sub w13, w12, #64");                                   // index the high bitmap half
    emitter.instruction("lsr x13, x15, x13");                                   // move this byte's escape bit into position 0
    emitter.instruction("tbz x13, #0, __rt_preg_quote_store");                  // bytes without an escape bit are copied through untouched

    emitter.label("__rt_preg_quote_escape");
    emitter.instruction("mov w13, #92");                                        // ASCII backslash is the escape prefix
    emitter.instruction("strb w13, [x9], #1");                                  // write the escape prefix ahead of the metacharacter

    emitter.label("__rt_preg_quote_store");
    emitter.instruction("strb w12, [x9], #1");                                  // copy the source byte itself into the result
    emitter.instruction("b __rt_preg_quote_loop");                              // continue with the next source byte

    emitter.label("__rt_preg_quote_nul");
    emitter.instruction("mov w13, #92");                                        // ASCII backslash starts the octal NUL escape
    emitter.instruction("strb w13, [x9], #1");                                  // write the backslash of \000
    emitter.instruction("mov w13, #48");                                        // ASCII zero digit
    emitter.instruction("strb w13, [x9], #1");                                  // write the first zero of \000
    emitter.instruction("strb w13, [x9], #1");                                  // write the second zero of \000
    emitter.instruction("strb w13, [x9], #1");                                  // write the third zero of \000
    emitter.instruction("b __rt_preg_quote_loop");                              // continue with the next source byte

    emitter.label("__rt_preg_quote_done");
    emitter.instruction("mov x1, x10");                                         // return the escaped string start pointer
    emitter.instruction("sub x2, x9, x10");                                     // the written byte count is the result length
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the preg_quote helper frame
    emitter.instruction("ret");                                                 // return the escaped string as a PHP string pair

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_preg_quote_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits `__rt_preg_quote` for x86_64 Linux using the System V ABI.
fn emit_preg_quote_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_quote ---");
    emitter.label_global("__rt_preg_quote");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the reservation and publish calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the borrowed source string
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the source string and delimiter
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source byte count across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the delimiter byte across the reservation call
    emitter.instruction("mov rcx, rdx");                                        // copy the source length before testing the 4x size for overflow
    emitter.instruction("shr rcx, 62");                                         // isolate the bits a 4x size would shift out
    emitter.instruction("jnz __rt_preg_quote_size_overflow_linux_x86_64");      // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("lea rax, [rdx * 4]");                                  // compute the worst-case result size as 4 * source length
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov r9, rax");                                         // destination cursor
    emitter.instruction("mov r10, rax");                                        // save the result start for the published pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the borrowed source pointer as a read cursor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the source byte count as a decrementing counter
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the delimiter byte
    emitter.instruction(&format!("mov r11, 0x{PREG_QUOTE_MASK_LOW:x}"));        // materialize the low escape bitmap half (bytes 0..=63)
    emitter.instruction(&format!("mov r8, 0x{PREG_QUOTE_MASK_HIGH:x}"));        // materialize the high escape bitmap half (bytes 64..=127)

    emitter.label("__rt_preg_quote_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been consumed
    emitter.instruction("je __rt_preg_quote_done_linux_x86_64");                // finish when the source string has been fully escaped
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next source byte and widen it for the bitmap test
    emitter.instruction("add rsi, 1");                                          // advance the source cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // record that one source byte has been consumed
    emitter.instruction("test eax, eax");                                       // is this a NUL byte?
    emitter.instruction("je __rt_preg_quote_nul_linux_x86_64");                 // a NUL byte is written as the four bytes \000
    emitter.instruction("cmp rax, rdi");                                        // is this byte the caller's delimiter?
    emitter.instruction("je __rt_preg_quote_escape_linux_x86_64");              // the delimiter is escaped like a metacharacter
    emitter.instruction("cmp eax, 128");                                        // every metacharacter is plain ASCII
    emitter.instruction("jae __rt_preg_quote_store_linux_x86_64");              // high-bit bytes are copied through untouched
    emitter.instruction("mov edx, eax");                                        // copy the byte as the bitmap bit index
    emitter.instruction("cmp edx, 64");                                         // select the bitmap half that covers this byte
    emitter.instruction("jae __rt_preg_quote_high_linux_x86_64");               // bytes 64..=127 are tested against the high half
    emitter.instruction("bt r11, rdx");                                         // test this byte's escape bit in the low half
    emitter.instruction("jc __rt_preg_quote_escape_linux_x86_64");              // metacharacters get the escape prefix
    emitter.instruction("jmp __rt_preg_quote_store_linux_x86_64");              // every other byte is copied through untouched

    emitter.label("__rt_preg_quote_high_linux_x86_64");
    emitter.instruction("sub edx, 64");                                         // index the high bitmap half
    emitter.instruction("bt r8, rdx");                                          // test this byte's escape bit in the high half
    emitter.instruction("jnc __rt_preg_quote_store_linux_x86_64");              // bytes without an escape bit are copied through untouched

    emitter.label("__rt_preg_quote_escape_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], 92");                               // write the ASCII backslash escape prefix
    emitter.instruction("add r9, 1");                                           // advance the destination cursor past the escape prefix

    emitter.label("__rt_preg_quote_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], al");                               // copy the source byte itself into the result
    emitter.instruction("add r9, 1");                                           // advance the destination cursor past the copied byte
    emitter.instruction("jmp __rt_preg_quote_loop_linux_x86_64");               // continue with the next source byte

    emitter.label("__rt_preg_quote_nul_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], 92");                               // write the backslash of \000
    emitter.instruction("mov BYTE PTR [r9 + 1], 48");                           // write the first zero of \000
    emitter.instruction("mov BYTE PTR [r9 + 2], 48");                           // write the second zero of \000
    emitter.instruction("mov BYTE PTR [r9 + 3], 48");                           // write the third zero of \000
    emitter.instruction("add r9, 4");                                           // advance the destination cursor past the octal escape
    emitter.instruction("jmp __rt_preg_quote_loop_linux_x86_64");               // continue with the next source byte

    emitter.label("__rt_preg_quote_done_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // return the escaped string start pointer
    emitter.instruction("mov rdx, r9");                                         // copy the destination cursor into the length scratch register
    emitter.instruction("sub rdx, r10");                                        // the written byte count is the result length
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 32");                                         // release the preg_quote spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the escaped string as a PHP string pair

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_preg_quote_size_overflow_linux_x86_64");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
