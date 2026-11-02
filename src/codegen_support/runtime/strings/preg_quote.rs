//! Purpose:
//! Emits the `__rt_preg_quote` runtime helper assembly for PHP's `preg_quote`: prefixes every
//! PCRE metacharacter — and, when one is supplied, the caller's delimiter byte — with a single
//! backslash, and spells NUL as the four-character escape `\000`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The escaped set was derived by sweeping every byte `0..=255` through `php -n`, not copied
//!   from the manual: `! # $ ( ) * + - . : < = > ? [ \ ] ^ { | }`. Note what is NOT in it —
//!   `/` is escaped only when it is the delimiter, and `"` `'` `,` `;` `@` `~` never are.
//! - Membership is two window tests instead of a twenty-one-way compare chain. The set splits
//!   into `33..=94`, which fits one 64-bit bitmap, and the contiguous run `{`(123) `|`(124)
//!   `}`(125), which needs no bitmap because every byte in it is escaped.
//! - NUL becomes `\000` (four bytes), so the worst case is four output bytes per input byte.
//!   php-src reserves exactly `4 * len + 1` for the same reason.
//! - Only the FIRST byte of the delimiter matters, and it is compared byte-wise: php's own
//!   behavior for a multi-byte delimiter is to escape occurrences of its leading byte alone
//!   (measured: `preg_quote("aéb", "é")` yields `a\éb`, a backslash before `0xC3` only).
//! - A delimiter byte that is already a metacharacter is escaped once, not twice, because the
//!   two tests share one escape-prefix path.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Bitmap of the metacharacters in `33..=94`, indexed by `byte - 33`.
///
/// Bit `n` is set when the character `33 + n` must be prefixed with a backslash, covering
/// `!`(33) `#`(35) `$`(36) `(`(40) `)`(41) `*`(42) `+`(43) `-`(45) `.`(46) `:`(58) `<`(60)
/// `=`(61) `>`(62) `?`(63) `[`(91) `\`(92) `]`(93) `^`(94).
const PREG_QUOTE_ESCAPE_MASK: i64 = 0x3C00_0000_7A00_378D;

/// First byte covered by `PREG_QUOTE_ESCAPE_MASK`.
const PREG_QUOTE_WINDOW_START: u32 = 33;

/// Width of the bitmap window; `byte - 33` must stay below it to be tested.
const PREG_QUOTE_WINDOW_LEN: u32 = 62;

/// First byte of the trailing escaped run `{` `|` `}`, which lies outside the bitmap window.
const PREG_QUOTE_BRACE_START: u32 = 123;

/// Width of that trailing run. Every byte in it is escaped, so no bitmap test is needed.
const PREG_QUOTE_BRACE_LEN: u32 = 3;

