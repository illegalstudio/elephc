//! Purpose:
//! Emits `__rt_strnatcmp`, the runtime implementation of PHP's natural-order string
//! comparison (`strnatcmp_ex`), used by `ksort()`/`krsort()` under `SORT_NATURAL`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//! - `__rt_key_compare_flagged` in `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The algorithm is php-src's, quirks included: leading zeros are skipped once before the
//!   loop rather than per run; whitespace is skipped on each side independently; a run that
//!   starts with `0` on either side compares left-aligned (`"a0.5"` sorts before `"a0.10"`)
//!   and any other run compares by length first. Two operands that differ only in skipped
//!   bytes compare EQUAL, which is why `strnatcmp("a 7", "a7")` is `0`.
//! - PHP reads the NUL byte its strings always carry when a whitespace run walks off the end.
//!   Elephc strings are pointer/length pairs with no terminator, so the skip synthesizes that
//!   `0` at the boundary instead of reading past it.
//! - Case folding covers ASCII `a`-`z` and stops there. php-src folds through the process's
//!   `LC_CTYPE` table, so its own answer above `0x7F` follows the host C library: under
//!   `LC_CTYPE=C` it folds ASCII only, which is what this emits, and under the `C.UTF-8` the
//!   CLI forces at startup Darwin's single-byte table also folds `0xE0..0xFE` while glibc's
//!   does not. Folding through a table would mean the emitted binary answering for the machine
//!   that compiled it, so this bakes the `C` answer on every target instead.
//! - Only the SIGN of the result is contractual, as in php-src.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_strnatcmp` for the active target.
///
/// Input: AArch64 `x0` = case-folding flag, `x1`/`x2` = left pointer and length, `x3`/`x4` =
/// right pointer and length; x86_64 `rdi` = case-folding flag, `rsi`/`rdx` = left pointer and
/// length, `rcx`/`r8` = right pointer and length.
///
/// Output: AArch64 `x0` / x86_64 `rax` = a negative value, zero, or a positive value.
///
/// The helper makes no calls and allocates nothing.
pub fn emit_strnatcmp(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strnatcmp_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strnatcmp (PHP natural order) ---");
    emitter.label_global("__rt_strnatcmp");
    emitter.instruction("cbz x2, __rt_snc_empty");                              // an empty operand is decided by length alone
    emitter.instruction("cbz x4, __rt_snc_empty");                              // the same holds for an empty right operand
    emitter.instruction("mov x5, x1");                                          // ap = a
    emitter.instruction("add x6, x1, x2");                                      // aend = a + a_len
    emitter.instruction("mov x7, x3");                                          // bp = b
    emitter.instruction("add x8, x3, x4");                                      // bend = b + b_len
    emitter.instruction("ldrb w9, [x5]");                                       // ca = *ap
    emitter.instruction("ldrb w10, [x7]");                                      // cb = *bp

    emitter.comment("-- leading zeros are skipped once, before the main loop --");
    emitter.label("__rt_snc_lz_a");
    emitter.instruction("cmp w9, #48");                                         // only a literal zero starts a skippable run
    emitter.instruction("b.ne __rt_snc_lz_b");                                  // the left operand is already positioned
    emitter.instruction("add x11, x5, #1");                                     // look at the byte after the zero
    emitter.instruction("cmp x11, x6");                                         // PHP requires a byte to remain inside the operand
    emitter.instruction("b.hs __rt_snc_lz_b");                                  // a trailing zero is kept as a digit
    emitter.instruction("ldrb w12, [x11]");                                     // load the following byte
    emitter.instruction("sub w13, w12, #48");                                   // normalize it against the digit range
    emitter.instruction("cmp w13, #9");                                         // is another digit following?
    emitter.instruction("b.hi __rt_snc_lz_b");                                  // a zero before a non-digit stays significant
    emitter.instruction("mov x5, x11");                                         // consume the leading zero
    emitter.instruction("mov w9, w12");                                         // ca = the digit that followed it
    emitter.instruction("b __rt_snc_lz_a");                                     // keep skipping leading zeros
    emitter.label("__rt_snc_lz_b");
    emitter.instruction("cmp w10, #48");                                        // mirror the skip on the right operand
    emitter.instruction("b.ne __rt_snc_ws_a");                                  // enter the main loop
    emitter.instruction("add x11, x7, #1");                                     // look at the byte after the zero
    emitter.instruction("cmp x11, x8");                                         // a byte must remain inside the operand
    emitter.instruction("b.hs __rt_snc_ws_a");                                  // a trailing zero is kept as a digit
    emitter.instruction("ldrb w12, [x11]");                                     // load the following byte
    emitter.instruction("sub w13, w12, #48");                                   // normalize it against the digit range
    emitter.instruction("cmp w13, #9");                                         // is another digit following?
    emitter.instruction("b.hi __rt_snc_ws_a");                                  // a zero before a non-digit stays significant
    emitter.instruction("mov x7, x11");                                         // consume the leading zero
    emitter.instruction("mov w10, w12");                                        // cb = the digit that followed it
    emitter.instruction("b __rt_snc_lz_b");                                     // keep skipping leading zeros

    emitter.comment("-- main loop: whitespace, then a digit run, then one plain byte --");
    emitter.label("__rt_snc_ws_a");
    emitter.instruction("cmp w9, #32");                                         // ASCII space is whitespace
    emitter.instruction("b.eq __rt_snc_ws_a_step");                             // skip it
    emitter.instruction("sub w11, w9, #9");                                     // normalize the tab/newline/vtab/formfeed/return range
    emitter.instruction("cmp w11, #4");                                         // are we inside 9..13?
    emitter.instruction("b.hi __rt_snc_ws_b");                                  // the left cursor rests on a significant byte
    emitter.label("__rt_snc_ws_a_step");
    emitter.instruction("add x5, x5, #1");                                      // consume one whitespace byte
    emitter.instruction("mov w9, #0");                                          // past the operand PHP reads its NUL terminator
    emitter.instruction("cmp x5, x6");                                          // did the operand end?
    emitter.instruction("b.hs __rt_snc_ws_a");                                  // the synthetic NUL ends the skip on the next test
    emitter.instruction("ldrb w9, [x5]");                                       // ca = the next byte
    emitter.instruction("b __rt_snc_ws_a");                                     // keep skipping whitespace
    emitter.label("__rt_snc_ws_b");
    emitter.instruction("cmp w10, #32");                                        // mirror the skip on the right operand
    emitter.instruction("b.eq __rt_snc_ws_b_step");                             // skip it
    emitter.instruction("sub w11, w10, #9");                                    // normalize the tab/newline/vtab/formfeed/return range
    emitter.instruction("cmp w11, #4");                                         // are we inside 9..13?
    emitter.instruction("b.hi __rt_snc_run");                                   // both cursors rest on significant bytes
    emitter.label("__rt_snc_ws_b_step");
    emitter.instruction("add x7, x7, #1");                                      // consume one whitespace byte
    emitter.instruction("mov w10, #0");                                         // past the operand PHP reads its NUL terminator
    emitter.instruction("cmp x7, x8");                                          // did the operand end?
    emitter.instruction("b.hs __rt_snc_ws_b");                                  // the synthetic NUL ends the skip on the next test
    emitter.instruction("ldrb w10, [x7]");                                      // cb = the next byte
    emitter.instruction("b __rt_snc_ws_b");                                     // keep skipping whitespace

    emitter.label("__rt_snc_run");
    emitter.instruction("sub w11, w9, #48");                                    // normalize the left byte against the digit range
    emitter.instruction("cmp w11, #9");                                         // is the left cursor on a digit?
    emitter.instruction("b.hi __rt_snc_chars");                                 // no digit run starts here
    emitter.instruction("sub w11, w10, #48");                                   // normalize the right byte against the digit range
    emitter.instruction("cmp w11, #9");                                         // is the right cursor on a digit?
    emitter.instruction("b.hi __rt_snc_chars");                                 // no digit run starts here
    emitter.instruction("cmp w9, #48");                                         // a leading zero makes the run fractional
    emitter.instruction("b.eq __rt_snc_left");                                  // compare fractional runs left-aligned
    emitter.instruction("cmp w10, #48");                                        // either side is enough to make it fractional
    emitter.instruction("b.eq __rt_snc_left");                                  // compare fractional runs left-aligned

    emitter.comment("-- compare_right: the longer digit run wins, ties fall back to the first difference --");
    emitter.instruction("mov w14, #0");                                         // bias = 0
    emitter.label("__rt_snc_cr");
    emitter.instruction("mov w1, #0");                                          // assume the left run is exhausted
    emitter.instruction("cmp x5, x6");                                          // is a left byte still available?
    emitter.instruction("b.hs __rt_snc_cr_a");                                  // the left run ended at the operand boundary
    emitter.instruction("ldrb w11, [x5]");                                      // load the left digit candidate
    emitter.instruction("sub w12, w11, #48");                                   // normalize it against the digit range
    emitter.instruction("cmp w12, #9");                                         // is it a digit?
    emitter.instruction("b.hi __rt_snc_cr_a");                                  // the left run ended at a non-digit
    emitter.instruction("mov w1, #1");                                          // the left run continues
    emitter.label("__rt_snc_cr_a");
    emitter.instruction("mov w2, #0");                                          // assume the right run is exhausted
    emitter.instruction("cmp x7, x8");                                          // is a right byte still available?
    emitter.instruction("b.hs __rt_snc_cr_b");                                  // the right run ended at the operand boundary
    emitter.instruction("ldrb w13, [x7]");                                      // load the right digit candidate
    emitter.instruction("sub w12, w13, #48");                                   // normalize it against the digit range
    emitter.instruction("cmp w12, #9");                                         // is it a digit?
    emitter.instruction("b.hi __rt_snc_cr_b");                                  // the right run ended at a non-digit
    emitter.instruction("mov w2, #1");                                          // the right run continues
    emitter.label("__rt_snc_cr_b");
    emitter.instruction("cbnz w1, __rt_snc_cr_have_a");                         // the left run still has digits
    emitter.instruction("cbnz w2, __rt_snc_neg");                               // only the right run continues, so it is larger
    emitter.instruction("b __rt_snc_after");                                    // both runs ended together: the bias decides
    emitter.label("__rt_snc_cr_have_a");
    emitter.instruction("cbz w2, __rt_snc_pos");                                // only the left run continues, so it is larger
    emitter.instruction("cmp w11, w13");                                        // compare the two digits
    emitter.instruction("b.eq __rt_snc_cr_next");                               // equal digits leave the bias alone
    emitter.instruction("cbnz w14, __rt_snc_cr_next");                          // only the first difference sets the bias
    emitter.instruction("mov w14, #1");                                         // assume the left digit is larger
    emitter.instruction("b.hi __rt_snc_cr_next");                               // the assumption held
    emitter.instruction("mov w14, #-1");                                        // the right digit was larger
    emitter.label("__rt_snc_cr_next");
    emitter.instruction("add x5, x5, #1");                                      // advance the left cursor
    emitter.instruction("add x7, x7, #1");                                      // advance the right cursor
    emitter.instruction("b __rt_snc_cr");                                       // keep measuring the two runs

    emitter.comment("-- compare_left: a fractional run is decided by its first differing digit --");
    emitter.label("__rt_snc_left");
    emitter.instruction("mov w14, #0");                                         // an exhausted pair compares equal
    emitter.label("__rt_snc_cl");
    emitter.instruction("mov w1, #0");                                          // assume the left run is exhausted
    emitter.instruction("cmp x5, x6");                                          // is a left byte still available?
    emitter.instruction("b.hs __rt_snc_cl_a");                                  // the left run ended at the operand boundary
    emitter.instruction("ldrb w11, [x5]");                                      // load the left digit candidate
    emitter.instruction("sub w12, w11, #48");                                   // normalize it against the digit range
    emitter.instruction("cmp w12, #9");                                         // is it a digit?
    emitter.instruction("b.hi __rt_snc_cl_a");                                  // the left run ended at a non-digit
    emitter.instruction("mov w1, #1");                                          // the left run continues
    emitter.label("__rt_snc_cl_a");
    emitter.instruction("mov w2, #0");                                          // assume the right run is exhausted
    emitter.instruction("cmp x7, x8");                                          // is a right byte still available?
    emitter.instruction("b.hs __rt_snc_cl_b");                                  // the right run ended at the operand boundary
    emitter.instruction("ldrb w13, [x7]");                                      // load the right digit candidate
    emitter.instruction("sub w12, w13, #48");                                   // normalize it against the digit range
    emitter.instruction("cmp w12, #9");                                         // is it a digit?
    emitter.instruction("b.hi __rt_snc_cl_b");                                  // the right run ended at a non-digit
    emitter.instruction("mov w2, #1");                                          // the right run continues
    emitter.label("__rt_snc_cl_b");
    emitter.instruction("cbnz w1, __rt_snc_cl_have_a");                         // the left run still has digits
    emitter.instruction("cbnz w2, __rt_snc_neg");                               // only the right run continues, so it is larger
    emitter.instruction("b __rt_snc_after");                                    // both runs ended together and compared equal
    emitter.label("__rt_snc_cl_have_a");
    emitter.instruction("cbz w2, __rt_snc_pos");                                // only the left run continues, so it is larger
    emitter.instruction("cmp w11, w13");                                        // compare the two digits
    emitter.instruction("b.lo __rt_snc_neg");                                   // the first difference decides immediately
    emitter.instruction("b.hi __rt_snc_pos");                                   // the first difference decides immediately
    emitter.instruction("add x5, x5, #1");                                      // advance the left cursor
    emitter.instruction("add x7, x7, #1");                                      // advance the right cursor
    emitter.instruction("b __rt_snc_cl");                                       // keep comparing the two runs

    emitter.label("__rt_snc_after");
    emitter.instruction("cbnz w14, __rt_snc_ret_bias");                         // a decided run ends the comparison
    emitter.instruction("cmp x5, x6");                                          // did the left operand end with its run?
    emitter.instruction("b.ne __rt_snc_after_a");                               // the left operand has more bytes
    emitter.instruction("cmp x7, x8");                                          // did the right operand end too?
    emitter.instruction("b.eq __rt_snc_zero");                                  // both ended together and every run tied
    emitter.instruction("b __rt_snc_neg");                                      // the shorter operand sorts first
    emitter.label("__rt_snc_after_a");
    emitter.instruction("cmp x7, x8");                                          // did the right operand end?
    emitter.instruction("b.eq __rt_snc_pos");                                   // the shorter operand sorts first
    emitter.instruction("ldrb w9, [x5]");                                       // reload the byte the run stopped on
    emitter.instruction("ldrb w10, [x7]");                                      // reload the byte the run stopped on

    emitter.label("__rt_snc_chars");
    emitter.instruction("cbz x0, __rt_snc_cmp");                                // SORT_NATURAL compares the raw bytes
    emitter.instruction("sub w11, w9, #97");                                    // normalize the left byte against a..z
    emitter.instruction("cmp w11, #25");                                        // is it a lowercase ASCII letter?
    emitter.instruction("b.hi __rt_snc_fold_b");                                // nothing to fold on the left
    emitter.instruction("sub w9, w9, #32");                                     // fold it to uppercase
    emitter.label("__rt_snc_fold_b");
    emitter.instruction("sub w11, w10, #97");                                   // normalize the right byte against a..z
    emitter.instruction("cmp w11, #25");                                        // is it a lowercase ASCII letter?
    emitter.instruction("b.hi __rt_snc_cmp");                                   // nothing to fold on the right
    emitter.instruction("sub w10, w10, #32");                                   // fold it to uppercase
    emitter.label("__rt_snc_cmp");
    emitter.instruction("cmp w9, w10");                                         // compare the two bytes as unsigned chars
    emitter.instruction("b.lo __rt_snc_neg");                                   // the left byte sorts first
    emitter.instruction("b.hi __rt_snc_pos");                                   // the right byte sorts first
    emitter.instruction("add x5, x5, #1");                                      // the bytes matched, so advance both cursors
    emitter.instruction("add x7, x7, #1");                                      // the bytes matched, so advance both cursors
    emitter.instruction("cmp x5, x6");                                          // does the left operand have more bytes?
    emitter.instruction("b.lo __rt_snc_more_a");                                // keep going on the left
    emitter.instruction("cmp x7, x8");                                          // does the right operand have more bytes?
    emitter.instruction("b.hs __rt_snc_zero");                                  // both operands ended together
    emitter.instruction("b __rt_snc_neg");                                      // the shorter operand sorts first
    emitter.label("__rt_snc_more_a");
    emitter.instruction("cmp x7, x8");                                          // does the right operand have more bytes?
    emitter.instruction("b.hs __rt_snc_pos");                                   // the shorter operand sorts first
    emitter.instruction("ldrb w9, [x5]");                                       // ca = the next byte
    emitter.instruction("ldrb w10, [x7]");                                      // cb = the next byte
    emitter.instruction("b __rt_snc_ws_a");                                     // start the next round of the main loop

    emitter.label("__rt_snc_ret_bias");
    emitter.instruction("sxtw x0, w14");                                        // publish the digit-run bias
    emitter.instruction("ret");                                                 // return the signed comparison result
    emitter.label("__rt_snc_neg");
    emitter.instruction("mov x0, #-1");                                         // the left operand sorts first
    emitter.instruction("ret");                                                 // return the signed comparison result
    emitter.label("__rt_snc_pos");
    emitter.instruction("mov x0, #1");                                          // the right operand sorts first
    emitter.instruction("ret");                                                 // return the signed comparison result
    emitter.label("__rt_snc_zero");
    emitter.instruction("mov x0, #0");                                          // the operands compare equal
    emitter.instruction("ret");                                                 // return the signed comparison result
    emitter.label("__rt_snc_empty");
    emitter.instruction("cmp x2, x4");                                          // an empty operand is decided by length alone
    emitter.instruction("b.eq __rt_snc_zero");                                  // two empty operands compare equal
    emitter.instruction("b.hi __rt_snc_pos");                                   // the longer operand sorts last
    emitter.instruction("b __rt_snc_neg");                                      // the empty operand sorts first
}

