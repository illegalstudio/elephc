//! Purpose:
//! Emits `__rt_mixed_numeric_div`, PHP's `/` over two boxed Mixed operands, preserving
//! exact integer division at run time.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - Integer-form operands (ints, bools, null, int-form numeric strings) use the integer
//!   path: an exact quotient that fits stays an integer; a non-zero remainder or
//!   `PHP_INT_MIN / -1` promotes both operands to double.
//! - Any double payload or float-form numeric string uses the double path.
//! - A zero divisor returns a null pointer sentinel; the caller raises PHP's
//!   `DivisionByZeroError` after releasing its operand boxes. Valid boxed cells are never
//!   null, so the sentinel cannot collide with a result.
//! - The helper returns a fresh boxed Mixed cell and never aliases or retains an input.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the boxed Mixed division helper for the active target.
pub fn emit_mixed_numeric_div(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 helper.
fn emit_aarch64(emitter: &mut Emitter) {
    let float_path = "__rt_mixed_numeric_div_float";
    let check_right_string = "__rt_mixed_numeric_div_check_right_string";
    let int_path = "__rt_mixed_numeric_div_int";
    let int_checked = "__rt_mixed_numeric_div_int_checked";
    let int_quotient = "__rt_mixed_numeric_div_int_quotient";
    let float_from_int = "__rt_mixed_numeric_div_float_from_int";
    let box_float = "__rt_mixed_numeric_div_box_float";
    let div_zero = "__rt_mixed_numeric_div_zero";
    let done = "__rt_mixed_numeric_div_done";

    emitter.blank();
    emitter.comment("--- runtime: mixed_numeric_div ---");
    emitter.label_global("__rt_mixed_numeric_div");
    emitter.instruction("sub sp, sp, #80");                                     // allocate slots for operands, tags, casts, and saved FP state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed left operand pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed right operand pointer

    // -- classify both payloads: any double or float-form string uses the double path --
    emitter.instruction("bl __rt_mixed_unbox");                                 // inspect the left boxed payload tag
    emitter.instruction("str x0, [sp, #24]");                                   // save the left runtime value tag
    emitter.instruction("str x1, [sp, #16]");                                   // save the left integer payload for the fast path
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the boxed right operand
    emitter.instruction("bl __rt_mixed_unbox");                                 // inspect the right boxed payload tag
    emitter.instruction("str x0, [sp, #32]");                                   // save the right runtime value tag
    emitter.instruction("str x1, [sp, #56]");                                   // save the right integer payload for the fast path
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime value tag
    emitter.instruction("cmp x9, #2");                                          // does the left operand hold a double?
    emitter.instruction(&format!("b.eq {float_path}"));                         // a double forces the double path
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the right runtime value tag
    emitter.instruction("cmp x9, #2");                                          // does the right operand hold a double?
    emitter.instruction(&format!("b.eq {float_path}"));                         // a double forces the double path
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime value tag
    emitter.instruction("cmp x9, #1");                                          // does the left operand hold a string?
    emitter.instruction(&format!("b.ne {check_right_string}"));                 // left is not a string: inspect the right operand
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed left string
    emitter.instruction("bl __rt_mixed_str_operand_is_float");                  // classify the string as int-form or float-form
    emitter.instruction(&format!("cbnz x0, {float_path}"));                     // a float-form string forces the double path
    emitter.label(check_right_string);
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the right runtime value tag
    emitter.instruction("cmp x9, #1");                                          // does the right operand hold a string?
    emitter.instruction(&format!("b.ne {int_path}"));                           // right is not a string: use the integer path
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right string
    emitter.instruction("bl __rt_mixed_str_operand_is_float");                  // classify the string as int-form or float-form
    emitter.instruction(&format!("cbnz x0, {float_path}"));                     // a float-form string forces the double path

    // -- both payload tags already integer: skip the repeated unboxing casts --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime value tag
    emitter.instruction(&format!("cbnz x9, {int_path}"));                       // a non-integer payload needs the coercion path
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the right runtime value tag
    emitter.instruction(&format!("cbnz x9, {int_path}"));                       // a non-integer payload needs the coercion path
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = left integer payload
    emitter.instruction("ldr x2, [sp, #56]");                                   // x2 = right integer payload
    emitter.instruction(&format!("b {int_checked}"));                           // run the exact-quotient rule directly

    // -- integer path: cast both operands, then apply PHP's exact-quotient rule --
    emitter.label(int_path);
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed left operand
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the left operand to its integer form
    emitter.instruction("str x0, [sp, #40]");                                   // save the left integer payload
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right operand
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the right operand to its integer form
    emitter.instruction("mov x2, x0");                                          // x2 = right integer operand
    emitter.instruction("ldr x1, [sp, #40]");                                   // x1 = left integer operand

    emitter.label(int_checked);
    emitter.instruction(&format!("cbz x2, {div_zero}"));                        // a zero divisor returns the error sentinel
    abi::emit_load_int_immediate(emitter, "x9", i64::MIN);
    emitter.instruction("cmp x1, x9");                                          // is the dividend PHP_INT_MIN?
    emitter.instruction(&format!("b.ne {int_quotient}"));                       // every other dividend is safe
    emitter.instruction("cmp x2, #-1");                                         // does INT_MIN divide by -1?
    emitter.instruction(&format!("b.eq {float_from_int}"));                     // that quotient is not representable

    emitter.label(int_quotient);
    emitter.instruction("sdiv x3, x1, x2");                                     // truncate toward zero like PHP integer division
    emitter.instruction("msub x4, x3, x2, x1");                                 // remainder = dividend - quotient * divisor
    emitter.instruction(&format!("cbnz x4, {float_from_int}"));                 // a non-zero remainder yields a double
    emitter.instruction("mov x1, x3");                                          // integer payload for the boxed quotient
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_jump(emitter, done);

    emitter.label(float_from_int);
    emitter.instruction("scvtf d0, x1");                                        // convert the dividend to double
    emitter.instruction("scvtf d1, x2");                                        // convert the divisor to double
    emitter.instruction("fdiv d0, d0, d1");                                     // build PHP's promoted float quotient
    abi::emit_jump(emitter, box_float);

    // -- double path --
    emitter.label(float_path);
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // coerce the left operand to double
    emitter.instruction("str d0, [sp, #48]");                                   // save the left double across the right cast
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // coerce the right operand to double
    emitter.instruction("fmov d1, d0");                                         // d1 = right double operand
    emitter.instruction("ldr d0, [sp, #48]");                                   // d0 = left double operand
    emitter.instruction("fcmp d1, #0.0");                                       // is the divisor positive or negative zero?
    emitter.instruction(&format!("b.eq {div_zero}"));                           // zero divisors return the error sentinel
    emitter.instruction("fdiv d0, d0, d1");                                     // perform PHP floating-point division

    emitter.label(box_float);
    emitter.instruction("fmov x1, d0");                                         // move the double bits into the payload register
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    abi::emit_call_label(emitter, "__rt_mixed_from_value");

    emitter.label(done);
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the boxed result or the null error sentinel

    emitter.label(div_zero);
    emitter.instruction("mov x0, #0");                                          // null sentinel: the caller raises DivisionByZeroError
    abi::emit_jump(emitter, done);
}

/// Emits the x86_64 helper.
fn emit_x86_64(emitter: &mut Emitter) {
    let float_path = "__rt_mixed_numeric_div_float_x86";
    let check_right_string = "__rt_mixed_numeric_div_check_right_string_x86";
    let int_path = "__rt_mixed_numeric_div_int_x86";
    let int_checked = "__rt_mixed_numeric_div_int_checked_x86";
    let int_quotient = "__rt_mixed_numeric_div_int_quotient_x86";
    let float_from_int = "__rt_mixed_numeric_div_float_from_int_x86";
    let box_float = "__rt_mixed_numeric_div_box_float_x86";
    let div_zero = "__rt_mixed_numeric_div_zero_x86";
    let done = "__rt_mixed_numeric_div_done_x86";

    emitter.blank();
    emitter.comment("--- runtime: mixed_numeric_div ---");
    emitter.label_global("__rt_mixed_numeric_div");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 80");                                         // reserve aligned slots for operands, tags, and casts
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the boxed left operand pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the boxed right operand pointer

    // -- classify both payloads: any double or float-form string uses the double path --
    emitter.instruction("call __rt_mixed_unbox");                               // inspect the left boxed payload tag
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the left runtime value tag
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the left integer payload for the fast path
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the boxed right operand
    emitter.instruction("call __rt_mixed_unbox");                               // inspect the right boxed payload tag
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the right runtime value tag
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // save the right integer payload for the fast path
    emitter.instruction("cmp QWORD PTR [rbp - 24], 2");                         // does the left operand hold a double?
    emitter.instruction(&format!("je {float_path}"));                           // a double forces the double path
    emitter.instruction("cmp QWORD PTR [rbp - 32], 2");                         // does the right operand hold a double?
    emitter.instruction(&format!("je {float_path}"));                           // a double forces the double path
    emitter.instruction("cmp QWORD PTR [rbp - 24], 1");                         // does the left operand hold a string?
    emitter.instruction(&format!("jne {check_right_string}"));                  // left is not a string: inspect the right operand
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed left string
    emitter.instruction("call __rt_mixed_str_operand_is_float");                // classify the string as int-form or float-form
    emitter.instruction("test rax, rax");                                       // did the classifier report a float-form string?
    emitter.instruction(&format!("jnz {float_path}"));                          // a float-form string forces the double path
    emitter.label(check_right_string);
    emitter.instruction("cmp QWORD PTR [rbp - 32], 1");                         // does the right operand hold a string?
    emitter.instruction(&format!("jne {int_path}"));                            // right is not a string: use the integer path
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right string
    emitter.instruction("call __rt_mixed_str_operand_is_float");                // classify the string as int-form or float-form
    emitter.instruction("test rax, rax");                                       // did the classifier report a float-form string?
    emitter.instruction(&format!("jnz {float_path}"));                          // a float-form string forces the double path

    // -- both payload tags already integer: skip the repeated unboxing casts --
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // is the left payload already an integer?
    emitter.instruction(&format!("jne {int_path}"));                            // a non-integer payload needs the coercion path
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is the right payload already an integer?
    emitter.instruction(&format!("jne {int_path}"));                            // a non-integer payload needs the coercion path
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // r9 = left integer payload
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // rcx = right integer payload
    emitter.instruction(&format!("jmp {int_checked}"));                         // run the exact-quotient rule directly

    // -- integer path: cast both operands, then apply PHP's exact-quotient rule --
    emitter.label(int_path);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed left operand
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the left operand to its integer form
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the left integer payload
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right operand
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the right operand to its integer form
    emitter.instruction("mov rcx, rax");                                        // rcx = right integer operand
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // r9 = left integer operand

    emitter.label(int_checked);
    emitter.instruction("test rcx, rcx");                                       // is the divisor zero?
    emitter.instruction(&format!("jz {div_zero}"));                             // zero divisors return the error sentinel
    abi::emit_load_int_immediate(emitter, "r8", i64::MIN);
    emitter.instruction("cmp r9, r8");                                          // is the dividend INT64_MIN?
    emitter.instruction(&format!("jne {int_quotient}"));                        // every other dividend is safe
    emitter.instruction("cmp rcx, -1");                                         // does INT64_MIN divide by -1?
    emitter.instruction(&format!("je {float_from_int}"));                       // that quotient would trap the hardware divide

    emitter.label(int_quotient);
    emitter.instruction("mov rax, r9");                                         // stage the dividend for signed division
    emitter.instruction("cqo");                                                 // sign-extend the dividend before signed division
    emitter.instruction("idiv rcx");                                            // quotient in rax, remainder in rdx
    emitter.instruction("test rdx, rdx");                                       // is the division exact?
    emitter.instruction(&format!("jnz {float_from_int}"));                      // a non-zero remainder yields a double
    emitter.instruction("mov rdi, rax");                                        // integer payload for the boxed quotient
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed integer cell
    abi::emit_jump(emitter, done);

    emitter.label(float_from_int);
    emitter.instruction("cvtsi2sd xmm0, r9");                                   // convert the dividend to double
    emitter.instruction("cvtsi2sd xmm1, rcx");                                  // convert the divisor to double
    emitter.instruction("divsd xmm0, xmm1");                                    // build PHP's promoted float quotient
    abi::emit_jump(emitter, box_float);

    // -- double path --
    emitter.label(float_path);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed left operand
    emitter.instruction("call __rt_mixed_cast_float");                          // coerce the left operand to double
    emitter.instruction("movsd QWORD PTR [rbp - 48], xmm0");                    // save the left double across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right operand
    emitter.instruction("call __rt_mixed_cast_float");                          // coerce the right operand to double
    emitter.instruction("movsd xmm1, xmm0");                                    // xmm1 = right double operand
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 48]");                    // xmm0 = left double operand
    emitter.instruction("movq r10, xmm1");                                      // copy the divisor bits for the zero test
    emitter.instruction("add r10, r10");                                        // shift out the sign so -0.0 tests like +0.0
    emitter.instruction(&format!("jz {div_zero}"));                             // zero divisors return the error sentinel
    emitter.instruction("divsd xmm0, xmm1");                                    // perform PHP floating-point division

    emitter.label(box_float);
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into the payload word
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed float cell

    emitter.label(done);
    emitter.instruction("leave");                                               // restore the helper frame and return address
    emitter.instruction("ret");                                                 // return the boxed result or the null error sentinel

    emitter.label(div_zero);
    emitter.instruction("xor eax, eax");                                        // null sentinel: the caller raises DivisionByZeroError
    abi::emit_jump(emitter, done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits both the exact-integer and double paths, plus the
    /// zero-divisor sentinel.
    #[test]
    fn mixed_numeric_div_emits_all_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_mixed_numeric_div(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_mixed_numeric_div"), "{target}");
            assert!(asm.contains("__rt_mixed_cast_int"), "{target}");
            assert!(asm.contains("__rt_mixed_cast_float"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("idiv rcx"), "{target}");
                assert!(asm.contains("divsd xmm0, xmm1"), "{target}");
            } else {
                assert!(asm.contains("sdiv x3, x1, x2"), "{target}");
                assert!(asm.contains("fdiv d0, d0, d1"), "{target}");
            }
        }
    }
}
