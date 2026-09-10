//! Purpose:
//! Classifies a valid PHP numeric string as the integer or floating union member.
//! Preserves `int|float` selection at signed 64-bit boundaries on every target.
//!
//! Called from:
//! - Userspace stream-wrapper callback adapters before weak union coercion.
//!
//! Key details:
//! - Returns 0 for non-numeric, 1 for integer, 2 for out-of-range float, and 3 for int-coercible float.
//! - Decimal/exponent forms and integer-form overflows select float exactly like php-src.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_str_numeric_union_kind` for the active target.
pub fn emit_str_numeric_union_kind(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_numeric_union_kind_linux_x86_64(emitter);
        return;
    }
    emit_str_numeric_union_kind_aarch64(emitter);
}

/// Emits the AArch64 numeric-string union classifier.
fn emit_str_numeric_union_kind_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_numeric_union_kind ---");
    emitter.label_global("__rt_str_numeric_union_kind");
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved string state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame across numeric validation
    emitter.instruction("add x29, sp, #32");                                    // establish the classifier frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the bounded PHP string pointer and length
    emitter.instruction("bl __rt_str_looks_like_int_for_coercion");             // reject non-numeric and libc-only numeric spellings
    emitter.instruction("cbz x0, __rt_snuk_return");                            // return kind 0 for a non-numeric PHP string
    emitter.instruction("ldp x9, x10, [sp, #0]");                               // restore scan pointer and remaining byte count

    emitter.label("__rt_snuk_marker_loop");
    emitter.instruction("cbz x10, __rt_snuk_prepare_integer");                  // no decimal/exponent marker means integer-form syntax
    emitter.instruction("ldrb w11, [x9], #1");                                  // consume one numeric-string byte
    emitter.instruction("sub x10, x10, #1");                                    // decrement the bounded scan count
    emitter.instruction("cmp w11, #46");                                        // decimal point forces the float union member
    emitter.instruction("b.eq __rt_snuk_float");                                // classify a decimal-form numeric string as float
    emitter.instruction("orr w11, w11, #0x20");                                 // fold exponent markers to lowercase ASCII
    emitter.instruction("cmp w11, #101");                                       // e/E forces the float union member
    emitter.instruction("b.eq __rt_snuk_float");                                // classify an exponent-form numeric string as float
    emitter.instruction("b __rt_snuk_marker_loop");                             // scan the remaining bounded bytes

    emitter.label("__rt_snuk_prepare_integer");
    emitter.instruction("ldp x9, x10, [sp, #0]");                               // restart at the original bounded string
    emitter.instruction("mov x11, #0");                                         // negative-sign flag defaults to false
    emitter.label("__rt_snuk_leading_ws");
    emitter.instruction("ldrb w12, [x9]");                                      // inspect one leading byte
    emitter.instruction("cmp w12, #32");                                        // ASCII space is accepted around numeric strings
    emitter.instruction("b.eq __rt_snuk_leading_ws_next");                      // skip an allowed leading space
    emitter.instruction("sub w13, w12, #9");                                    // normalize tab through carriage return
    emitter.instruction("cmp w13, #4");                                         // bytes 9..13 are accepted PHP whitespace
    emitter.instruction("b.hi __rt_snuk_sign");                                 // first non-whitespace byte begins the numeric payload
    emitter.label("__rt_snuk_leading_ws_next");
    emitter.instruction("add x9, x9, #1");                                      // consume one leading whitespace byte
    emitter.instruction("sub x10, x10, #1");                                    // keep the scan within the PHP string bound
    emitter.instruction("b __rt_snuk_leading_ws");                              // continue trimming leading whitespace

    emitter.label("__rt_snuk_sign");
    emitter.instruction("cmp w12, #43");                                        // check for a leading plus sign
    emitter.instruction("b.eq __rt_snuk_sign_next");                            // plus does not change the overflow threshold
    emitter.instruction("cmp w12, #45");                                        // check for a leading minus sign
    emitter.instruction("b.ne __rt_snuk_leading_zero");                         // unsigned payload starts at the current byte
    emitter.instruction("mov x11, #1");                                         // remember the negative signed-boundary threshold
    emitter.label("__rt_snuk_sign_next");
    emitter.instruction("add x9, x9, #1");                                      // consume the optional sign
    emitter.instruction("sub x10, x10, #1");                                    // reduce the bounded payload length

    emitter.label("__rt_snuk_leading_zero");
    emitter.instruction("cbz x10, __rt_snuk_int");                              // a validated all-zero payload is integer form
    emitter.instruction("ldrb w12, [x9]");                                      // inspect the next payload byte
    emitter.instruction("cmp w12, #48");                                        // leading zeroes do not count toward integer overflow
    emitter.instruction("b.ne __rt_snuk_digits_start");                         // first significant digit starts accumulation
    emitter.instruction("add x9, x9, #1");                                      // skip one leading zero
    emitter.instruction("sub x10, x10, #1");                                    // consume it from the bounded input
    emitter.instruction("b __rt_snuk_leading_zero");                            // keep removing insignificant zeroes

    emitter.label("__rt_snuk_digits_start");
    emitter.instruction("mov x12, #0");                                         // unsigned significant-digit accumulator
    emitter.instruction("mov x13, #0");                                         // significant digit count
    emitter.label("__rt_snuk_digits");
    emitter.instruction("cbz x10, __rt_snuk_digits_done");                      // end of bounded string finishes the integer scan
    emitter.instruction("ldrb w14, [x9]");                                      // inspect the next potential decimal digit
    emitter.instruction("sub w15, w14, #48");                                   // normalize ASCII digit zero to integer zero
    emitter.instruction("cmp w15, #9");                                         // values outside 0..9 begin trailing whitespace
    emitter.instruction("b.hi __rt_snuk_digits_done");                          // validated input has no other trailing syntax
    emitter.instruction("mov x16, #10");                                        // decimal accumulation multiplier
    emitter.instruction("madd x12, x12, x16, x15");                             // accumulator = accumulator * 10 + digit
    emitter.instruction("add x13, x13, #1");                                    // count one significant digit
    emitter.instruction("add x9, x9, #1");                                      // advance to the next input byte
    emitter.instruction("sub x10, x10, #1");                                    // consume one bounded byte
    emitter.instruction("b __rt_snuk_digits");                                  // continue accumulating significant digits

    emitter.label("__rt_snuk_digits_done");
    emitter.instruction("cmp x13, #19");                                        // signed 64-bit limits contain 19 significant digits
    emitter.instruction("b.lo __rt_snuk_int");                                  // fewer digits always fit a PHP integer
    emitter.instruction("b.hi __rt_snuk_float");                                // more digits necessarily classify as double
    abi::emit_load_int_immediate(emitter, "x14", i64::MAX);
    emitter.instruction("cbz x11, __rt_snuk_compare_limit");                    // positive integers use PHP_INT_MAX
    abi::emit_load_int_immediate(emitter, "x14", i64::MIN);
    emitter.label("__rt_snuk_compare_limit");
    emitter.instruction("cmp x12, x14");                                        // compare unsigned magnitude with the signed boundary
    emitter.instruction("b.hi __rt_snuk_float");                                // overflowed integer-form numeric strings select float

    emitter.label("__rt_snuk_int");
    emitter.instruction("mov x0, #1");                                          // report PHP integer numeric-string kind
    emitter.instruction("b __rt_snuk_return");                                  // share frame restoration
    emitter.label("__rt_snuk_float");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore the original string for double parsing
    emitter.instruction("bl __rt_str_to_number");                               // parse the validated float-form numeric string
    abi::emit_load_int_immediate(emitter, "x9", 0x43e0000000000000);
    emitter.instruction("fmov d1, x9");                                         // materialize positive 2^63 as the exclusive long bound
    emitter.instruction("fcmp d0, d1");                                         // compare the parsed double with PHP's positive integer bound
    emitter.instruction("b.ge __rt_snuk_float_out_of_range");                   // positive overflow cannot weakly coerce to int
    abi::emit_load_int_immediate(emitter, "x9", 0xc3e0000000000000u64 as i64);
    emitter.instruction("fmov d1, x9");                                         // materialize negative 2^63 as the inclusive long bound
    emitter.instruction("fcmp d0, d1");                                         // compare the parsed double with PHP's negative integer bound
    emitter.instruction("b.lt __rt_snuk_float_out_of_range");                   // negative overflow cannot weakly coerce to int
    emitter.instruction("mov x0, #3");                                          // report a float form that an int-only hint may coerce
    emitter.instruction("b __rt_snuk_return");                                  // share frame restoration
    emitter.label("__rt_snuk_float_out_of_range");
    emitter.instruction("mov x0, #2");                                          // report a float form outside PHP integer range
    emitter.label("__rt_snuk_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame linkage
    emitter.instruction("add sp, sp, #48");                                     // release classifier scratch storage
    emitter.instruction("ret");                                                 // return one numeric-string kind from 0 through 3
}

