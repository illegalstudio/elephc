//! Purpose:
//! Emits `__rt_array_map_int_div`, the stored-result kernel for
//! `array_map(fn($v) => $v / C, $packedInts)` with a nonzero integer literal divisor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - PHP keeps an integer when both operands are integers and the division is exact;
//!   otherwise `/` returns a double. The first pass stores raw 8-byte integer quotients
//!   while every element divides exactly and the quotient fits.
//! - The first non-exact element (or `PHP_INT_MIN / -1`) releases the partial raw
//!   `array<int>` and rebuilds an ordinary boxed Mixed array with the exact per-element
//!   int/float result.
//! - The divisor is a nonzero literal proven by the AST shape, so no zero-divisor guard is
//!   needed; a zero divisor keeps the generic map and its `DivisionByZeroError`.
//! - Both arrays are ordinary copy-on-write heap containers created by `__rt_array_new`;
//!   callers own exactly the returned reference. The callback is pure, so rebuilding cannot
//!   repeat an observable effect.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the stored integer-division map kernel for the active target.
pub fn emit_array_map_int_div(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 kernel.
fn emit_aarch64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_int_div_loop";
    let raw_divide = "__rt_array_map_int_div_raw_divide";
    let overflow_label = "__rt_array_map_int_div_overflow";
    let mixed_loop = "__rt_array_map_int_div_mixed_loop";
    let mixed_divide = "__rt_array_map_int_div_mixed_divide";
    let mixed_float = "__rt_array_map_int_div_mixed_float";
    let mixed_store = "__rt_array_map_int_div_mixed_store";
    let mixed_done = "__rt_array_map_int_div_mixed_done";
    let done = "__rt_array_map_int_div_done";

    emitter.blank();
    emitter.comment("--- runtime: array_map_int_div (stored int division) ---");
    emitter.label_global("__rt_array_map_int_div");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish a stable helper frame
    emitter.instruction("stp x19, x20, [sp, #-16]!");                           // preserve the source and literal divisor
    emitter.instruction("stp x21, x22, [sp, #-16]!");                           // preserve the element count and index
    emitter.instruction("stp x23, x24, [sp, #-16]!");                           // preserve the result array
    emitter.instruction("mov x19, x0");                                         // x19 = packed source array
    emitter.instruction("mov x20, x1");                                         // x20 = nonzero integer divisor
    emitter.instruction("ldr x21, [x19]");                                      // x21 = source element count
    emitter.instruction("mov x0, x21");                                         // allocate exactly one slot per source element
    emitter.instruction("mov x1, #8");                                          // raw integer slots are pointer-sized
    emitter.instruction("bl __rt_array_new");                                   // allocate the copy-on-write destination container
    emitter.instruction("mov x23, x0");                                         // x23 = raw int destination array
    emitter.instruction("mov x22, xzr");                                        // x22 = current element index

    emitter.label(loop_label);
    emitter.instruction("cmp x22, x21");                                        // has every element been mapped?
    emitter.instruction(&format!("b.ge {done}"));                               // finish once the loop exhausts the source
    emitter.instruction("add x9, x19, #24");                                    // skip the source header to the payload base
    emitter.instruction("ldr x10, [x9, x22, lsl #3]");                          // load the current source integer
    emitter.instruction("cmp x20, #-1");                                        // only a -1 divisor can overflow INT_MIN
    emitter.instruction(&format!("b.ne {raw_divide}"));                         // every other divisor is safe to divide
    abi::emit_load_int_immediate(emitter, "x9", i64::MIN);
    emitter.instruction("cmp x10, x9");                                         // is the element PHP_INT_MIN?
    emitter.instruction(&format!("b.eq {overflow_label}"));                     // INT_MIN / -1 promotes to double

    emitter.label(raw_divide);
    emitter.instruction("sdiv x11, x10, x20");                                  // truncate toward zero like PHP integer division
    emitter.instruction("msub x12, x11, x20, x10");                             // remainder = element - quotient * divisor
    emitter.instruction(&format!("cbnz x12, {overflow_label}"));                // a non-zero remainder yields a double
    emitter.instruction("add x9, x23, #24");                                    // skip the destination header to the payload base
    emitter.instruction("str x11, [x9, x22, lsl #3]");                          // store the exact raw integer quotient
    emitter.instruction("add x22, x22, #1");                                    // advance to the next source element
    emitter.instruction(&format!("b {loop_label}"));                            // continue until every element is mapped

    emitter.label(done);
    emitter.instruction("str x21, [x23]");                                      // publish the completed logical length
    emitter.instruction("mov x0, x23");                                         // return the raw int array
    emitter.instruction("ldp x23, x24, [sp], #16");                             // restore the result array
    emitter.instruction("ldp x21, x22, [sp], #16");                             // restore the element count and index
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the source and divisor
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the stored map result

    emitter.label(overflow_label);
    emitter.instruction("mov x0, x23");                                         // pass the partial raw array for release
    emitter.instruction("bl __rt_decref_array");                                // retire every slot built before the promoting element
    emitter.instruction("mov x0, x21");                                         // allocate one Mixed slot per source element
    emitter.instruction("mov x1, #8");                                          // boxed Mixed slots are pointer-sized
    emitter.instruction("bl __rt_array_new");                                   // allocate the overflow destination container
    emitter.instruction("mov x23, x0");                                         // x23 = boxed Mixed destination array
    emitter.instruction("ldr x9, [x23, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("mov x10, #0x80ff");                                    // preserve heap kind and copy-on-write metadata
    emitter.instruction("and x9, x9, x10");                                     // clear the raw-int value_type bits
    emitter.instruction("mov x10, #7");                                         // value_type 7 = boxed Mixed slots
    emitter.instruction("lsl x10, x10, #8");                                    // move the Mixed tag into the packed kind lane
    emitter.instruction("orr x9, x9, x10");                                     // combine stable metadata with the Mixed tag
    emitter.instruction("str x9, [x23, #-8]");                                  // publish the Mixed indexed-array metadata
    emitter.instruction("mov x22, xzr");                                        // restart at the first source element

    emitter.label(mixed_loop);
    emitter.instruction("cmp x22, x21");                                        // has every element been rebuilt?
    emitter.instruction(&format!("b.ge {mixed_done}"));                         // finish once the Mixed array is complete
    emitter.instruction("add x9, x19, #24");                                    // skip the source header to the payload base
    emitter.instruction("ldr x10, [x9, x22, lsl #3]");                          // load the current source integer
    emitter.instruction("cmp x20, #-1");                                        // only a -1 divisor can overflow INT_MIN
    emitter.instruction(&format!("b.ne {mixed_divide}"));                       // every other divisor is safe to divide
    abi::emit_load_int_immediate(emitter, "x9", i64::MIN);
    emitter.instruction("cmp x10, x9");                                         // is the element PHP_INT_MIN?
    emitter.instruction(&format!("b.eq {mixed_float}"));                        // INT_MIN / -1 is a promoted double

    emitter.label(mixed_divide);
    emitter.instruction("sdiv x11, x10, x20");                                  // truncate toward zero like PHP integer division
    emitter.instruction("msub x12, x11, x20, x10");                             // remainder = element - quotient * divisor
    emitter.instruction(&format!("cbnz x12, {mixed_float}"));                   // a non-zero remainder yields a double
    emitter.instruction("mov x0, #0");                                          // Mixed tag 0 = integer
    emitter.instruction("mov x1, x11");                                         // integer payload
    emitter.instruction("mov x2, xzr");                                         // scalar payloads do not use a second word
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed integer cell
    emitter.instruction(&format!("b {mixed_store}"));                           // share the destination store

    emitter.label(mixed_float);
    emitter.instruction("scvtf d0, x10");                                       // convert the source integer to double
    emitter.instruction("scvtf d1, x20");                                       // convert the literal divisor to double
    emitter.instruction("fdiv d0, d0, d1");                                     // build PHP's promoted float quotient
    emitter.instruction("mov x0, #2");                                          // Mixed tag 2 = float
    emitter.instruction("fmov x1, d0");                                         // float bits as the payload word
    emitter.instruction("mov x2, xzr");                                         // scalar payloads do not use a second word
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed float cell

    emitter.label(mixed_store);
    emitter.instruction("add x9, x23, #24");                                    // skip the Mixed destination header to the payload base
    emitter.instruction("str x0, [x9, x22, lsl #3]");                           // store the owned Mixed cell pointer
    emitter.instruction("add x22, x22, #1");                                    // advance to the next source element
    emitter.instruction(&format!("b {mixed_loop}"));                            // continue until every element is boxed

    emitter.label(mixed_done);
    emitter.instruction("str x21, [x23]");                                      // publish the completed logical length
    emitter.instruction("mov x0, x23");                                         // return the boxed Mixed array
    emitter.instruction("ldp x23, x24, [sp], #16");                             // restore the result array
    emitter.instruction("ldp x21, x22, [sp], #16");                             // restore the element count and index
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the source and divisor
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the stored map result
}

/// Emits the x86_64 kernel.
fn emit_x86_64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_int_div_loop_x86";
    let raw_divide = "__rt_array_map_int_div_raw_divide_x86";
    let overflow_label = "__rt_array_map_int_div_overflow_x86";
    let mixed_loop = "__rt_array_map_int_div_mixed_loop_x86";
    let mixed_divide = "__rt_array_map_int_div_mixed_divide_x86";
    let mixed_float = "__rt_array_map_int_div_mixed_float_x86";
    let mixed_store = "__rt_array_map_int_div_mixed_store_x86";
    let mixed_done = "__rt_array_map_int_div_mixed_done_x86";
    let done = "__rt_array_map_int_div_done_x86";

    emitter.blank();
    emitter.comment("--- runtime: array_map_int_div (stored int division) ---");
    emitter.label_global("__rt_array_map_int_div");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("push r12");                                            // preserve the packed source array
    emitter.instruction("push r13");                                            // preserve the integer divisor
    emitter.instruction("push r14");                                            // preserve the source element count
    emitter.instruction("push r15");                                            // preserve the element index
    emitter.instruction("push rbx");                                            // preserve the result array pointer
    emitter.instruction("sub rsp, 8");                                          // keep the stack aligned for runtime calls
    emitter.instruction("mov r12, rdi");                                        // r12 = packed source array
    emitter.instruction("mov r13, rsi");                                        // r13 = nonzero integer divisor
    emitter.instruction("mov r14, QWORD PTR [r12]");                            // r14 = source element count
    emitter.instruction("mov rdi, r14");                                        // allocate exactly one slot per source element
    emitter.instruction("mov rsi, 8");                                          // raw integer slots are pointer-sized
    emitter.instruction("call __rt_array_new");                                 // allocate the copy-on-write destination container
    emitter.instruction("mov rbx, rax");                                        // rbx = raw int destination array
    emitter.instruction("xor r15d, r15d");                                      // initialize the element index to zero
    abi::emit_load_int_immediate(emitter, "r8", i64::MIN);

    emitter.label(loop_label);
    emitter.instruction("cmp r15, r14");                                        // has every element been mapped?
    emitter.instruction(&format!("jge {done}"));                                // finish once the loop exhausts the source
    emitter.instruction("lea r10, [r12 + 24]");                                 // skip the source header to the payload base
    emitter.instruction("mov r11, QWORD PTR [r10 + r15 * 8]");                  // load the current source integer
    emitter.instruction("cmp r13, -1");                                         // only a -1 divisor can overflow INT64_MIN
    emitter.instruction(&format!("jne {raw_divide}"));                          // every other divisor is safe to divide
    emitter.instruction("cmp r11, r8");                                         // is the element INT64_MIN?
    emitter.instruction(&format!("je {overflow_label}"));                       // INT64_MIN / -1 promotes to double

    emitter.label(raw_divide);
    emitter.instruction("mov rax, r11");                                        // stage the dividend for signed division
    emitter.instruction("cqo");                                                 // sign-extend the dividend before signed division
    emitter.instruction("idiv r13");                                            // quotient in rax, remainder in rdx
    emitter.instruction("test rdx, rdx");                                       // is the division exact?
    emitter.instruction(&format!("jnz {overflow_label}"));                      // a non-zero remainder yields a double
    emitter.instruction("lea r10, [rbx + 24]");                                 // skip the destination header to the payload base
    emitter.instruction("mov QWORD PTR [r10 + r15 * 8], rax");                  // store the exact raw integer quotient
    emitter.instruction("add r15, 1");                                          // advance to the next source element
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue until every element is mapped

    emitter.label(done);
    emitter.instruction("mov QWORD PTR [rbx], r14");                            // publish the completed logical length
    emitter.instruction("mov rax, rbx");                                        // return the raw int array
    emitter.instruction("add rsp, 8");                                          // release the alignment slot
    emitter.instruction("pop rbx");                                             // restore the result array pointer
    emitter.instruction("pop r15");                                             // restore the element index
    emitter.instruction("pop r14");                                             // restore the source element count
    emitter.instruction("pop r13");                                             // restore the integer divisor
    emitter.instruction("pop r12");                                             // restore the packed source array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the stored map result

    emitter.label(overflow_label);
    emitter.instruction("mov rdi, rbx");                                        // pass the partial raw array for release
    emitter.instruction("call __rt_decref_array");                              // retire every slot built before the promoting element
    emitter.instruction("mov rdi, r14");                                        // allocate one Mixed slot per source element
    emitter.instruction("mov rsi, 8");                                          // boxed Mixed slots are pointer-sized
    emitter.instruction("call __rt_array_new");                                 // allocate the overflow destination container
    emitter.instruction("mov rbx, rax");                                        // rbx = boxed Mixed destination array
    emitter.instruction("mov r10, QWORD PTR [rbx - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve the heap marker, kind, and copy-on-write bits
    emitter.instruction("and r10, r11");                                        // clear the raw-int value_type bits
    emitter.instruction("or r10, 0x700");                                       // value_type 7 = boxed Mixed slots
    emitter.instruction("mov QWORD PTR [rbx - 8], r10");                        // publish the Mixed indexed-array metadata
    emitter.instruction("xor r15d, r15d");                                      // restart at the first source element

    emitter.label(mixed_loop);
    emitter.instruction("cmp r15, r14");                                        // has every element been rebuilt?
    emitter.instruction(&format!("jge {mixed_done}"));                          // finish once the Mixed array is complete
    abi::emit_load_int_immediate(emitter, "r8", i64::MIN);
    emitter.instruction("lea r10, [r12 + 24]");                                 // skip the source header to the payload base
    emitter.instruction("mov r11, QWORD PTR [r10 + r15 * 8]");                  // load the current source integer
    emitter.instruction("cmp r13, -1");                                         // only a -1 divisor can overflow INT64_MIN
    emitter.instruction(&format!("jne {mixed_divide}"));                        // every other divisor is safe to divide
    emitter.instruction("cmp r11, r8");                                         // is the element INT64_MIN?
    emitter.instruction(&format!("je {mixed_float}"));                          // INT64_MIN / -1 is a promoted double

    emitter.label(mixed_divide);
    emitter.instruction("mov rax, r11");                                        // stage the dividend for signed division
    emitter.instruction("cqo");                                                 // sign-extend the dividend before signed division
    emitter.instruction("idiv r13");                                            // quotient in rax, remainder in rdx
    emitter.instruction("test rdx, rdx");                                       // is the division exact?
    emitter.instruction(&format!("jnz {mixed_float}"));                         // a non-zero remainder yields a double
    emitter.instruction("mov rdi, rax");                                        // integer payload
    emitter.instruction("xor esi, esi");                                        // scalar payloads do not use a second word
    emitter.instruction("xor eax, eax");                                        // Mixed tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed integer cell
    emitter.instruction(&format!("jmp {mixed_store}"));                         // share the destination store

    emitter.label(mixed_float);
    emitter.instruction("cvtsi2sd xmm0, r11");                                  // convert the source integer to double
    emitter.instruction("cvtsi2sd xmm1, r13");                                  // convert the literal divisor to double
    emitter.instruction("divsd xmm0, xmm1");                                    // build PHP's promoted float quotient
    emitter.instruction("movq rdi, xmm0");                                      // float bits as the payload word
    emitter.instruction("xor esi, esi");                                        // scalar payloads do not use a second word
    emitter.instruction("mov eax, 2");                                          // Mixed tag 2 = float
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed float cell

    emitter.label(mixed_store);
    emitter.instruction("lea r10, [rbx + 24]");                                 // skip the Mixed destination header to the payload base
    emitter.instruction("mov QWORD PTR [r10 + r15 * 8], rax");                  // store the owned Mixed cell pointer
    emitter.instruction("add r15, 1");                                          // advance to the next source element
    emitter.instruction(&format!("jmp {mixed_loop}"));                          // continue until every element is boxed

    emitter.label(mixed_done);
    emitter.instruction("mov QWORD PTR [rbx], r14");                            // publish the completed logical length
    emitter.instruction("mov rax, rbx");                                        // return the boxed Mixed array
    emitter.instruction("add rsp, 8");                                          // release the alignment slot
    emitter.instruction("pop rbx");                                             // restore the result array pointer
    emitter.instruction("pop r15");                                             // restore the element index
    emitter.instruction("pop r14");                                             // restore the source element count
    emitter.instruction("pop r13");                                             // restore the integer divisor
    emitter.instruction("pop r12");                                             // restore the packed source array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the stored map result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits both the raw-int and Mixed rebuild paths.
    #[test]
    fn array_map_int_div_emits_all_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_map_int_div(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_array_map_int_div"), "{target}");
            assert!(asm.contains("__rt_array_new"), "{target}");
            assert!(asm.contains("__rt_mixed_from_value"), "{target}");
            assert!(asm.contains("__rt_decref_array"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("idiv r13"), "{target}");
                assert!(asm.contains("divsd xmm0, xmm1"), "{target}");
            } else {
                assert!(asm.contains("sdiv x11, x10, x20"), "{target}");
                assert!(asm.contains("fdiv d0, d0, d1"), "{target}");
            }
        }
    }
}
