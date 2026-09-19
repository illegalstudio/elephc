//! Purpose:
//! Emits `__rt_key_compare_flagged`, the associative-array key comparator PHP selects with
//! `ksort()`/`krsort()`'s `$flags` argument, plus the two small helpers it needs.
//!
//! Called from:
//! - `crate::codegen_support::runtime::arrays::hash_sort` through `emit_hash_sort`.
//!
//! Key details:
//! - `SORT_REGULAR` is NOT handled here: it keeps `__rt_key_compare_regular` unchanged, so a
//!   flagless sort runs exactly the code it ran before `$flags` existed.
//! - Every byte-comparing mode spells an integer key out as decimal digits first, which is what
//!   php-src does with `zend_print_long_to_buf`. That is why `ksort([10 => .., 9 => ..],
//!   SORT_STRING)` puts `10` before `9`.
//! - `SORT_LOCALE_STRING` clips both operands at the first NUL and then compares bytes. That is
//!   `strcoll` in the C locale, and the C locale is the only one an elephc program can be in:
//!   there is no PHP-visible `setlocale()`. Calling libc would mean two heap allocations per
//!   comparison to manufacture the NUL-terminated operands `strcoll` wants.
//! - Only the SIGN of the result is contractual; the merge sort reads it with a signed branch.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the flag-selected key comparator and its helpers for the active target.
///
/// Input: AArch64 `x0`/`x1` = left key payload and length-or-sentinel, `x2`/`x3` = the same for
/// the right key, `x4` = the resolved comparator selector; x86_64 `rdi`/`rsi`, `rdx`/`rcx` and
/// `r8` in the same roles.
///
/// Output: AArch64 `x0` / x86_64 `rax` = a negative value, zero, or a positive value.
pub(super) fn emit_key_compare_flags(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: key_compare_flagged (PHP sort flags) ---");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 comparator, decimal formatter, and NUL clipper.
///
/// The 160-byte frame holds the two key words per side, the selector, the resolved byte
/// pointer/length per side, a 24-byte digit buffer per side (long enough for
/// `-9223372036854775808`), and the parked left double the numeric path needs across its
/// second parse.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_key_compare_flagged");
    emitter.instruction("sub sp, sp, #160");                                    // reserve the operand words, both digit buffers, and the frame
    emitter.instruction("stp x29, x30, [sp, #144]");                            // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #144");                                   // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the left key payload
    emitter.instruction("str x1, [sp, #8]");                                    // save the left key length or integer sentinel
    emitter.instruction("str x2, [sp, #16]");                                   // save the right key payload
    emitter.instruction("str x3, [sp, #24]");                                   // save the right key length or integer sentinel
    emitter.instruction("str x4, [sp, #32]");                                   // save the resolved comparator selector
    emitter.instruction(&format!("cmp x4, #{}", super::hash_sort::KEY_COMPARATOR_NUMERIC));                                          // SORT_NUMERIC never looks at the keys as bytes
    emitter.instruction("b.eq __rt_kcf_numeric");                               // take the numeric path

    emitter.comment("-- every remaining mode compares bytes, so an integer key is spelled out first --");
    emitter.instruction("cmn x1, #1");                                          // is the left key an integer?
    emitter.instruction("b.ne __rt_kcf_left_str");                              // a string key already has bytes
    emitter.instruction("add x1, sp, #72");                                     // point at the left digit buffer
    emitter.instruction("bl __rt_key_sort_int_bytes");                          // render the integer key as decimal digits
    emitter.label("__rt_kcf_left_str");
    emitter.instruction("str x0, [sp, #40]");                                   // save the left byte pointer
    emitter.instruction("str x1, [sp, #48]");                                   // save the left byte length
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the right key payload
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the right key length or integer sentinel
    emitter.instruction("cmn x1, #1");                                          // is the right key an integer?
    emitter.instruction("b.ne __rt_kcf_right_str");                             // a string key already has bytes
    emitter.instruction("add x1, sp, #96");                                     // point at the right digit buffer
    emitter.instruction("bl __rt_key_sort_int_bytes");                          // render the integer key as decimal digits
    emitter.label("__rt_kcf_right_str");
    emitter.instruction("str x0, [sp, #56]");                                   // save the right byte pointer
    emitter.instruction("str x1, [sp, #64]");                                   // save the right byte length

    emitter.label("__rt_kcf_dispatch");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the resolved comparator selector
    emitter.instruction(&format!("cmp x9, #{}", super::hash_sort::KEY_COMPARATOR_LOCALE));                                          // SORT_LOCALE_STRING clips both operands first
    emitter.instruction("b.eq __rt_kcf_locale");                                // take the locale path
    emitter.instruction(&format!("cmp x9, #{}", super::hash_sort::KEY_COMPARATOR_NATURAL));                                          // SORT_NATURAL and its case-folded twin
    emitter.instruction("b.ge __rt_kcf_natural");                               // take the natural-order path
    emitter.instruction("ldr x1, [sp, #40]");                                   // pass the left byte pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // pass the left byte length
    emitter.instruction("ldr x3, [sp, #56]");                                   // pass the right byte pointer
    emitter.instruction("ldr x4, [sp, #64]");                                   // pass the right byte length
    emitter.instruction(&format!("cmp x9, #{}", super::hash_sort::KEY_COMPARATOR_STRING_CI));                                          // SORT_STRING | SORT_FLAG_CASE
    emitter.instruction("b.eq __rt_kcf_string_ci");                             // fold ASCII case while comparing
    emitter.instruction("bl __rt_strcmp");                                      // zend_binary_strcmp: shared prefix, then length
    emitter.instruction("b __rt_kcf_done");                                     // publish the signed result
    emitter.label("__rt_kcf_string_ci");
    emitter.instruction("bl __rt_strcasecmp");                                  // zend_binary_strcasecmp over the same operands
    emitter.instruction("b __rt_kcf_done");                                     // publish the signed result

    emitter.label("__rt_kcf_locale");
    emitter.instruction("ldr x0, [sp, #40]");                                   // pass the left byte pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // pass the left byte length
    emitter.instruction("bl __rt_key_sort_clip_nul");                           // strcoll reads a C string, so stop at the first NUL
    emitter.instruction("str x0, [sp, #48]");                                   // save the clipped left length
    emitter.instruction("ldr x0, [sp, #56]");                                   // pass the right byte pointer
    emitter.instruction("ldr x1, [sp, #64]");                                   // pass the right byte length
    emitter.instruction("bl __rt_key_sort_clip_nul");                           // strcoll reads a C string, so stop at the first NUL
    emitter.instruction("str x0, [sp, #64]");                                   // save the clipped right length
    emitter.instruction("ldr x1, [sp, #40]");                                   // pass the left byte pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // pass the clipped left length
    emitter.instruction("ldr x3, [sp, #56]");                                   // pass the right byte pointer
    emitter.instruction("ldr x4, [sp, #64]");                                   // pass the clipped right length
    emitter.instruction("bl __rt_strcmp");                                      // the C locale collates by unsigned byte order
    emitter.instruction("b __rt_kcf_done");                                     // publish the signed result

    emitter.label("__rt_kcf_natural");
    emitter.instruction(&format!("cmp x9, #{}", super::hash_sort::KEY_COMPARATOR_NATURAL_CI));                                          // SORT_NATURAL | SORT_FLAG_CASE
    emitter.instruction("cset x0, eq");                                         // 1 when case folding was requested
    emitter.instruction("ldr x1, [sp, #40]");                                   // pass the left byte pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // pass the left byte length
    emitter.instruction("ldr x3, [sp, #56]");                                   // pass the right byte pointer
    emitter.instruction("ldr x4, [sp, #64]");                                   // pass the right byte length
    emitter.instruction("bl __rt_strnatcmp");                                   // PHP natural order over the two key spellings
    emitter.instruction("b __rt_kcf_done");                                     // publish the signed result

    emitter.comment("-- SORT_NUMERIC: PHP reads each key as a double, integer keys without rounding --");
    emitter.label("__rt_kcf_numeric");
    emitter.instruction("cmn x1, #1");                                          // is the left key an integer?
    emitter.instruction("b.ne __rt_kcf_num_left_str");                          // a string key goes through PHP's numeric grammar
    emitter.instruction("scvtf d0, x0");                                        // widen the integer key to a double
    emitter.instruction("b __rt_kcf_num_left_done");                            // the left operand is ready
    emitter.label("__rt_kcf_num_left_str");
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the left key bytes
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the left key length
    emitter.instruction("bl __rt_str_to_number");                               // parse the leading numeric run into d0
    emitter.label("__rt_kcf_num_left_done");
    emitter.instruction("str d0, [sp, #120]");                                  // park the left double across the right parse
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the right key payload
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the right key length or integer sentinel
    emitter.instruction("cmn x1, #1");                                          // is the right key an integer?
    emitter.instruction("b.ne __rt_kcf_num_right_str");                         // a string key goes through PHP's numeric grammar
    emitter.instruction("scvtf d1, x0");                                        // widen the integer key to a double
    emitter.instruction("b __rt_kcf_num_right_done");                           // the right operand is ready
    emitter.label("__rt_kcf_num_right_str");
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the right key bytes
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the right key length
    emitter.instruction("bl __rt_str_to_number");                               // parse the leading numeric run into d0
    emitter.instruction("fmov d1, d0");                                         // move it clear of the parked left double
    emitter.label("__rt_kcf_num_right_done");
    emitter.instruction("ldr d0, [sp, #120]");                                  // restore the left double
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the left key length or integer sentinel
    emitter.instruction("cmn x9, #1");                                          // was the left key an integer?
    emitter.instruction("b.ne __rt_kcf_num_double");                            // a string operand forces the double comparison
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the right key length or integer sentinel
    emitter.instruction("cmn x9, #1");                                          // was the right key an integer too?
    emitter.instruction("b.ne __rt_kcf_num_double");                            // a string operand forces the double comparison
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the left integer key
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the right integer key
    emitter.instruction("cmp x9, x10");                                         // two integer keys compare exactly, never through a double
    emitter.instruction("mov x0, #-1");                                         // PHP answers -1 for every non-greater pair here
    emitter.instruction("mov x11, #1");                                         // and 1 otherwise: two hash keys are never equal
    emitter.instruction("csel x0, x11, x0, gt");                                // select the greater-than answer
    emitter.instruction("b __rt_kcf_done");                                     // publish the signed result
    emitter.label("__rt_kcf_num_double");
    emitter.instruction("fcmp d0, d1");                                         // compare the two parsed doubles
    emitter.instruction("mov x0, #1");                                          // an unordered or greater pair answers 1
    emitter.instruction("mov x9, #-1");                                         // prepare the less-than answer
    emitter.instruction("csel x0, x9, x0, mi");                                 // select it when the left double is smaller
    emitter.instruction("mov x9, #0");                                          // prepare the equal answer
    emitter.instruction("csel x0, x9, x0, eq");                                 // select it when the doubles match

    emitter.label("__rt_kcf_done");
    emitter.instruction("ldp x29, x30, [sp, #144]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // release the comparison frame
    emitter.instruction("ret");                                                 // return the signed comparison result in x0

    emitter.blank();
    emitter.comment("--- runtime: key_sort_int_bytes ---");
    emitter.label_global("__rt_key_sort_int_bytes");
    emitter.instruction("add x9, x1, #24");                                     // one past the end of the caller's buffer
    emitter.instruction("mov x10, x9");                                         // digits are written right to left from there
    emitter.instruction("cmp x0, #0");                                          // classify the value once
    emitter.instruction("cneg x12, x0, lt");                                    // magnitude, read as unsigned so i64::MIN survives
    emitter.instruction("b.ne __rt_ksib_digits");                               // a non-zero value has digits to emit
    emitter.instruction("mov w11, #48");                                        // zero has exactly one digit
    emitter.instruction("sub x10, x10, #1");                                    // step back one byte
    emitter.instruction("strb w11, [x10]");                                     // store it
    emitter.instruction("b __rt_ksib_done");                                    // the buffer is complete
    emitter.label("__rt_ksib_digits");
    emitter.instruction("mov x13, #10");                                        // the decimal radix
    emitter.label("__rt_ksib_loop");
    emitter.instruction("udiv x14, x12, x13");                                  // quotient, unsigned so the magnitude stays exact
    emitter.instruction("msub x15, x14, x13, x12");                             // remainder = magnitude - quotient * 10
    emitter.instruction("add w15, w15, #48");                                   // make it an ASCII digit
    emitter.instruction("sub x10, x10, #1");                                    // step back one byte
    emitter.instruction("strb w15, [x10]");                                     // store the digit
    emitter.instruction("mov x12, x14");                                        // keep dividing the quotient
    emitter.instruction("cbnz x12, __rt_ksib_loop");                            // until nothing is left
    emitter.instruction("cmp x0, #0");                                          // was the value negative?
    emitter.instruction("b.ge __rt_ksib_done");                                 // no sign byte is needed
    emitter.instruction("mov w11, #45");                                        // ASCII minus
    emitter.instruction("sub x10, x10, #1");                                    // step back one byte
    emitter.instruction("strb w11, [x10]");                                     // store the sign
    emitter.label("__rt_ksib_done");
    emitter.instruction("mov x0, x10");                                         // publish the first byte written
    emitter.instruction("sub x1, x9, x10");                                     // publish how many bytes that is
    emitter.instruction("ret");                                                 // return the borrowed pointer/length pair

    emitter.blank();
    emitter.comment("--- runtime: key_sort_clip_nul ---");
    emitter.label_global("__rt_key_sort_clip_nul");
    emitter.instruction("mov x9, #0");                                          // scan from the first byte
    emitter.label("__rt_kscn_loop");
    emitter.instruction("cmp x9, x1");                                          // did the whole operand pass without a NUL?
    emitter.instruction("b.hs __rt_kscn_done");                                 // keep the full length
    emitter.instruction("ldrb w10, [x0, x9]");                                  // load the next byte
    emitter.instruction("cbz w10, __rt_kscn_done");                             // a NUL ends the C string PHP would hand strcoll
    emitter.instruction("add x9, x9, #1");                                      // advance
    emitter.instruction("b __rt_kscn_loop");                                    // keep scanning
    emitter.label("__rt_kscn_done");
    emitter.instruction("mov x0, x9");                                          // publish the clipped length
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 comparator, decimal formatter, and NUL clipper.
///
/// The frame mirrors the AArch64 one. `__rt_str_to_number` takes its operand in `rax`/`rdx`
/// here because it reaches PHP's numeric grammar through `__rt_cstr`, which reads those.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_key_compare_flagged");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the comparison frame pointer
    emitter.instruction("sub rsp, 176");                                        // reserve the operand words and both digit buffers
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left key payload
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the left key length or integer sentinel
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the right key payload
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the right key length or integer sentinel
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the resolved comparator selector
    emitter.instruction(&format!("cmp r8, {}", super::hash_sort::KEY_COMPARATOR_NUMERIC));                                           // SORT_NUMERIC never looks at the keys as bytes
    emitter.instruction("je __rt_kcf_numeric");                                 // take the numeric path

    emitter.comment("-- every remaining mode compares bytes, so an integer key is spelled out first --");
    emitter.instruction("cmp rsi, -1");                                         // is the left key an integer?
    emitter.instruction("jne __rt_kcf_left_str");                               // a string key already has bytes
    emitter.instruction("lea rsi, [rbp - 96]");                                 // point at the left digit buffer
    emitter.instruction("call __rt_key_sort_int_bytes");                        // render the integer key as decimal digits
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the left byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the left byte length
    emitter.instruction("jmp __rt_kcf_right");                                  // move on to the right key
    emitter.label("__rt_kcf_left_str");
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the left byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rsi");                       // save the left byte length
    emitter.label("__rt_kcf_right");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the right key payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the right key length or integer sentinel
    emitter.instruction("cmp rsi, -1");                                         // is the right key an integer?
    emitter.instruction("jne __rt_kcf_right_str");                              // a string key already has bytes
    emitter.instruction("lea rsi, [rbp - 120]");                                // point at the right digit buffer
    emitter.instruction("call __rt_key_sort_int_bytes");                        // render the integer key as decimal digits
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the right byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the right byte length
    emitter.instruction("jmp __rt_kcf_dispatch");                               // compare the two spellings
    emitter.label("__rt_kcf_right_str");
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // save the right byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rsi");                       // save the right byte length

    emitter.label("__rt_kcf_dispatch");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the resolved comparator selector
    emitter.instruction(&format!("cmp r9, {}", super::hash_sort::KEY_COMPARATOR_LOCALE));                                           // SORT_LOCALE_STRING clips both operands first
    emitter.instruction("je __rt_kcf_locale");                                  // take the locale path
    emitter.instruction(&format!("cmp r9, {}", super::hash_sort::KEY_COMPARATOR_NATURAL));                                           // SORT_NATURAL and its case-folded twin
    emitter.instruction("jge __rt_kcf_natural");                                // take the natural-order path
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // pass the left byte pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the left byte length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // pass the right byte pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // pass the right byte length
    emitter.instruction(&format!("cmp r9, {}", super::hash_sort::KEY_COMPARATOR_STRING_CI));                                           // SORT_STRING | SORT_FLAG_CASE
    emitter.instruction("je __rt_kcf_string_ci");                               // fold ASCII case while comparing
    emitter.instruction("call __rt_strcmp");                                    // zend_binary_strcmp: shared prefix, then length
    emitter.instruction("jmp __rt_kcf_done");                                   // publish the signed result
    emitter.label("__rt_kcf_string_ci");
    emitter.instruction("call __rt_strcasecmp");                                // zend_binary_strcasecmp over the same operands
    emitter.instruction("jmp __rt_kcf_done");                                   // publish the signed result

    emitter.label("__rt_kcf_locale");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // pass the left byte pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the left byte length
    emitter.instruction("call __rt_key_sort_clip_nul");                         // strcoll reads a C string, so stop at the first NUL
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the clipped left length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // pass the right byte pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // pass the right byte length
    emitter.instruction("call __rt_key_sort_clip_nul");                         // strcoll reads a C string, so stop at the first NUL
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the clipped right length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // pass the left byte pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the clipped left length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // pass the right byte pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // pass the clipped right length
    emitter.instruction("call __rt_strcmp");                                    // the C locale collates by unsigned byte order
    emitter.instruction("jmp __rt_kcf_done");                                   // publish the signed result

    emitter.label("__rt_kcf_natural");
    emitter.instruction("xor edi, edi");                                        // assume SORT_NATURAL without case folding
    emitter.instruction(&format!("cmp r9, {}", super::hash_sort::KEY_COMPARATOR_NATURAL_CI));                                           // SORT_NATURAL | SORT_FLAG_CASE
    emitter.instruction("jne __rt_kcf_natural_call");                           // keep the raw-byte comparison
    emitter.instruction("mov edi, 1");                                          // fold ASCII case while comparing
    emitter.label("__rt_kcf_natural_call");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // pass the left byte pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // pass the left byte length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // pass the right byte pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 72]");                        // pass the right byte length
    emitter.instruction("call __rt_strnatcmp");                                 // PHP natural order over the two key spellings
    emitter.instruction("jmp __rt_kcf_done");                                   // publish the signed result

    emitter.comment("-- SORT_NUMERIC: PHP reads each key as a double, integer keys without rounding --");
    emitter.label("__rt_kcf_numeric");
    emitter.instruction("cmp rsi, -1");                                         // is the left key an integer?
    emitter.instruction("jne __rt_kcf_num_left_str");                           // a string key goes through PHP's numeric grammar
    emitter.instruction("cvtsi2sd xmm0, rdi");                                  // widen the integer key to a double
    emitter.instruction("jmp __rt_kcf_num_left_done");                          // the left operand is ready
    emitter.label("__rt_kcf_num_left_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // pass the left key bytes
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the left key length
    emitter.instruction("call __rt_str_to_number");                             // parse the leading numeric run into xmm0
    emitter.label("__rt_kcf_num_left_done");
    emitter.instruction("movsd QWORD PTR [rbp - 128], xmm0");                   // park the left double across the right parse
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the right key payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the right key length or integer sentinel
    emitter.instruction("cmp rsi, -1");                                         // is the right key an integer?
    emitter.instruction("jne __rt_kcf_num_right_str");                          // a string key goes through PHP's numeric grammar
    emitter.instruction("cvtsi2sd xmm1, rdi");                                  // widen the integer key to a double
    emitter.instruction("jmp __rt_kcf_num_right_done");                         // the right operand is ready
    emitter.label("__rt_kcf_num_right_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // pass the right key bytes
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass the right key length
    emitter.instruction("call __rt_str_to_number");                             // parse the leading numeric run into xmm0
    emitter.instruction("movapd xmm1, xmm0");                                   // move it clear of the parked left double
    emitter.label("__rt_kcf_num_right_done");
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 128]");                   // restore the left double
    emitter.instruction("cmp QWORD PTR [rbp - 16], -1");                        // was the left key an integer?
    emitter.instruction("jne __rt_kcf_num_double");                             // a string operand forces the double comparison
    emitter.instruction("cmp QWORD PTR [rbp - 32], -1");                        // was the right key an integer too?
    emitter.instruction("jne __rt_kcf_num_double");                             // a string operand forces the double comparison
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left integer key
    emitter.instruction("cmp rax, QWORD PTR [rbp - 24]");                       // two integer keys compare exactly, never through a double
    emitter.instruction("mov rax, -1");                                         // PHP answers -1 for every non-greater pair here
    emitter.instruction("mov r9, 1");                                           // and 1 otherwise: two hash keys are never equal
    emitter.instruction("cmovg rax, r9");                                       // select the greater-than answer
    emitter.instruction("jmp __rt_kcf_done");                                   // publish the signed result
    emitter.label("__rt_kcf_num_double");
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the two parsed doubles
    emitter.instruction("mov rax, 1");                                          // an unordered or greater pair answers 1
    emitter.instruction("jp __rt_kcf_done");                                    // an unordered pair is already answered
    emitter.instruction("mov r9, -1");                                          // prepare the less-than answer
    emitter.instruction("cmovb rax, r9");                                       // select it when the left double is smaller
    emitter.instruction("mov r9, 0");                                           // prepare the equal answer
    emitter.instruction("cmove rax, r9");                                       // select it when the doubles match

    emitter.label("__rt_kcf_done");
    emitter.instruction("mov rsp, rbp");                                        // release the comparison frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the signed comparison result in rax

    emitter.blank();
    emitter.comment("--- runtime: key_sort_int_bytes ---");
    emitter.label_global("__rt_key_sort_int_bytes");
    emitter.instruction("lea r9, [rsi + 24]");                                  // one past the end of the caller's buffer
    emitter.instruction("mov r10, r9");                                         // digits are written right to left from there
    emitter.instruction("test rdi, rdi");                                       // classify the value once
    emitter.instruction("jnz __rt_ksib_digits");                                // a non-zero value has digits to emit
    emitter.instruction("dec r10");                                             // step back one byte
    emitter.instruction("mov BYTE PTR [r10], 48");                              // zero has exactly one digit
    emitter.instruction("jmp __rt_ksib_done");                                  // the buffer is complete
    emitter.label("__rt_ksib_digits");
    emitter.instruction("mov r11, rdi");                                        // magnitude, read as unsigned so i64::MIN survives
    emitter.instruction("test rdi, rdi");                                       // was the value negative?
    emitter.instruction("jns __rt_ksib_loop");                                  // a positive value is already its own magnitude
    emitter.instruction("neg r11");                                             // negate it inside the unsigned range
    emitter.label("__rt_ksib_loop");
    emitter.instruction("mov rax, r11");                                        // dividend low word
    emitter.instruction("xor edx, edx");                                        // dividend high word
    emitter.instruction("mov rcx, 10");                                         // the decimal radix
    emitter.instruction("div rcx");                                             // unsigned divide so the magnitude stays exact
    emitter.instruction("add rdx, 48");                                         // make the remainder an ASCII digit
    emitter.instruction("dec r10");                                             // step back one byte
    emitter.instruction("mov BYTE PTR [r10], dl");                              // store the digit
    emitter.instruction("mov r11, rax");                                        // keep dividing the quotient
    emitter.instruction("test r11, r11");                                       // until nothing is left
    emitter.instruction("jnz __rt_ksib_loop");                                  // emit the next digit
    emitter.instruction("test rdi, rdi");                                       // was the value negative?
    emitter.instruction("jns __rt_ksib_done");                                  // no sign byte is needed
    emitter.instruction("dec r10");                                             // step back one byte
    emitter.instruction("mov BYTE PTR [r10], 45");                              // store the ASCII minus
    emitter.label("__rt_ksib_done");
    emitter.instruction("mov rax, r10");                                        // publish the first byte written
    emitter.instruction("mov rdx, r9");                                         // publish how many bytes that is
    emitter.instruction("sub rdx, r10");                                        // end minus start
    emitter.instruction("ret");                                                 // return the borrowed pointer/length pair

    emitter.blank();
    emitter.comment("--- runtime: key_sort_clip_nul ---");
    emitter.label_global("__rt_key_sort_clip_nul");
    emitter.instruction("xor eax, eax");                                        // scan from the first byte
    emitter.label("__rt_kscn_loop");
    emitter.instruction("cmp rax, rsi");                                        // did the whole operand pass without a NUL?
    emitter.instruction("jae __rt_kscn_done");                                  // keep the full length
    emitter.instruction("cmp BYTE PTR [rdi + rax], 0");                         // is the next byte a NUL?
    emitter.instruction("je __rt_kscn_done");                                   // a NUL ends the C string PHP would hand strcoll
    emitter.instruction("inc rax");                                             // advance
    emitter.instruction("jmp __rt_kscn_loop");                                  // keep scanning
    emitter.label("__rt_kscn_done");
    emitter.instruction("ret");                                                 // return the clipped length in rax
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits the flag-selected comparator for one target and returns the assembly.
    fn assembly_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_key_compare_flags(&mut emitter);
        emitter.output()
    }

    /// Every mode reaches a shared comparison helper rather than open-coding one.
    ///
    /// The point of spelling an integer key out as decimal digits first is that `SORT_STRING`
    /// and `SORT_NATURAL` can then reuse the comparators the string builtins already use, so
    /// `ksort()` cannot drift away from `strcmp()` and `strcasecmp()`.
    #[test]
    fn every_mode_defers_to_a_shared_comparator() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = assembly_for(target);
            for helper in [
                "__rt_strcmp",
                "__rt_strcasecmp",
                "__rt_strnatcmp",
                "__rt_str_to_number",
            ] {
                assert!(
                    asm.contains(helper),
                    "{target:?} must reach {helper}: {asm}"
                );
            }
        }
    }

    /// `SORT_REGULAR` never reaches this comparator.
    ///
    /// A flagless key sort has to keep running `__rt_key_compare_regular` unchanged; the
    /// dispatch in `hash_sort` owns that choice, so the selector `0` must have no arm here.
    #[test]
    fn the_regular_comparator_is_not_reimplemented_here() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = assembly_for(target);
            assert!(
                !asm.contains("__rt_key_compare_regular"),
                "{target:?} must leave SORT_REGULAR to the original comparator: {asm}"
            );
        }
    }

    /// `SORT_LOCALE_STRING` clips at the first NUL instead of calling libc.
    ///
    /// `strcoll` wants NUL-terminated operands, which a pointer/length key is not, and in the
    /// C locale -- the only locale a program without `setlocale()` is in -- it is byte order
    /// anyway. The clip is what makes the two agree on a key with an embedded NUL.
    #[test]
    fn the_locale_mode_clips_rather_than_calling_libc() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = assembly_for(target);
            assert!(
                asm.contains("__rt_key_sort_clip_nul"),
                "{target:?} must clip the operands: {asm}"
            );
            assert!(
                !asm.contains("strcoll"),
                "{target:?} must not call libc collation: {asm}"
            );
        }
    }

    /// Two integer keys compare exactly under `SORT_NUMERIC`, never through a double.
    ///
    /// Rounding them would make `9223372036854775806` and `9223372036854775807` compare equal,
    /// and PHP orders them.
    #[test]
    fn integer_keys_skip_the_double_conversion() {
        let arm = assembly_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("cmp x9, x10"));
        assert!(arm.contains("csel x0, x11, x0, gt"));
        let x86 = assembly_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x86.contains("cmp rax, QWORD PTR [rbp - 24]"));
        assert!(x86.contains("cmovg rax, r9"));
    }
}