/// Emits the Linux x86_64 numeric-string union classifier.
fn emit_str_numeric_union_kind_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_numeric_union_kind ---");
    emitter.label_global("__rt_str_numeric_union_kind");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the classifier frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve saved string state and sign flag
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the bounded PHP string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the bounded PHP string length
    emitter.instruction("call __rt_str_looks_like_int_for_coercion");           // reject non-numeric and libc-only numeric spellings
    emitter.instruction("test rax, rax");                                       // inspect numeric validity
    emitter.instruction("jz __rt_snuk_return_x86_64");                          // return kind 0 for a non-numeric PHP string
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore scan pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // restore remaining byte count

    emitter.label("__rt_snuk_marker_loop_x86_64");
    emitter.instruction("test r10, r10");                                       // did the bounded marker scan finish?
    emitter.instruction("jz __rt_snuk_prepare_integer_x86_64");                 // no marker means integer-form syntax
    emitter.instruction("movzx r11d, BYTE PTR [r9]");                           // consume one numeric-string byte
    emitter.instruction("add r9, 1");                                           // advance the marker cursor
    emitter.instruction("sub r10, 1");                                          // decrement the bounded scan count
    emitter.instruction("cmp r11d, 46");                                        // decimal point forces the float union member
    emitter.instruction("je __rt_snuk_float_x86_64");                           // classify decimal form as float
    emitter.instruction("or r11d, 32");                                         // fold exponent markers to lowercase ASCII
    emitter.instruction("cmp r11d, 101");                                       // e/E forces the float union member
    emitter.instruction("je __rt_snuk_float_x86_64");                           // classify exponent form as float
    emitter.instruction("jmp __rt_snuk_marker_loop_x86_64");                    // scan the remaining bounded bytes

    emitter.label("__rt_snuk_prepare_integer_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restart at the original bounded string
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // restore the original byte count
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // negative-sign flag defaults to false
    emitter.label("__rt_snuk_leading_ws_x86_64");
    emitter.instruction("movzx r11d, BYTE PTR [r9]");                           // inspect one leading byte
    emitter.instruction("cmp r11d, 32");                                        // ASCII space is accepted around numeric strings
    emitter.instruction("je __rt_snuk_leading_ws_next_x86_64");                 // skip an allowed leading space
    emitter.instruction("mov r12d, r11d");                                      // copy the byte for control-whitespace normalization
    emitter.instruction("sub r12d, 9");                                         // normalize tab through carriage return
    emitter.instruction("cmp r12d, 4");                                         // bytes 9..13 are accepted PHP whitespace
    emitter.instruction("ja __rt_snuk_sign_x86_64");                            // first non-whitespace byte begins the payload
    emitter.label("__rt_snuk_leading_ws_next_x86_64");
    emitter.instruction("add r9, 1");                                           // consume one leading whitespace byte
    emitter.instruction("sub r10, 1");                                          // keep the scan within the PHP string bound
    emitter.instruction("jmp __rt_snuk_leading_ws_x86_64");                     // continue trimming leading whitespace

    emitter.label("__rt_snuk_sign_x86_64");
    emitter.instruction("cmp r11d, 43");                                        // check for a leading plus sign
    emitter.instruction("je __rt_snuk_sign_next_x86_64");                       // plus keeps the positive overflow threshold
    emitter.instruction("cmp r11d, 45");                                        // check for a leading minus sign
    emitter.instruction("jne __rt_snuk_leading_zero_x86_64");                   // unsigned payload starts at the current byte
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // remember the negative signed-boundary threshold
    emitter.label("__rt_snuk_sign_next_x86_64");
    emitter.instruction("add r9, 1");                                           // consume the optional sign
    emitter.instruction("sub r10, 1");                                          // reduce the bounded payload length

    emitter.label("__rt_snuk_leading_zero_x86_64");
    emitter.instruction("test r10, r10");                                       // did a validated all-zero payload end?
    emitter.instruction("jz __rt_snuk_int_x86_64");                             // all-zero forms are integers
    emitter.instruction("movzx r11d, BYTE PTR [r9]");                           // inspect the next payload byte
    emitter.instruction("cmp r11d, 48");                                        // leading zeroes do not count toward overflow
    emitter.instruction("jne __rt_snuk_digits_start_x86_64");                   // first significant digit starts accumulation
    emitter.instruction("add r9, 1");                                           // skip one insignificant zero
    emitter.instruction("sub r10, 1");                                          // consume it from the bounded input
    emitter.instruction("jmp __rt_snuk_leading_zero_x86_64");                   // keep removing insignificant zeroes

    emitter.label("__rt_snuk_digits_start_x86_64");
    emitter.instruction("xor r12, r12");                                        // unsigned significant-digit accumulator
    emitter.instruction("xor r13, r13");                                        // significant digit count
    emitter.label("__rt_snuk_digits_x86_64");
    emitter.instruction("test r10, r10");                                       // did the bounded integer scan finish?
    emitter.instruction("jz __rt_snuk_digits_done_x86_64");                     // finish at the end of the PHP string
    emitter.instruction("movzx r14d, BYTE PTR [r9]");                           // inspect one potential decimal digit
    emitter.instruction("sub r14d, 48");                                        // normalize ASCII digit zero
    emitter.instruction("cmp r14d, 9");                                         // values outside 0..9 begin trailing whitespace
    emitter.instruction("ja __rt_snuk_digits_done_x86_64");                     // validated input has no other trailing syntax
    emitter.instruction("imul r12, r12, 10");                                   // multiply the unsigned magnitude by ten
    emitter.instruction("add r12, r14");                                        // add the current decimal digit
    emitter.instruction("add r13, 1");                                          // count one significant digit
    emitter.instruction("add r9, 1");                                           // advance to the next input byte
    emitter.instruction("sub r10, 1");                                          // consume one bounded byte
    emitter.instruction("jmp __rt_snuk_digits_x86_64");                         // continue accumulating significant digits

    emitter.label("__rt_snuk_digits_done_x86_64");
    emitter.instruction("cmp r13, 19");                                         // signed 64-bit limits contain 19 significant digits
    emitter.instruction("jb __rt_snuk_int_x86_64");                             // fewer digits always fit a PHP integer
    emitter.instruction("ja __rt_snuk_float_x86_64");                           // more digits necessarily classify as double
    abi::emit_load_int_immediate(emitter, "r14", i64::MAX);
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // choose the positive or negative magnitude threshold
    emitter.instruction("je __rt_snuk_compare_limit_x86_64");                   // positive integers use PHP_INT_MAX
    abi::emit_load_int_immediate(emitter, "r14", i64::MIN);
    emitter.label("__rt_snuk_compare_limit_x86_64");
    emitter.instruction("cmp r12, r14");                                        // compare unsigned magnitude with the signed boundary
    emitter.instruction("ja __rt_snuk_float_x86_64");                           // overflowed integer-form strings select float

    emitter.label("__rt_snuk_int_x86_64");
    emitter.instruction("mov rax, 1");                                          // report PHP integer numeric-string kind
    emitter.instruction("jmp __rt_snuk_return_x86_64");                         // share frame restoration
    emitter.label("__rt_snuk_float_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the original string pointer for double parsing
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the original bounded string length
    emitter.instruction("call __rt_str_to_number");                             // parse the validated float-form numeric string
    abi::emit_load_int_immediate(emitter, "rax", 0x43e0000000000000);
    emitter.instruction("movq xmm1, rax");                                      // materialize positive 2^63 as the exclusive long bound
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the parsed double with PHP's positive integer bound
    emitter.instruction("jae __rt_snuk_float_out_of_range_x86_64");             // positive overflow cannot weakly coerce to int
    abi::emit_load_int_immediate(emitter, "rax", 0xc3e0000000000000u64 as i64);
    emitter.instruction("movq xmm1, rax");                                      // materialize negative 2^63 as the inclusive long bound
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the parsed double with PHP's negative integer bound
    emitter.instruction("jb __rt_snuk_float_out_of_range_x86_64");              // negative overflow cannot weakly coerce to int
    emitter.instruction("mov rax, 3");                                          // report a float form that an int-only hint may coerce
    emitter.instruction("jmp __rt_snuk_return_x86_64");                         // share frame restoration
    emitter.label("__rt_snuk_float_out_of_range_x86_64");
    emitter.instruction("mov rax, 2");                                          // report a float form outside PHP integer range
    emitter.label("__rt_snuk_return_x86_64");
    emitter.instruction("add rsp, 32");                                         // release classifier scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return one numeric-string kind from 0 through 3
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies the AArch64 classifier preserves signed bounds and the int-coercible float kind.
    #[test]
    fn aarch64_numeric_union_kind_emits_php_integer_boundaries() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_str_numeric_union_kind(&mut emitter);
        let assembly = emitter.output();

        assert!(assembly.contains("__rt_str_numeric_union_kind:\n"));
        assert!(assembly.contains("bl __rt_str_looks_like_int_for_coercion\n"));
        assert!(assembly.contains("b.ge __rt_snuk_float_out_of_range\n"));
        assert!(assembly.contains("b.lt __rt_snuk_float_out_of_range\n"));
        assert!(assembly.contains("mov x0, #3\n"));
    }

    /// Verifies the x86_64 classifier preserves signed bounds and the int-coercible float kind.
    #[test]
    fn x86_numeric_union_kind_emits_php_integer_boundaries() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_str_numeric_union_kind(&mut emitter);
        let assembly = emitter.output();

        assert!(assembly.contains("__rt_str_numeric_union_kind:\n"));
        assert!(assembly.contains("call __rt_str_looks_like_int_for_coercion\n"));
        assert!(assembly.contains("jae __rt_snuk_float_out_of_range_x86_64\n"));
        assert!(assembly.contains("jb __rt_snuk_float_out_of_range_x86_64\n"));
        assert!(assembly.contains("mov rax, 3\n"));
    }
}
