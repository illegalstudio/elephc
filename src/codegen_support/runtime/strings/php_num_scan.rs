//! Purpose:
//! Emits `__rt_php_num_scan`, the runtime implementation of PHP's numeric-string
//! grammar (`_is_numeric_string_ex`). It clips a NUL-terminated C string down to its
//! longest leading numeric run so libc `strtod`/`strtoll` see exactly the bytes PHP
//! would accept, and reports whether the whole string was numeric.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//! - `__rt_str_to_number`, `__rt_str_to_int` and
//!   `__rt_str_looks_like_int_for_coercion` (all in
//!   `crate::codegen_support::runtime::strings`) after they materialize the PHP string
//!   through `__rt_cstr`.
//!
//! Key details:
//! - This is the RUNTIME twin of the compile-time scanner in
//!   `crate::optimize::fold::compare::scan_numeric_prefix`; the two must agree byte for
//!   byte or a literal and a runtime value give different answers for the same cast.
//! - Grammar: optional PHP whitespace (`' '`, `\t`, `\n`, `\v`, `\f`, `\r`), optional
//!   `+`/`-`, decimal digits, an optional `.` with more digits (`12`, `.5`, `5.` all
//!   qualify as long as at least one digit was seen), and an optional `e`/`E` exponent
//!   that is only consumed when at least one digit follows it. There is NO hexadecimal
//!   form, NO underscore separator, and NO `INF`/`NAN` spelling — those are libc
//!   `strtod` extensions PHP does not have, which is exactly why the string must be
//!   clipped before `strtod` ever sees it.
//! - The clip is written in place into the `__rt_cstr` scratch buffer, which the caller
//!   owns until its next `__rt_cstr` call. When there is no numeric prefix at all the
//!   run is made empty, so `strtod`/`strtoll` consume nothing and yield PHP's `0`/`0.0`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_php_num_scan`: clip a C string to PHP's leading numeric run.
///
/// Input: AArch64 `x0` / x86_64 `rdi` = pointer to a NUL-terminated, writable C string
/// (the `__rt_cstr` scratch buffer).
///
/// Output: AArch64 `x0` / x86_64 `rax` = pointer to the first byte of the numeric run
/// (past any leading whitespace), NUL-terminated in place at the end of the run;
/// AArch64 `x1` / x86_64 `rdx` = `1` when the string was FULLY numeric (`is_numeric`
/// semantics: only PHP whitespace follows the run), `0` otherwise.
/// AArch64 `x2` / x86_64 `rdi` = `1` when the accepted run uses decimal/exponent syntax,
/// `0` when its numeric representation is integer-like.
///
/// The helper is a leaf: it makes no calls and needs no stack frame.
pub fn emit_php_num_scan(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_php_num_scan_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: php_num_scan (PHP numeric-string grammar) ---");
    emitter.label_global("__rt_php_num_scan");

    emitter.instruction("mov x9, x0");                                          // x9 = scan cursor over the C string
    emitter.instruction("mov x2, xzr");                                         // default to integer-like numeric syntax

    // -- skip PHP leading whitespace: ' ' plus the 9..13 control range --
    emitter.label("__rt_pns_ws");
    emitter.instruction("ldrb w10, [x9]");                                      // load the next candidate whitespace byte
    emitter.instruction("cmp w10, #32");                                        // ASCII space is PHP whitespace
    emitter.instruction("b.eq __rt_pns_ws_next");                               // skip an allowed leading space
    emitter.instruction("sub w11, w10, #9");                                    // normalize the tab/newline/vtab/formfeed/return range
    emitter.instruction("cmp w11, #4");                                         // bytes 9 through 13 are PHP whitespace
    emitter.instruction("b.hi __rt_pns_sign");                                  // not whitespace: the numeric run starts here
    emitter.label("__rt_pns_ws_next");
    emitter.instruction("add x9, x9, #1");                                      // advance past one whitespace byte
    emitter.instruction("b __rt_pns_ws");                                       // keep skipping leading whitespace

    // -- optional sign --
    emitter.label("__rt_pns_sign");
    emitter.instruction("mov x12, x9");                                         // x12 = start of the numeric run
    emitter.instruction("cmp w10, #43");                                        // ASCII '+' may lead the run
    emitter.instruction("b.eq __rt_pns_sign_skip");                             // consume the leading plus
    emitter.instruction("cmp w10, #45");                                        // ASCII '-' may lead the run
    emitter.instruction("b.ne __rt_pns_int");                                   // no sign: start on the integer digits
    emitter.label("__rt_pns_sign_skip");
    emitter.instruction("add x9, x9, #1");                                      // consume the sign byte

    // -- integer digits --
    emitter.label("__rt_pns_int");
    emitter.instruction("mov x13, #0");                                         // x13 = total digit count seen so far
    emitter.label("__rt_pns_int_loop");
    emitter.instruction("ldrb w10, [x9]");                                      // load the next integer-part byte
    emitter.instruction("sub w11, w10, #48");                                   // normalize to a candidate decimal digit
    emitter.instruction("cmp w11, #9");                                         // verify the decimal digit range
    emitter.instruction("b.hi __rt_pns_dot");                                   // non-digit: try the fractional part
    emitter.instruction("add x9, x9, #1");                                      // consume the digit
    emitter.instruction("add x13, x13, #1");                                    // record one more digit
    emitter.instruction("b __rt_pns_int_loop");                                 // keep consuming integer digits

    // -- optional '.' followed by more digits; "5." counts once a digit was seen --
    emitter.label("__rt_pns_dot");
    emitter.instruction("cmp w10, #46");                                        // ASCII '.' introduces the fractional part
    emitter.instruction("b.ne __rt_pns_after_mantissa");                        // no decimal point: mantissa is complete
    emitter.instruction("add x14, x9, #1");                                     // probe cursor just past the '.'
    emitter.label("__rt_pns_frac_loop");
    emitter.instruction("ldrb w11, [x14]");                                     // load the next fractional byte
    emitter.instruction("sub w15, w11, #48");                                   // normalize to a candidate decimal digit
    emitter.instruction("cmp w15, #9");                                         // verify the decimal digit range
    emitter.instruction("b.hi __rt_pns_frac_done");                             // fractional digits are complete
    emitter.instruction("add x14, x14, #1");                                    // consume the fractional digit
    emitter.instruction("add x13, x13, #1");                                    // record one more digit
    emitter.instruction("b __rt_pns_frac_loop");                                // keep consuming fractional digits
    emitter.label("__rt_pns_frac_done");
    emitter.instruction("cbz x13, __rt_pns_after_mantissa");                    // a lone '.' is not part of any numeric run
    emitter.instruction("mov x2, #1");                                          // accepted decimal syntax requires floating-point arithmetic
    emitter.instruction("mov x9, x14");                                         // accept the '.' and its fractional digits

    // -- a run with no digit at all is not numeric --
    emitter.label("__rt_pns_after_mantissa");
    emitter.instruction("cbz x13, __rt_pns_none");                              // no digits anywhere: report no numeric prefix

    // -- optional exponent, consumed only when at least one digit follows --
    emitter.instruction("ldrb w10, [x9]");                                      // load the byte after the mantissa
    emitter.instruction("orr w11, w10, #0x20");                                 // fold it to lowercase ASCII
    emitter.instruction("cmp w11, #101");                                       // lowercase 'e' introduces the exponent
    emitter.instruction("b.ne __rt_pns_end");                                   // no exponent marker: the run ends here
    emitter.instruction("add x14, x9, #1");                                     // probe cursor just past the exponent marker
    emitter.instruction("ldrb w11, [x14]");                                     // load the optional exponent sign
    emitter.instruction("cmp w11, #43");                                        // ASCII '+' may lead the exponent
    emitter.instruction("b.eq __rt_pns_exp_sign");                              // consume the exponent plus
    emitter.instruction("cmp w11, #45");                                        // ASCII '-' may lead the exponent
    emitter.instruction("b.ne __rt_pns_exp_init");                              // no exponent sign: start on the digits
    emitter.label("__rt_pns_exp_sign");
    emitter.instruction("add x14, x14, #1");                                    // consume the exponent sign
    emitter.label("__rt_pns_exp_init");
    emitter.instruction("mov x15, x14");                                        // remember where the exponent digits begin
    emitter.label("__rt_pns_exp_loop");
    emitter.instruction("ldrb w11, [x14]");                                     // load the next exponent byte
    emitter.instruction("sub w16, w11, #48");                                   // normalize to a candidate decimal digit
    emitter.instruction("cmp w16, #9");                                         // verify the decimal digit range
    emitter.instruction("b.hi __rt_pns_exp_done");                              // exponent digits are complete
    emitter.instruction("add x14, x14, #1");                                    // consume the exponent digit
    emitter.instruction("b __rt_pns_exp_loop");                                 // keep consuming exponent digits
    emitter.label("__rt_pns_exp_done");
    emitter.instruction("cmp x14, x15");                                        // did the exponent contain any digit?
    emitter.instruction("b.ls __rt_pns_end");                                   // bare "1e" keeps the 'e' out of the run
    emitter.instruction("mov x2, #1");                                          // accepted exponent syntax requires floating-point arithmetic
    emitter.instruction("mov x9, x14");                                         // accept the exponent

    // -- classify the trailing bytes: only PHP whitespace keeps the string numeric --
    emitter.label("__rt_pns_end");
    emitter.instruction("mov x14, x9");                                         // x14 = trailing-byte scan cursor
    emitter.label("__rt_pns_trail");
    emitter.instruction("ldrb w10, [x14]");                                     // load the next trailing byte
    emitter.instruction("cbz w10, __rt_pns_trail_ok");                          // end of string: the whole string was numeric
    emitter.instruction("cmp w10, #32");                                        // ASCII space is allowed after the run
    emitter.instruction("b.eq __rt_pns_trail_next");                            // keep scanning after an allowed space
    emitter.instruction("sub w11, w10, #9");                                    // normalize the tab/newline/vtab/formfeed/return range
    emitter.instruction("cmp w11, #4");                                         // bytes 9 through 13 are PHP whitespace
    emitter.instruction("b.hi __rt_pns_trail_bad");                             // any other byte makes the string non-numeric
    emitter.label("__rt_pns_trail_next");
    emitter.instruction("add x14, x14, #1");                                    // advance past one trailing whitespace byte
    emitter.instruction("b __rt_pns_trail");                                    // keep scanning trailing whitespace
    emitter.label("__rt_pns_trail_ok");
    emitter.instruction("mov x1, #1");                                          // report a fully numeric string
    emitter.instruction("b __rt_pns_finish");                                   // clip the run and return
    emitter.label("__rt_pns_trail_bad");
    emitter.instruction("mov x1, #0");                                          // report a leading-numeric-only string

    emitter.label("__rt_pns_finish");
    emitter.instruction("strb wzr, [x9]");                                      // clip the scratch buffer at the end of the run
    emitter.instruction("mov x0, x12");                                         // return the pointer to the numeric run
    emitter.instruction("ret");                                                 // return run pointer (x0) and numeric flag (x1)

    emitter.label("__rt_pns_none");
    emitter.instruction("mov x1, #0");                                          // no numeric prefix means not a numeric string
    emitter.instruction("strb wzr, [x12]");                                     // make the run empty so strtod/strtoll yield zero
    emitter.instruction("mov x0, x12");                                         // return the empty run pointer
    emitter.instruction("ret");                                                 // return run pointer (x0) and numeric flag (x1)
}

