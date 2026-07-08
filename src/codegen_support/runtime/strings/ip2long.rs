//! Purpose:
//! Emits the `__rt_ip2long` runtime helper assembly for the ip2long builtin.
//! Parses a decimal dotted-quad IPv4 string into its 32-bit integer value.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Returns the address as a non-negative integer, or -1 when the string is
//!   not a valid `A.B.C.D` form with each octet in 0..=255.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// ip2long: parse a dotted-quad IPv4 string into a 32-bit integer.
/// Input:  x0 = string pointer, x1 = string length
/// Output: x0 = IP integer (0..=0xFFFFFFFF), or -1 when the string is invalid
pub fn emit_ip2long(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ip2long_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ip2long ---");
    emitter.label_global("__rt_ip2long");

    emitter.instruction("mov x2, #0");                                          // x2 = accumulated 32-bit result
    emitter.instruction("mov x3, #0");                                          // x3 = scan position
    emitter.instruction("mov x4, #0");                                          // x4 = octet index 0..3

    emitter.label("__rt_ip2long_octet");
    emitter.instruction("mov x5, #0");                                          // x5 = current octet value
    emitter.instruction("mov x6, #0");                                          // x6 = digit count for this octet

    emitter.label("__rt_ip2long_digit");
    emitter.instruction("cmp x3, x1");                                          // reached the end of the string?
    emitter.instruction("b.ge __rt_ip2long_digit_end");                         // stop the digit scan at end of string
    emitter.instruction("ldrb w7, [x0, x3]");                                   // load the current character
    emitter.instruction("cmp w7, #48");                                         // below ASCII '0'?
    emitter.instruction("b.lt __rt_ip2long_digit_end");                         // non-digit ends this octet
    emitter.instruction("cmp w7, #57");                                         // above ASCII '9'?
    emitter.instruction("b.gt __rt_ip2long_digit_end");                         // non-digit ends this octet
    emitter.instruction("sub w7, w7, #48");                                     // convert the digit to its value
    emitter.instruction("mov x8, #10");                                         // decimal base
    emitter.instruction("mul x5, x5, x8");                                      // shift the octet value one decimal place
    emitter.instruction("add x5, x5, x7");                                      // add the new digit
    emitter.instruction("add x6, x6, #1");                                      // count the digit
    emitter.instruction("add x3, x3, #1");                                      // advance the scan position
    emitter.instruction("cmp x6, #3");                                          // more than three digits?
    emitter.instruction("b.gt __rt_ip2long_fail");                              // an octet has at most three digits
    emitter.instruction("b __rt_ip2long_digit");                                // scan the next digit

    emitter.label("__rt_ip2long_digit_end");
    emitter.instruction("cbz x6, __rt_ip2long_fail");                           // an octet must have at least one digit
    emitter.instruction("cmp x5, #255");                                        // octet larger than 255?
    emitter.instruction("b.gt __rt_ip2long_fail");                              // each octet must fit in a byte
    emitter.instruction("lsl x2, x2, #8");                                      // make room for the next octet
    emitter.instruction("orr x2, x2, x5");                                      // place the octet in the low byte
    emitter.instruction("cmp x4, #3");                                          // is this the final octet?
    emitter.instruction("b.eq __rt_ip2long_last");                              // the fourth octet has no separator

    emitter.instruction("cmp x3, x1");                                          // is there a character for the separator?
    emitter.instruction("b.ge __rt_ip2long_fail");                              // missing dot separator
    emitter.instruction("ldrb w7, [x0, x3]");                                   // load the expected separator
    emitter.instruction("cmp w7, #46");                                         // is it an ASCII '.'?
    emitter.instruction("b.ne __rt_ip2long_fail");                              // octets must be dot-separated
    emitter.instruction("add x3, x3, #1");                                      // consume the dot
    emitter.instruction("add x4, x4, #1");                                      // move to the next octet
    emitter.instruction("b __rt_ip2long_octet");                                // parse the next octet

    emitter.label("__rt_ip2long_last");
    emitter.instruction("cmp x3, x1");                                          // any trailing characters?
    emitter.instruction("b.ne __rt_ip2long_fail");                              // trailing characters are invalid
    emitter.instruction("mov x0, x2");                                          // return the parsed 32-bit address
    emitter.instruction("ret");                                                 // success

    emitter.label("__rt_ip2long_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 signals an invalid address to the caller
    emitter.instruction("ret");                                                 // failure
}

/// Emits the Linux x86_64 string runtime helper for ip2long.
fn emit_ip2long_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ip2long ---");
    emitter.label_global("__rt_ip2long");

    emitter.instruction("xor eax, eax");                                        // rax = accumulated 32-bit result
    emitter.instruction("xor ecx, ecx");                                        // rcx = scan position
    emitter.instruction("xor edx, edx");                                        // rdx = octet index 0..3

    emitter.label("__rt_ip2long_octet_x86");
    emitter.instruction("xor r8d, r8d");                                        // r8 = current octet value
    emitter.instruction("xor r9d, r9d");                                        // r9 = digit count for this octet

    emitter.label("__rt_ip2long_digit_x86");
    emitter.instruction("cmp rcx, rsi");                                        // reached the end of the string?
    emitter.instruction("jge __rt_ip2long_digit_end_x86");                      // stop the digit scan at end of string
    emitter.instruction("movzx r10d, BYTE PTR [rdi + rcx]");                    // load the current character
    emitter.instruction("cmp r10d, 48");                                        // below ASCII '0'?
    emitter.instruction("jl __rt_ip2long_digit_end_x86");                       // non-digit ends this octet
    emitter.instruction("cmp r10d, 57");                                        // above ASCII '9'?
    emitter.instruction("jg __rt_ip2long_digit_end_x86");                       // non-digit ends this octet
    emitter.instruction("sub r10d, 48");                                        // convert the digit to its value
    emitter.instruction("imul r8, r8, 10");                                     // shift the octet value one decimal place
    emitter.instruction("add r8, r10");                                         // add the new digit
    emitter.instruction("inc r9");                                              // count the digit
    emitter.instruction("inc rcx");                                             // advance the scan position
    emitter.instruction("cmp r9, 3");                                           // more than three digits?
    emitter.instruction("jg __rt_ip2long_fail_x86");                            // an octet has at most three digits
    emitter.instruction("jmp __rt_ip2long_digit_x86");                          // scan the next digit

    emitter.label("__rt_ip2long_digit_end_x86");
    emitter.instruction("test r9, r9");                                         // did this octet have any digits?
    emitter.instruction("jz __rt_ip2long_fail_x86");                            // an octet must have at least one digit
    emitter.instruction("cmp r8, 255");                                         // octet larger than 255?
    emitter.instruction("jg __rt_ip2long_fail_x86");                            // each octet must fit in a byte
    emitter.instruction("shl rax, 8");                                          // make room for the next octet
    emitter.instruction("or rax, r8");                                          // place the octet in the low byte
    emitter.instruction("cmp rdx, 3");                                          // is this the final octet?
    emitter.instruction("je __rt_ip2long_last_x86");                            // the fourth octet has no separator

    emitter.instruction("cmp rcx, rsi");                                        // is there a character for the separator?
    emitter.instruction("jge __rt_ip2long_fail_x86");                           // missing dot separator
    emitter.instruction("movzx r10d, BYTE PTR [rdi + rcx]");                    // load the expected separator
    emitter.instruction("cmp r10d, 46");                                        // is it an ASCII '.'?
    emitter.instruction("jne __rt_ip2long_fail_x86");                           // octets must be dot-separated
    emitter.instruction("inc rcx");                                             // consume the dot
    emitter.instruction("inc rdx");                                             // move to the next octet
    emitter.instruction("jmp __rt_ip2long_octet_x86");                          // parse the next octet

    emitter.label("__rt_ip2long_last_x86");
    emitter.instruction("cmp rcx, rsi");                                        // any trailing characters?
    emitter.instruction("jne __rt_ip2long_fail_x86");                           // trailing characters are invalid
    emitter.instruction("ret");                                                 // success: the result is already in rax

    emitter.label("__rt_ip2long_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 signals an invalid address to the caller
    emitter.instruction("ret");                                                 // failure
}
