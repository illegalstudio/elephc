//! Purpose:
//! Emits `__rt_array_map_sum_int_div`, the fused scalar kernel for a stored
//! `array_map(fn($v) => $v / C, $packedInts)` whose only observer is `array_sum()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - The kernel walks the packed int payload exactly once and never materializes the
//!   intermediate mapped array. PHP's `/` keeps an integer when both operands are integers
//!   and the division is exact, otherwise it returns a double; `PHP_INT_MIN / -1` promotes.
//! - The running sum follows PHP's left-to-right `array_sum()`: integer additions stay integer
//!   until one overflows, then every later term is added as a double. The first promoted
//!   quotient converts the accumulated integer prefix and switches the accumulator to double.
//! - The divisor is a nonzero integer literal proven by the AST stored-map shape, so a zero
//!   divisor keeps the generic map and its `DivisionByZeroError`.
//! - The result is one owned Mixed cell (`int` or `float`), exactly like `array_sum()` over the
//!   boxed map. The empty source returns the integer zero.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the fused int-division map+sum kernel for the active target.
pub fn emit_array_map_sum_int_div(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 fused kernel.
fn emit_aarch64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_sum_int_div_loop";
    let divide = "__rt_array_map_sum_int_div_divide";
    let float_path = "__rt_array_map_sum_int_div_float";
    let int_as_float = "__rt_array_map_sum_int_div_int_float";
    let acc_overflow = "__rt_array_map_sum_int_div_acc_overflow";
    let float_add = "__rt_array_map_sum_int_div_float_add";
    let next = "__rt_array_map_sum_int_div_next";
    let done = "__rt_array_map_sum_int_div_done";
    let box_float = "__rt_array_map_sum_int_div_box_float";

    emitter.blank();
    emitter.comment("--- runtime: array_map_sum_int_div (fused stored map and sum) ---");
    emitter.label_global("__rt_array_map_sum_int_div");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish a stable helper frame
    emitter.instruction("stp x19, x20, [sp, #-16]!");                           // preserve the packed source and literal divisor
    emitter.instruction("stp x21, x22, [sp, #-16]!");                           // preserve the remaining count and payload cursor
    emitter.instruction("stp x23, x24, [sp, #-16]!");                           // preserve the integer accumulator and float-mode flag
    emitter.instruction("mov x19, x0");                                         // x19 = packed int source array
    emitter.instruction("mov x20, x1");                                         // x20 = nonzero integer divisor
    emitter.instruction("ldr x21, [x19]");                                      // x21 = remaining source element count
    emitter.instruction("add x9, x19, #24");                                    // x9 = source payload cursor
    emitter.instruction("mov x23, xzr");                                        // start the integer accumulator at zero
    emitter.instruction("mov x24, xzr");                                        // float mode is off until a quotient or sum promotes
    abi::emit_load_int_immediate(emitter, "x17", i64::MIN);

    emitter.label(loop_label);
    emitter.instruction(&format!("cbz x21, {done}"));                           // stop once every element has been divided and summed
    emitter.instruction("ldr x10, [x9], #8");                                   // load the current source integer
    emitter.instruction("sub x21, x21, #1");                                    // account for the consumed element
    emitter.instruction("cmp x20, #-1");                                        // only a -1 divisor can overflow INT_MIN
    emitter.instruction(&format!("b.ne {divide}"));                             // every other divisor is safe to divide
    emitter.instruction("cmp x10, x17");                                        // is the element PHP_INT_MIN?
    emitter.instruction(&format!("b.eq {float_path}"));                         // INT_MIN / -1 promotes to double

    emitter.label(divide);
    emitter.instruction("sdiv x11, x10, x20");                                  // truncate toward zero like PHP integer division
    emitter.instruction("msub x12, x11, x20, x10");                             // remainder = element - quotient * divisor
    emitter.instruction(&format!("cbnz x12, {float_path}"));                    // a non-zero remainder yields a double
    emitter.instruction(&format!("cbnz x24, {int_as_float}"));                  // the accumulator is already a double
    emitter.instruction("adds x13, x23, x11");                                  // sum the exact quotient with PHP's checked integer addition
    emitter.instruction(&format!("b.vs {acc_overflow}"));                       // overflow promotes the running sum to a double
    emitter.instruction("mov x23, x13");                                        // commit the still-integer running sum
    emitter.instruction(&format!("b {next}"));                                  // continue with the next element

    emitter.label(acc_overflow);
    emitter.instruction("scvtf d0, x23");                                       // convert the integer prefix to a double
    emitter.instruction("scvtf d1, x11");                                       // convert the quotient to a double
    emitter.instruction("fadd d0, d0, d1");                                     // add the overflowed term exactly as PHP does
    emitter.instruction("mov x24, #1");                                         // the running sum is now a double
    emitter.instruction(&format!("b {next}"));                                  // continue with the next element

    emitter.label(int_as_float);
    emitter.instruction("scvtf d1, x11");                                       // convert the exact quotient to a double
    emitter.instruction("fadd d0, d0, d1");                                     // add it to the promoted running sum
    emitter.instruction(&format!("b {next}"));                                  // continue with the next element

    emitter.label(float_path);
    emitter.instruction("scvtf d1, x10");                                       // convert the dividend to a double
    emitter.instruction("scvtf d2, x20");                                       // convert the divisor to a double
    emitter.instruction("fdiv d1, d1, d2");                                     // build PHP's promoted float quotient
    emitter.instruction(&format!("cbnz x24, {float_add}"));                     // the accumulator is already a double
    emitter.instruction("scvtf d0, x23");                                       // convert the integer prefix to a double
    emitter.instruction("mov x24, #1");                                         // the running sum is now a double

    emitter.label(float_add);
    emitter.instruction("fadd d0, d0, d1");                                     // add the promoted quotient in source order
    emitter.instruction(&format!("b {next}"));                                  // continue with the next element

    emitter.label(next);
    emitter.instruction(&format!("b {loop_label}"));                           // resume the fused walk

    emitter.label(done);
    emitter.instruction(&format!("cbnz x24, {box_float}"));                     // a promoted sum boxes its float bits
    emitter.instruction("mov x0, #0");                                          // Mixed tag 0 = integer
    emitter.instruction("mov x1, x23");                                         // integer payload
    emitter.instruction("mov x2, xzr");                                         // scalar payloads do not use a second word
    emitter.instruction("b __rt_array_map_sum_int_div_epilogue");               // share the return sequence

    emitter.label(box_float);
    emitter.instruction("mov x0, #2");                                          // Mixed tag 2 = float
    emitter.instruction("fmov x1, d0");                                         // float bits as the payload word
    emitter.instruction("mov x2, xzr");                                         // scalar payloads do not use a second word
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the owned result cell
    emitter.instruction("b __rt_array_map_sum_int_div_return");                 // restore the frame and return

    emitter.label("__rt_array_map_sum_int_div_epilogue");
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the owned result cell

    emitter.label("__rt_array_map_sum_int_div_return");
    emitter.instruction("ldp x23, x24, [sp], #16");                             // restore the integer accumulator and float-mode flag
    emitter.instruction("ldp x21, x22, [sp], #16");                             // restore the remaining count and payload cursor
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the packed source and divisor
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the owned Mixed sum
}