/// Emits the `__rt_preg_quote` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x1` = source pointer, `x2` = source length,
///           `x3` = delimiter pointer, `x4` = delimiter length (0 = no delimiter).
///   Output: `x1` = result pointer, `x2` = result length.
///
/// ABI (x86_64 System V):
///   Input:  `rax` = source pointer, `rdx` = source length,
///           `rdi` = delimiter pointer, `rsi` = delimiter length (0 = no delimiter).
///   Output: `rax` = result pointer, `rdx` = result length.
///
/// An empty input reserves and publishes zero bytes, which matches PHP's empty-string result.
/// The result is published through `__rt_concat_publish`, so it lives in the shared concat
/// scratch while it fits and in an owned heap block otherwise.
pub fn emit_preg_quote(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_quote_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: preg_quote ---");
    emitter.label_global("__rt_preg_quote");

    // -- resolve the delimiter to a single byte before anything can clobber x3/x4 --
    // 256 marks "no delimiter": it can never equal a zero-extended source byte, so the
    // per-byte comparison needs no separate presence flag. It does not fit a byte, so the
    // spill slot below holds a full word.
    emitter.instruction("mov w9, #256");                                        // sentinel meaning the caller supplied no delimiter
    emitter.instruction("cbz x4, __rt_preg_quote_delim_ready");                 // an absent or empty delimiter keeps the sentinel
    emitter.instruction("ldrb w9, [x3]");                                       // only the delimiter's FIRST byte participates, like php-src
    emitter.label("__rt_preg_quote_delim_ready");

    // -- reserve the worst-case four-bytes-per-input-byte result before writing anything --
    // Frame: [0..16) source pointer/length, [16..32) saved x29/x30, [32) delimiter word.
    // The delimiter is SPILLED rather than parked in a register because it has to survive
    // `bl __rt_concat_reserve`: a linker-inserted branch island clobbers x16/x17 across a
    // `bl`, and every other scratch register is caller-saved.
    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for the source string and the delimiter word
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save the frame pointer and return address across the reservation call
    emitter.instruction("add x29, sp, #16");                                    // establish the preg_quote helper frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the source pointer and length across the reservation call
    emitter.instruction("str x9, [sp, #32]");                                   // save the resolved delimiter word across the reservation call
    emitter.instruction("adds x0, x2, x2");                                     // start the worst-case size at 2 * source length
    emitter.instruction("b.cs __rt_preg_quote_size_overflow");                  // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("adds x0, x0, x0");                                     // NUL expands to four bytes, so the reservation is 4 * source length
    emitter.instruction("b.cs __rt_preg_quote_size_overflow");                  // reject a wrapped size on the second doubling too
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov x9, x0");                                          // destination cursor
    emitter.instruction("mov x10, x0");                                         // save the result start for the published pointer
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the borrowed source pointer and length
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload the delimiter word into a register the escape loop never touches
    emitter.instruction("mov x11, x2");                                         // remaining source byte count
    abi::emit_load_int_immediate(emitter, "x15", PREG_QUOTE_ESCAPE_MASK);

    emitter.label("__rt_preg_quote_loop");
    emitter.instruction("cbz x11, __rt_preg_quote_done");                       // finish once every source byte has been consumed
    emitter.instruction("ldrb w12, [x1], #1");                                  // load the next source byte and advance the source cursor
    emitter.instruction("sub x11, x11, #1");                                    // record that one source byte has been consumed
    emitter.instruction("cbz w12, __rt_preg_quote_nul");                        // NUL is spelled \000 rather than backslash-escaped
    emitter.instruction("cmp w12, w5");                                         // does this byte match the caller's delimiter byte?
    emitter.instruction("b.eq __rt_preg_quote_escape");                         // the delimiter is escaped wherever it appears
    emitter.instruction(&format!("sub w13, w12, #{PREG_QUOTE_WINDOW_START}"));  // index the escape bitmap by shifting the byte into window space
    emitter.instruction(&format!("cmp w13, #{PREG_QUOTE_WINDOW_LEN}"));         // is the byte outside the bitmap window (unsigned, so low bytes wrap high)?
    emitter.instruction("b.hs __rt_preg_quote_braces");                         // bytes above the bitmap may still be one of `{` `|` `}`
    emitter.instruction("lsr x14, x15, x13");                                   // move this character's escape bit into position 0
    emitter.instruction("tbz x14, #0, __rt_preg_quote_store");                  // characters without an escape bit are copied through untouched
    emitter.instruction("b __rt_preg_quote_escape");                            // bitmap members take the shared escape-prefix path

    emitter.label("__rt_preg_quote_braces");
    emitter.instruction(&format!("sub w13, w12, #{PREG_QUOTE_BRACE_START}"));   // shift the byte into the trailing escaped run
    emitter.instruction(&format!("cmp w13, #{PREG_QUOTE_BRACE_LEN}"));          // `{` `|` `}` form a contiguous run, so a range test suffices
    emitter.instruction("b.hs __rt_preg_quote_store");                          // everything else is copied through untouched

    emitter.label("__rt_preg_quote_escape");
    emitter.instruction("mov w13, #92");                                        // ASCII backslash is the escape prefix
    emitter.instruction("strb w13, [x9], #1");                                  // write the escape prefix ahead of the metacharacter

    emitter.label("__rt_preg_quote_store");
    emitter.instruction("strb w12, [x9], #1");                                  // copy the source byte itself into the result
    emitter.instruction("b __rt_preg_quote_loop");                              // continue with the next source byte

    // -- NUL is the one byte php spells as an escape SEQUENCE rather than a prefixed byte --
    emitter.label("__rt_preg_quote_nul");
    emitter.instruction("mov w13, #92");                                        // ASCII backslash opens the four-character NUL escape
    emitter.instruction("strb w13, [x9], #1");                                  // write the backslash
    emitter.instruction("mov w13, #48");                                        // ASCII '0' fills the three remaining characters
    emitter.instruction("strb w13, [x9], #1");                                  // write the first '0'
    emitter.instruction("strb w13, [x9], #1");                                  // write the second '0'
    emitter.instruction("strb w13, [x9], #1");                                  // write the third '0'
    emitter.instruction("b __rt_preg_quote_loop");                              // continue with the next source byte

    emitter.label("__rt_preg_quote_done");
    emitter.instruction("mov x1, x10");                                         // return the escaped string start pointer
    emitter.instruction("sub x2, x9, x10");                                     // the written byte count is the result length
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the preg_quote helper frame
    emitter.instruction("ret");                                                 // return the escaped string as a PHP string pair

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_preg_quote_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits `__rt_preg_quote` for x86_64 Linux using the System V ABI.
///
/// Uses `bt` against the escape bitmap so the membership test needs no `cl` shift count,
/// which keeps `rcx` free as the source countdown register.
fn emit_preg_quote_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_quote ---");
    emitter.label_global("__rt_preg_quote");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the reservation and publish calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the borrowed source string
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the source pointer, length, and delimiter byte

    // -- resolve the delimiter to a single byte before anything can clobber rdi/rsi --
    // 256 marks "no delimiter": it can never equal a zero-extended source byte, so the
    // per-byte comparison needs no separate presence flag.
    emitter.instruction("mov r8d, 256");                                        // sentinel meaning the caller supplied no delimiter
    emitter.instruction("test rsi, rsi");                                       // did the caller supply a non-empty delimiter?
    emitter.instruction("je __rt_preg_quote_delim_ready_linux_x86_64");         // an absent or empty delimiter keeps the sentinel
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // only the delimiter's FIRST byte participates, like php-src
    emitter.label("__rt_preg_quote_delim_ready_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // park the delimiter byte across the reservation call

    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source byte count across the reservation call
    emitter.instruction("mov rax, rdx");                                        // seed the result size from the source byte count
    emitter.instruction("add rax, rax");                                        // start the worst-case size at 2 * source length
    emitter.instruction("jc __rt_preg_quote_size_overflow_linux_x86_64");       // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("add rax, rax");                                        // NUL expands to four bytes, so the reservation is 4 * source length
    emitter.instruction("jc __rt_preg_quote_size_overflow_linux_x86_64");       // reject a wrapped size on the second doubling too
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov r9, rax");                                         // destination cursor
    emitter.instruction("mov r10, rax");                                        // save the result start for the published pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the borrowed source pointer as a read cursor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the source byte count as a decrementing counter
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the resolved delimiter byte for the per-byte comparison
    emitter.instruction(&format!("mov r11, 0x{PREG_QUOTE_ESCAPE_MASK:x}"));     // materialize the escape bitmap for the membership test

    emitter.label("__rt_preg_quote_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been consumed
    emitter.instruction("je __rt_preg_quote_done_linux_x86_64");                // finish when the source string has been fully escaped
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next source byte and widen it for the window test
    emitter.instruction("add rsi, 1");                                          // advance the source cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // record that one source byte has been consumed
    emitter.instruction("test eax, eax");                                       // is this the one byte php spells as an escape SEQUENCE?
    emitter.instruction("je __rt_preg_quote_nul_linux_x86_64");                 // NUL is spelled \000 rather than backslash-escaped
    emitter.instruction("cmp eax, r8d");                                        // does this byte match the caller's delimiter byte?
    emitter.instruction("je __rt_preg_quote_escape_linux_x86_64");              // the delimiter is escaped wherever it appears
    emitter.instruction("mov edx, eax");                                        // copy the source byte before shifting it into window space
    emitter.instruction(&format!("sub edx, {PREG_QUOTE_WINDOW_START}"));        // index the escape bitmap by shifting the byte into window space
    emitter.instruction(&format!("cmp edx, {PREG_QUOTE_WINDOW_LEN}"));          // is the byte outside the bitmap window (unsigned, so low bytes wrap high)?
    emitter.instruction("jae __rt_preg_quote_braces_linux_x86_64");             // bytes above the bitmap may still be one of `{` `|` `}`
    emitter.instruction("bt r11, rdx");                                         // test this character's escape bit inside the bitmap
    emitter.instruction("jnc __rt_preg_quote_store_linux_x86_64");              // characters without an escape bit are copied through untouched
    emitter.instruction("jmp __rt_preg_quote_escape_linux_x86_64");             // bitmap members take the shared escape-prefix path

    emitter.label("__rt_preg_quote_braces_linux_x86_64");
    emitter.instruction("mov edx, eax");                                        // recompute the window index for the trailing escaped run
    emitter.instruction(&format!("sub edx, {PREG_QUOTE_BRACE_START}"));         // shift the byte into the trailing escaped run
    emitter.instruction(&format!("cmp edx, {PREG_QUOTE_BRACE_LEN}"));           // `{` `|` `}` form a contiguous run, so a range test suffices
    emitter.instruction("jae __rt_preg_quote_store_linux_x86_64");              // everything else is copied through untouched

    emitter.label("__rt_preg_quote_escape_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], 92");                               // write the ASCII backslash escape prefix
    emitter.instruction("add r9, 1");                                           // advance the destination cursor past the escape prefix

    emitter.label("__rt_preg_quote_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], al");                               // copy the source byte itself into the result
    emitter.instruction("add r9, 1");                                           // advance the destination cursor past the copied byte
    emitter.instruction("jmp __rt_preg_quote_loop_linux_x86_64");               // continue with the next source byte

    // -- NUL is the one byte php spells as an escape SEQUENCE rather than a prefixed byte --
    emitter.label("__rt_preg_quote_nul_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], 92");                               // write the backslash that opens the NUL escape
    emitter.instruction("mov BYTE PTR [r9 + 1], 48");                           // write the first '0'
    emitter.instruction("mov BYTE PTR [r9 + 2], 48");                           // write the second '0'
    emitter.instruction("mov BYTE PTR [r9 + 3], 48");                           // write the third '0'
    emitter.instruction("add r9, 4");                                           // advance the destination cursor past the four-character escape
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