/// Emits the Linux x86_64 variant of `__rt_php_num_scan`.
///
/// Mirrors the AArch64 grammar exactly using SysV registers.
/// Input: `rdi` = pointer to a NUL-terminated, writable C string.
/// Output: `rax` = pointer to the numeric run, `rdx` = fully-numeric flag,
/// `rdi` = decimal/exponent syntax flag.
fn emit_php_num_scan_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php_num_scan (PHP numeric-string grammar) ---");
    emitter.label_global("__rt_php_num_scan");

    emitter.instruction("mov r8, rdi");                                         // r8 = scan cursor over the C string
    emitter.instruction("xor edi, edi");                                        // default to integer-like numeric syntax

    emitter.label("__rt_pns_ws_x");
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // load the next candidate whitespace byte
    emitter.instruction("cmp cl, 32");                                          // ASCII space is PHP whitespace
    emitter.instruction("je __rt_pns_ws_next_x");                               // skip an allowed leading space
    emitter.instruction("mov r9d, ecx");                                        // copy the byte before normalizing the range
    emitter.instruction("sub r9d, 9");                                          // normalize the tab/newline/vtab/formfeed/return range
    emitter.instruction("cmp r9d, 4");                                          // bytes 9 through 13 are PHP whitespace
    emitter.instruction("ja __rt_pns_sign_x");                                  // not whitespace: the numeric run starts here
    emitter.label("__rt_pns_ws_next_x");
    emitter.instruction("inc r8");                                              // advance past one whitespace byte
    emitter.instruction("jmp __rt_pns_ws_x");                                   // keep skipping leading whitespace

    emitter.label("__rt_pns_sign_x");
    emitter.instruction("mov r10, r8");                                         // r10 = start of the numeric run
    emitter.instruction("cmp cl, 43");                                          // ASCII '+' may lead the run
    emitter.instruction("je __rt_pns_sign_skip_x");                             // consume the leading plus
    emitter.instruction("cmp cl, 45");                                          // ASCII '-' may lead the run
    emitter.instruction("jne __rt_pns_int_x");                                  // no sign: start on the integer digits
    emitter.label("__rt_pns_sign_skip_x");
    emitter.instruction("inc r8");                                              // consume the sign byte

    emitter.label("__rt_pns_int_x");
    emitter.instruction("xor r11d, r11d");                                      // r11 = total digit count seen so far
    emitter.label("__rt_pns_int_loop_x");
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // load the next integer-part byte
    emitter.instruction("mov r9d, ecx");                                        // copy the byte before normalizing the range
    emitter.instruction("sub r9d, 48");                                         // normalize to a candidate decimal digit
    emitter.instruction("cmp r9d, 9");                                          // verify the decimal digit range
    emitter.instruction("ja __rt_pns_dot_x");                                   // non-digit: try the fractional part
    emitter.instruction("inc r8");                                              // consume the digit
    emitter.instruction("inc r11");                                             // record one more digit
    emitter.instruction("jmp __rt_pns_int_loop_x");                             // keep consuming integer digits

    emitter.label("__rt_pns_dot_x");
    emitter.instruction("cmp cl, 46");                                          // ASCII '.' introduces the fractional part
    emitter.instruction("jne __rt_pns_after_mantissa_x");                       // no decimal point: mantissa is complete
    emitter.instruction("lea rsi, [r8 + 1]");                                   // probe cursor just past the '.'
    emitter.label("__rt_pns_frac_loop_x");
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // load the next fractional byte
    emitter.instruction("mov r9d, ecx");                                        // copy the byte before normalizing the range
    emitter.instruction("sub r9d, 48");                                         // normalize to a candidate decimal digit
    emitter.instruction("cmp r9d, 9");                                          // verify the decimal digit range
    emitter.instruction("ja __rt_pns_frac_done_x");                             // fractional digits are complete
    emitter.instruction("inc rsi");                                             // consume the fractional digit
    emitter.instruction("inc r11");                                             // record one more digit
    emitter.instruction("jmp __rt_pns_frac_loop_x");                            // keep consuming fractional digits
    emitter.label("__rt_pns_frac_done_x");
    emitter.instruction("test r11, r11");                                       // did the run contain any digit?
    emitter.instruction("jz __rt_pns_after_mantissa_x");                        // a lone '.' is not part of any numeric run
    emitter.instruction("mov edi, 1");                                          // accepted decimal syntax requires floating-point arithmetic
    emitter.instruction("mov r8, rsi");                                         // accept the '.' and its fractional digits

    emitter.label("__rt_pns_after_mantissa_x");
    emitter.instruction("test r11, r11");                                       // did the run contain any digit?
    emitter.instruction("jz __rt_pns_none_x");                                  // no digits anywhere: report no numeric prefix

    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // load the byte after the mantissa
    emitter.instruction("mov r9d, ecx");                                        // copy the byte before case folding
    emitter.instruction("or r9d, 32");                                          // fold it to lowercase ASCII
    emitter.instruction("cmp r9d, 101");                                        // lowercase 'e' introduces the exponent
    emitter.instruction("jne __rt_pns_end_x");                                  // no exponent marker: the run ends here
    emitter.instruction("lea rsi, [r8 + 1]");                                   // probe cursor just past the exponent marker
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // load the optional exponent sign
    emitter.instruction("cmp cl, 43");                                          // ASCII '+' may lead the exponent
    emitter.instruction("je __rt_pns_exp_sign_x");                              // consume the exponent plus
    emitter.instruction("cmp cl, 45");                                          // ASCII '-' may lead the exponent
    emitter.instruction("jne __rt_pns_exp_init_x");                             // no exponent sign: start on the digits
    emitter.label("__rt_pns_exp_sign_x");
    emitter.instruction("inc rsi");                                             // consume the exponent sign
    emitter.label("__rt_pns_exp_init_x");
    emitter.instruction("mov r9, rsi");                                         // remember where the exponent digits begin
    emitter.label("__rt_pns_exp_loop_x");
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // load the next exponent byte
    emitter.instruction("mov eax, ecx");                                        // copy the byte before normalizing the range
    emitter.instruction("sub eax, 48");                                         // normalize to a candidate decimal digit
    emitter.instruction("cmp eax, 9");                                          // verify the decimal digit range
    emitter.instruction("ja __rt_pns_exp_done_x");                              // exponent digits are complete
    emitter.instruction("inc rsi");                                             // consume the exponent digit
    emitter.instruction("jmp __rt_pns_exp_loop_x");                             // keep consuming exponent digits
    emitter.label("__rt_pns_exp_done_x");
    emitter.instruction("cmp rsi, r9");                                         // did the exponent contain any digit?
    emitter.instruction("jbe __rt_pns_end_x");                                  // bare "1e" keeps the 'e' out of the run
    emitter.instruction("mov edi, 1");                                          // accepted exponent syntax requires floating-point arithmetic
    emitter.instruction("mov r8, rsi");                                         // accept the exponent

    emitter.label("__rt_pns_end_x");
    emitter.instruction("mov rsi, r8");                                         // rsi = trailing-byte scan cursor
    emitter.label("__rt_pns_trail_x");
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // load the next trailing byte
    emitter.instruction("test cl, cl");                                         // check for the C-string terminator
    emitter.instruction("jz __rt_pns_trail_ok_x");                              // end of string: the whole string was numeric
    emitter.instruction("cmp cl, 32");                                          // ASCII space is allowed after the run
    emitter.instruction("je __rt_pns_trail_next_x");                            // keep scanning after an allowed space
    emitter.instruction("mov r9d, ecx");                                        // copy the byte before normalizing the range
    emitter.instruction("sub r9d, 9");                                          // normalize the tab/newline/vtab/formfeed/return range
    emitter.instruction("cmp r9d, 4");                                          // bytes 9 through 13 are PHP whitespace
    emitter.instruction("ja __rt_pns_trail_bad_x");                             // any other byte makes the string non-numeric
    emitter.label("__rt_pns_trail_next_x");
    emitter.instruction("inc rsi");                                             // advance past one trailing whitespace byte
    emitter.instruction("jmp __rt_pns_trail_x");                                // keep scanning trailing whitespace
    emitter.label("__rt_pns_trail_ok_x");
    emitter.instruction("mov edx, 1");                                          // report a fully numeric string
    emitter.instruction("jmp __rt_pns_finish_x");                               // clip the run and return
    emitter.label("__rt_pns_trail_bad_x");
    emitter.instruction("xor edx, edx");                                        // report a leading-numeric-only string

    emitter.label("__rt_pns_finish_x");
    emitter.instruction("mov BYTE PTR [r8], 0");                                // clip the scratch buffer at the end of the run
    emitter.instruction("mov rax, r10");                                        // return the pointer to the numeric run
    emitter.instruction("ret");                                                 // return run pointer (rax) and numeric flag (rdx)

    emitter.label("__rt_pns_none_x");
    emitter.instruction("xor edx, edx");                                        // no numeric prefix means not a numeric string
    emitter.instruction("mov BYTE PTR [r10], 0");                               // make the run empty so strtod/strtoll yield zero
    emitter.instruction("mov rax, r10");                                        // return the empty run pointer
    emitter.instruction("ret");                                                 // return run pointer (rax) and numeric flag (rdx)
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies both targets emit the whole PHP grammar: whitespace skip, optional sign,
    /// mantissa, guarded exponent, and the trailing-whitespace classification.
    #[test]
    fn test_emit_php_num_scan_covers_full_grammar() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_php_num_scan(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_php_num_scan:\n"), "missing entry point for {:?}", arch);
            for fragment in ["__rt_pns_ws", "__rt_pns_sign", "__rt_pns_int", "__rt_pns_frac", "__rt_pns_exp", "__rt_pns_trail", "__rt_pns_none"] {
                assert!(asm.contains(fragment), "missing {} for {:?}", fragment, arch);
            }
        }
    }

    /// Verifies the helper is a leaf routine: PHP's grammar is scanned without any call,
    /// so callers can invoke it between `__rt_cstr` and `strtod` without a frame of their own.
    #[test]
    fn test_emit_php_num_scan_is_leaf() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_php_num_scan(&mut emitter);
            let asm = emitter.output();
            assert!(!asm.contains("    bl "), "unexpected call for {:?}", arch);
            assert!(!asm.contains("    call "), "unexpected call for {:?}", arch);
        }
    }
}