/// Emits the x86_64 fused kernel.
fn emit_x86_64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_sum_int_div_loop_x86";
    let divide = "__rt_array_map_sum_int_div_divide_x86";
    let float_path = "__rt_array_map_sum_int_div_float_x86";
    let int_as_float = "__rt_array_map_sum_int_div_int_float_x86";
    let acc_overflow = "__rt_array_map_sum_int_div_acc_overflow_x86";
    let float_add = "__rt_array_map_sum_int_div_float_add_x86";
    let next = "__rt_array_map_sum_int_div_next_x86";
    let done = "__rt_array_map_sum_int_div_done_x86";
    let box_float = "__rt_array_map_sum_int_div_box_float_x86";
    let epilogue = "__rt_array_map_sum_int_div_epilogue_x86";

    emitter.blank();
    emitter.comment("--- runtime: array_map_sum_int_div (fused stored map and sum) ---");
    emitter.label_global("__rt_array_map_sum_int_div");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("push r12");                                            // preserve the packed source array
    emitter.instruction("push r13");                                            // preserve the integer divisor
    emitter.instruction("push r14");                                            // preserve the remaining element count
    emitter.instruction("push r15");                                            // preserve the payload cursor
    emitter.instruction("push rbx");                                            // preserve the integer accumulator
    emitter.instruction("sub rsp, 8");                                          // keep the stack aligned for runtime calls
    emitter.instruction("mov r12, rdi");                                        // r12 = packed int source array
    emitter.instruction("mov r13, rsi");                                        // r13 = nonzero integer divisor
    emitter.instruction("mov r14, QWORD PTR [r12]");                            // r14 = remaining source element count
    emitter.instruction("lea r15, [r12 + 24]");                                 // r15 = source payload cursor
    emitter.instruction("xor ebx, ebx");                                        // start the integer accumulator at zero
    emitter.instruction("xor ebp, ebp");                                        // float mode is off until a quotient or sum promotes
    abi::emit_load_int_immediate(emitter, "r8", i64::MIN);

    emitter.label(loop_label);
    emitter.instruction("test r14, r14");                                       // stop once every element has been divided and summed
    emitter.instruction(&format!("jz {done}"));                                 // the payload walk is complete
    emitter.instruction("mov r11, QWORD PTR [r15]");                            // load the current source integer
    emitter.instruction("add r15, 8");                                          // advance the payload cursor
    emitter.instruction("sub r14, 1");                                          // account for the consumed element
    emitter.instruction("cmp r13, -1");                                         // only a -1 divisor can overflow INT64_MIN
    emitter.instruction(&format!("jne {divide}"));                              // every other divisor is safe to divide
    emitter.instruction("cmp r11, r8");                                         // is the element INT64_MIN?
    emitter.instruction(&format!("je {float_path}"));                           // INT64_MIN / -1 promotes to double

    emitter.label(divide);
    emitter.instruction("mov rax, r11");                                        // stage the dividend for signed division
    emitter.instruction("cqo");                                                 // sign-extend the dividend before signed division
    emitter.instruction("idiv r13");                                            // quotient in rax, remainder in rdx
    emitter.instruction("test rdx, rdx");                                       // is the division exact?
    emitter.instruction(&format!("jnz {float_path}"));                          // a non-zero remainder yields a double
    emitter.instruction("test rbp, rbp");                                       // is the accumulator already a double?
    emitter.instruction(&format!("jnz {int_as_float}"));                        // then add this exact quotient as a double
    emitter.instruction("mov rcx, rbx");                                        // copy the integer accumulator before adding
    emitter.instruction("add rcx, rax");                                        // sum the exact quotient with PHP's checked addition
    emitter.instruction(&format!("jo {acc_overflow}"));                         // overflow promotes the running sum to a double
    emitter.instruction("mov rbx, rcx");                                        // commit the still-integer running sum
    emitter.instruction(&format!("jmp {next}"));                                // continue with the next element

    emitter.label(acc_overflow);
    emitter.instruction("cvtsi2sd xmm0, rbx");                                  // convert the integer prefix to a double
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert the quotient to a double
    emitter.instruction("addsd xmm0, xmm1");                                    // add the overflowed term exactly as PHP does
    emitter.instruction("mov ebp, 1");                                          // the running sum is now a double
    emitter.instruction(&format!("jmp {next}"));                                // continue with the next element

    emitter.label(int_as_float);
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert the exact quotient to a double
    emitter.instruction("addsd xmm0, xmm1");                                    // add it to the promoted running sum
    emitter.instruction(&format!("jmp {next}"));                                // continue with the next element

    emitter.label(float_path);
    emitter.instruction("cvtsi2sd xmm1, r11");                                  // convert the dividend to a double
    emitter.instruction("cvtsi2sd xmm2, r13");                                  // convert the divisor to a double
    emitter.instruction("divsd xmm1, xmm2");                                    // build PHP's promoted float quotient
    emitter.instruction("test rbp, rbp");                                       // is the accumulator already a double?
    emitter.instruction(&format!("jnz {float_add}"));                           // then add the promoted quotient directly
    emitter.instruction("cvtsi2sd xmm0, rbx");                                  // convert the integer prefix to a double
    emitter.instruction("mov ebp, 1");                                          // the running sum is now a double

    emitter.label(float_add);
    emitter.instruction("addsd xmm0, xmm1");                                    // add the promoted quotient in source order
    emitter.instruction(&format!("jmp {next}"));                                // continue with the next element

    emitter.label(next);
    emitter.instruction(&format!("jmp {loop_label}"));                          // resume the fused walk

    emitter.label(done);
    emitter.instruction("test rbp, rbp");                                       // is the finished sum a double?
    emitter.instruction(&format!("jnz {box_float}"));                           // a promoted sum boxes its float bits
    emitter.instruction("xor eax, eax");                                        // Mixed tag 0 = integer
    emitter.instruction("mov rdi, rbx");                                        // integer payload
    emitter.instruction("xor esi, esi");                                        // scalar payloads do not use a second word
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the owned result cell
    emitter.instruction(&format!("jmp {epilogue}"));                            // restore the frame and return

    emitter.label(box_float);
    emitter.instruction("mov eax, 2");                                          // Mixed tag 2 = float
    emitter.instruction("movq rdi, xmm0");                                      // float bits as the payload word
    emitter.instruction("xor esi, esi");                                        // scalar payloads do not use a second word
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the owned result cell

    emitter.label(epilogue);
    emitter.instruction("add rsp, 8");                                          // release the alignment slot
    emitter.instruction("pop rbx");                                             // restore the integer accumulator
    emitter.instruction("pop r15");                                             // restore the payload cursor
    emitter.instruction("pop r14");                                             // restore the remaining element count
    emitter.instruction("pop r13");                                             // restore the integer divisor
    emitter.instruction("pop r12");                                             // restore the packed source array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits the single-pass division and sum sequence.
    #[test]
    fn array_map_sum_int_div_emits_all_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_map_sum_int_div(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_array_map_sum_int_div"), "{target}");
            assert!(asm.contains("__rt_mixed_from_value"), "{target}");
            assert!(!asm.contains("__rt_array_new"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("idiv r13"), "{target}");
                assert!(asm.contains("divsd xmm1, xmm2"), "{target}");
                assert!(asm.contains("jo __rt_array_map_sum_int_div_acc_overflow_x86"), "{target}");
            } else {
                assert!(asm.contains("sdiv x11, x10, x20"), "{target}");
                assert!(asm.contains("fdiv d1, d1, d2"), "{target}");
                assert!(asm.contains("b.vs __rt_array_map_sum_int_div_acc_overflow"), "{target}");
            }
        }
    }
}