/// Emits the x86_64 implementation of `__rt_strnatcmp`.
///
/// `rbx` carries the digit-run bias because the comparison loops already use every
/// caller-saved register the System V ABI offers; it is restored through the shared epilogue
/// that every exit branches to.
fn emit_strnatcmp_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strnatcmp (PHP natural order) ---");
    emitter.label_global("__rt_strnatcmp");
    emitter.instruction("push rbx");                                            // the digit-run bias needs a register that survives the loops
    emitter.instruction("test rdx, rdx");                                       // an empty operand is decided by length alone
    emitter.instruction("jz __rt_snc_empty");                                   // take the length-only answer
    emitter.instruction("test r8, r8");                                         // the same holds for an empty right operand
    emitter.instruction("jz __rt_snc_empty");                                   // take the length-only answer
    emitter.instruction("lea r10, [rsi + rdx]");                                // aend = a + a_len
    emitter.instruction("lea r11, [rcx + r8]");                                 // bend = b + b_len
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // ca = *ap
    emitter.instruction("movzx r8d, BYTE PTR [rcx]");                           // cb = *bp

    emitter.comment("-- leading zeros are skipped once, before the main loop --");
    emitter.label("__rt_snc_lz_a");
    emitter.instruction("cmp edx, 48");                                         // only a literal zero starts a skippable run
    emitter.instruction("jne __rt_snc_lz_b");                                   // the left operand is already positioned
    emitter.instruction("lea rax, [rsi + 1]");                                  // look at the byte after the zero
    emitter.instruction("cmp rax, r10");                                        // PHP requires a byte to remain inside the operand
    emitter.instruction("jae __rt_snc_lz_b");                                   // a trailing zero is kept as a digit
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // load the following byte
    emitter.instruction("cmp r9d, 48");                                         // is another digit following?
    emitter.instruction("jb __rt_snc_lz_b");                                    // a zero before a non-digit stays significant
    emitter.instruction("cmp r9d, 57");                                         // is another digit following?
    emitter.instruction("ja __rt_snc_lz_b");                                    // a zero before a non-digit stays significant
    emitter.instruction("mov rsi, rax");                                        // consume the leading zero
    emitter.instruction("mov edx, r9d");                                        // ca = the digit that followed it
    emitter.instruction("jmp __rt_snc_lz_a");                                   // keep skipping leading zeros
    emitter.label("__rt_snc_lz_b");
    emitter.instruction("cmp r8d, 48");                                         // mirror the skip on the right operand
    emitter.instruction("jne __rt_snc_ws_a");                                   // enter the main loop
    emitter.instruction("lea rax, [rcx + 1]");                                  // look at the byte after the zero
    emitter.instruction("cmp rax, r11");                                        // a byte must remain inside the operand
    emitter.instruction("jae __rt_snc_ws_a");                                   // a trailing zero is kept as a digit
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // load the following byte
    emitter.instruction("cmp r9d, 48");                                         // is another digit following?
    emitter.instruction("jb __rt_snc_ws_a");                                    // a zero before a non-digit stays significant
    emitter.instruction("cmp r9d, 57");                                         // is another digit following?
    emitter.instruction("ja __rt_snc_ws_a");                                    // a zero before a non-digit stays significant
    emitter.instruction("mov rcx, rax");                                        // consume the leading zero
    emitter.instruction("mov r8d, r9d");                                        // cb = the digit that followed it
    emitter.instruction("jmp __rt_snc_lz_b");                                   // keep skipping leading zeros

    emitter.comment("-- main loop: whitespace, then a digit run, then one plain byte --");
    emitter.label("__rt_snc_ws_a");
    emitter.instruction("cmp edx, 32");                                         // ASCII space is whitespace
    emitter.instruction("je __rt_snc_ws_a_step");                               // skip it
    emitter.instruction("cmp edx, 9");                                          // the tab/newline/vtab/formfeed/return range starts at 9
    emitter.instruction("jb __rt_snc_ws_b");                                    // the left cursor rests on a significant byte
    emitter.instruction("cmp edx, 13");                                         // and ends at 13
    emitter.instruction("ja __rt_snc_ws_b");                                    // the left cursor rests on a significant byte
    emitter.label("__rt_snc_ws_a_step");
    emitter.instruction("inc rsi");                                             // consume one whitespace byte
    emitter.instruction("xor edx, edx");                                        // past the operand PHP reads its NUL terminator
    emitter.instruction("cmp rsi, r10");                                        // did the operand end?
    emitter.instruction("jae __rt_snc_ws_a");                                   // the synthetic NUL ends the skip on the next test
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // ca = the next byte
    emitter.instruction("jmp __rt_snc_ws_a");                                   // keep skipping whitespace
    emitter.label("__rt_snc_ws_b");
    emitter.instruction("cmp r8d, 32");                                         // mirror the skip on the right operand
    emitter.instruction("je __rt_snc_ws_b_step");                               // skip it
    emitter.instruction("cmp r8d, 9");                                          // the tab/newline/vtab/formfeed/return range starts at 9
    emitter.instruction("jb __rt_snc_run");                                     // both cursors rest on significant bytes
    emitter.instruction("cmp r8d, 13");                                         // and ends at 13
    emitter.instruction("ja __rt_snc_run");                                     // both cursors rest on significant bytes
    emitter.label("__rt_snc_ws_b_step");
    emitter.instruction("inc rcx");                                             // consume one whitespace byte
    emitter.instruction("xor r8d, r8d");                                        // past the operand PHP reads its NUL terminator
    emitter.instruction("cmp rcx, r11");                                        // did the operand end?
    emitter.instruction("jae __rt_snc_ws_b");                                   // the synthetic NUL ends the skip on the next test
    emitter.instruction("movzx r8d, BYTE PTR [rcx]");                           // cb = the next byte
    emitter.instruction("jmp __rt_snc_ws_b");                                   // keep skipping whitespace

    emitter.label("__rt_snc_run");
    emitter.instruction("cmp edx, 48");                                         // is the left cursor on a digit?
    emitter.instruction("jb __rt_snc_chars");                                   // no digit run starts here
    emitter.instruction("cmp edx, 57");                                         // is the left cursor on a digit?
    emitter.instruction("ja __rt_snc_chars");                                   // no digit run starts here
    emitter.instruction("cmp r8d, 48");                                         // is the right cursor on a digit?
    emitter.instruction("jb __rt_snc_chars");                                   // no digit run starts here
    emitter.instruction("cmp r8d, 57");                                         // is the right cursor on a digit?
    emitter.instruction("ja __rt_snc_chars");                                   // no digit run starts here
    emitter.instruction("cmp edx, 48");                                         // a leading zero makes the run fractional
    emitter.instruction("je __rt_snc_left");                                    // compare fractional runs left-aligned
    emitter.instruction("cmp r8d, 48");                                         // either side is enough to make it fractional
    emitter.instruction("je __rt_snc_left");                                    // compare fractional runs left-aligned

    emitter.comment("-- compare_right: the longer digit run wins, ties fall back to the first difference --");
    emitter.instruction("xor ebx, ebx");                                        // bias = 0
    emitter.label("__rt_snc_cr");
    emitter.instruction("cmp rsi, r10");                                        // is a left byte still available?
    emitter.instruction("jae __rt_snc_cr_no_a");                                // the left run ended at the operand boundary
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the left digit candidate
    emitter.instruction("cmp eax, 48");                                         // is it a digit?
    emitter.instruction("jb __rt_snc_cr_no_a");                                 // the left run ended at a non-digit
    emitter.instruction("cmp eax, 57");                                         // is it a digit?
    emitter.instruction("ja __rt_snc_cr_no_a");                                 // the left run ended at a non-digit
    emitter.instruction("cmp rcx, r11");                                        // is a right byte still available?
    emitter.instruction("jae __rt_snc_pos");                                    // only the left run continues, so it is larger
    emitter.instruction("movzx r9d, BYTE PTR [rcx]");                           // load the right digit candidate
    emitter.instruction("cmp r9d, 48");                                         // is it a digit?
    emitter.instruction("jb __rt_snc_pos");                                     // only the left run continues, so it is larger
    emitter.instruction("cmp r9d, 57");                                         // is it a digit?
    emitter.instruction("ja __rt_snc_pos");                                     // only the left run continues, so it is larger
    emitter.instruction("cmp eax, r9d");                                        // compare the two digits
    emitter.instruction("je __rt_snc_cr_next");                                 // equal digits leave the bias alone
    emitter.instruction("mov r9, -1");                                          // assume the right digit is larger
    emitter.instruction("jb __rt_snc_cr_set");                                  // the assumption held
    emitter.instruction("mov r9, 1");                                           // the left digit was larger
    emitter.label("__rt_snc_cr_set");
    emitter.instruction("test rbx, rbx");                                       // only the first difference sets the bias
    emitter.instruction("cmovz rbx, r9");                                       // adopt the sign of that first difference
    emitter.label("__rt_snc_cr_next");
    emitter.instruction("inc rsi");                                             // advance the left cursor
    emitter.instruction("inc rcx");                                             // advance the right cursor
    emitter.instruction("jmp __rt_snc_cr");                                     // keep measuring the two runs
    emitter.label("__rt_snc_cr_no_a");
    emitter.instruction("cmp rcx, r11");                                        // is a right byte still available?
    emitter.instruction("jae __rt_snc_after");                                  // both runs ended together: the bias decides
    emitter.instruction("movzx r9d, BYTE PTR [rcx]");                           // load the right digit candidate
    emitter.instruction("cmp r9d, 48");                                         // is it a digit?
    emitter.instruction("jb __rt_snc_after");                                   // both runs ended together: the bias decides
    emitter.instruction("cmp r9d, 57");                                         // is it a digit?
    emitter.instruction("ja __rt_snc_after");                                   // both runs ended together: the bias decides
    emitter.instruction("jmp __rt_snc_neg");                                    // only the right run continues, so it is larger

    emitter.comment("-- compare_left: a fractional run is decided by its first differing digit --");
    emitter.label("__rt_snc_left");
    emitter.instruction("xor ebx, ebx");                                        // an exhausted pair compares equal
    emitter.label("__rt_snc_cl");
    emitter.instruction("cmp rsi, r10");                                        // is a left byte still available?
    emitter.instruction("jae __rt_snc_cl_no_a");                                // the left run ended at the operand boundary
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the left digit candidate
    emitter.instruction("cmp eax, 48");                                         // is it a digit?
    emitter.instruction("jb __rt_snc_cl_no_a");                                 // the left run ended at a non-digit
    emitter.instruction("cmp eax, 57");                                         // is it a digit?
    emitter.instruction("ja __rt_snc_cl_no_a");                                 // the left run ended at a non-digit
    emitter.instruction("cmp rcx, r11");                                        // is a right byte still available?
    emitter.instruction("jae __rt_snc_pos");                                    // only the left run continues, so it is larger
    emitter.instruction("movzx r9d, BYTE PTR [rcx]");                           // load the right digit candidate
    emitter.instruction("cmp r9d, 48");                                         // is it a digit?
    emitter.instruction("jb __rt_snc_pos");                                     // only the left run continues, so it is larger
    emitter.instruction("cmp r9d, 57");                                         // is it a digit?
    emitter.instruction("ja __rt_snc_pos");                                     // only the left run continues, so it is larger
    emitter.instruction("cmp eax, r9d");                                        // compare the two digits
    emitter.instruction("jb __rt_snc_neg");                                     // the first difference decides immediately
    emitter.instruction("ja __rt_snc_pos");                                     // the first difference decides immediately
    emitter.instruction("inc rsi");                                             // advance the left cursor
    emitter.instruction("inc rcx");                                             // advance the right cursor
    emitter.instruction("jmp __rt_snc_cl");                                     // keep comparing the two runs
    emitter.label("__rt_snc_cl_no_a");
    emitter.instruction("cmp rcx, r11");                                        // is a right byte still available?
    emitter.instruction("jae __rt_snc_after");                                  // both runs ended together and compared equal
    emitter.instruction("movzx r9d, BYTE PTR [rcx]");                           // load the right digit candidate
    emitter.instruction("cmp r9d, 48");                                         // is it a digit?
    emitter.instruction("jb __rt_snc_after");                                   // both runs ended together and compared equal
    emitter.instruction("cmp r9d, 57");                                         // is it a digit?
    emitter.instruction("ja __rt_snc_after");                                   // both runs ended together and compared equal
    emitter.instruction("jmp __rt_snc_neg");                                    // only the right run continues, so it is larger

    emitter.label("__rt_snc_after");
    emitter.instruction("test rbx, rbx");                                       // a decided run ends the comparison
    emitter.instruction("jnz __rt_snc_ret_bias");                               // publish the digit-run bias
    emitter.instruction("cmp rsi, r10");                                        // did the left operand end with its run?
    emitter.instruction("jne __rt_snc_after_a");                                // the left operand has more bytes
    emitter.instruction("cmp rcx, r11");                                        // did the right operand end too?
    emitter.instruction("je __rt_snc_zero");                                    // both ended together and every run tied
    emitter.instruction("jmp __rt_snc_neg");                                    // the shorter operand sorts first
    emitter.label("__rt_snc_after_a");
    emitter.instruction("cmp rcx, r11");                                        // did the right operand end?
    emitter.instruction("je __rt_snc_pos");                                     // the shorter operand sorts first
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // reload the byte the run stopped on
    emitter.instruction("movzx r8d, BYTE PTR [rcx]");                           // reload the byte the run stopped on

    emitter.label("__rt_snc_chars");
    emitter.instruction("test rdi, rdi");                                       // SORT_NATURAL compares the raw bytes
    emitter.instruction("jz __rt_snc_cmp");                                     // no case folding was requested
    emitter.instruction("cmp edx, 97");                                         // is the left byte a lowercase ASCII letter?
    emitter.instruction("jb __rt_snc_fold_b");                                  // nothing to fold on the left
    emitter.instruction("cmp edx, 122");                                        // is the left byte a lowercase ASCII letter?
    emitter.instruction("ja __rt_snc_fold_b");                                  // nothing to fold on the left
    emitter.instruction("sub edx, 32");                                         // fold it to uppercase
    emitter.label("__rt_snc_fold_b");
    emitter.instruction("cmp r8d, 97");                                         // is the right byte a lowercase ASCII letter?
    emitter.instruction("jb __rt_snc_cmp");                                     // nothing to fold on the right
    emitter.instruction("cmp r8d, 122");                                        // is the right byte a lowercase ASCII letter?
    emitter.instruction("ja __rt_snc_cmp");                                     // nothing to fold on the right
    emitter.instruction("sub r8d, 32");                                         // fold it to uppercase
    emitter.label("__rt_snc_cmp");
    emitter.instruction("cmp edx, r8d");                                        // compare the two bytes as unsigned chars
    emitter.instruction("jb __rt_snc_neg");                                     // the left byte sorts first
    emitter.instruction("ja __rt_snc_pos");                                     // the right byte sorts first
    emitter.instruction("inc rsi");                                             // the bytes matched, so advance both cursors
    emitter.instruction("inc rcx");                                             // the bytes matched, so advance both cursors
    emitter.instruction("cmp rsi, r10");                                        // does the left operand have more bytes?
    emitter.instruction("jb __rt_snc_more_a");                                  // keep going on the left
    emitter.instruction("cmp rcx, r11");                                        // does the right operand have more bytes?
    emitter.instruction("jae __rt_snc_zero");                                   // both operands ended together
    emitter.instruction("jmp __rt_snc_neg");                                    // the shorter operand sorts first
    emitter.label("__rt_snc_more_a");
    emitter.instruction("cmp rcx, r11");                                        // does the right operand have more bytes?
    emitter.instruction("jae __rt_snc_pos");                                    // the shorter operand sorts first
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // ca = the next byte
    emitter.instruction("movzx r8d, BYTE PTR [rcx]");                           // cb = the next byte
    emitter.instruction("jmp __rt_snc_ws_a");                                   // start the next round of the main loop

    emitter.label("__rt_snc_ret_bias");
    emitter.instruction("mov rax, rbx");                                        // publish the digit-run bias
    emitter.instruction("jmp __rt_snc_ret");                                    // leave through the shared epilogue
    emitter.label("__rt_snc_neg");
    emitter.instruction("mov rax, -1");                                         // the left operand sorts first
    emitter.instruction("jmp __rt_snc_ret");                                    // leave through the shared epilogue
    emitter.label("__rt_snc_pos");
    emitter.instruction("mov rax, 1");                                          // the right operand sorts first
    emitter.instruction("jmp __rt_snc_ret");                                    // leave through the shared epilogue
    emitter.label("__rt_snc_zero");
    emitter.instruction("xor eax, eax");                                        // the operands compare equal
    emitter.instruction("jmp __rt_snc_ret");                                    // leave through the shared epilogue
    emitter.label("__rt_snc_empty");
    emitter.instruction("cmp rdx, r8");                                         // an empty operand is decided by length alone
    emitter.instruction("je __rt_snc_zero");                                    // two empty operands compare equal
    emitter.instruction("ja __rt_snc_pos");                                     // the longer operand sorts last
    emitter.instruction("jmp __rt_snc_neg");                                    // the empty operand sorts first
    emitter.label("__rt_snc_ret");
    emitter.instruction("pop rbx");                                             // restore the caller's register
    emitter.instruction("ret");                                                 // return the signed comparison result in rax
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits the comparator for one target and returns the assembly.
    fn assembly_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_strnatcmp(&mut emitter);
        emitter.output()
    }

    /// Every target gets the helper, and it stays a leaf.
    ///
    /// The comparator runs once per comparison inside an `O(n log n)` sort, so a call here --
    /// to libc or to another runtime helper -- would show up in every natural-order key sort.
    #[test]
    fn natural_comparison_is_a_leaf_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = assembly_for(target);
            assert!(
                asm.contains(".globl __rt_strnatcmp\n"),
                "{target:?} must publish the comparator: {asm}"
            );
            for call in ["    bl ", "    call "] {
                assert!(
                    !asm.contains(call),
                    "{target:?} must not call out of the comparator: {asm}"
                );
            }
        }
    }

    /// Case folding is ASCII-only, on purpose.
    ///
    /// php-src folds with libc `toupper()`, which maps Latin-1 under Darwin's C locale and not
    /// under glibc's. Bounding the fold to `a`..`z` (97..122) is what keeps a compiled program
    /// ordering its keys the same way on macOS and Linux.
    #[test]
    fn case_folding_is_bounded_to_ascii_letters() {
        let arm = assembly_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("sub w11, w9, #97"));
        assert!(arm.contains("cmp w11, #25"));
        let x86 = assembly_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x86.contains("cmp edx, 97"));
        assert!(x86.contains("cmp edx, 122"));
    }

    /// The whitespace skip synthesizes the terminator PHP would have read.
    ///
    /// php-src walks one byte past the operand there and relies on the NUL its strings always
    /// carry. Elephc strings are pointer/length pairs with no terminator, so the skip has to
    /// produce that `0` itself rather than read whatever follows in memory.
    #[test]
    fn whitespace_skip_stops_at_the_operand_boundary() {
        let arm = assembly_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("mov w9, #0"));
        assert!(arm.contains("cmp x5, x6"));
        let x86 = assembly_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x86.contains("xor edx, edx"));
        assert!(x86.contains("cmp rsi, r10"));
    }
}
