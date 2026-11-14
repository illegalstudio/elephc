//! Purpose:
//! Emits the stored float map kernels `__rt_array_map_float_scale` and
//! `__rt_array_map_float_div` for `array_map(fn($v) => $v * F | $v / F, $packedInts)`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - Every source integer is converted to double and multiplied or divided by the literal
//!   operand, so the result is always a raw `array<float>` with value_type 2 and 8-byte
//!   IEEE-754 slots.
//! - No per-element boxed Mixed cell is allocated and there is no int/float promotion branch:
//!   PHP's `$v * F` and `$v / F` are floats for every integer source element.
//! - A zero divisor is rejected by the AST fusion, so the division kernel never has to raise
//!   DivisionByZeroError and the generic map keeps that diagnostic.
//! - The result is an ordinary copy-on-write heap container created by `__rt_array_new`;
//!   callers own exactly the returned reference.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the stored float-scale map kernel for the active target.
pub fn emit_array_map_float_scale(emitter: &mut Emitter) {
    emit_float_map(emitter, false);
}

/// Emits the stored float-division map kernel for the active target.
pub fn emit_array_map_float_div(emitter: &mut Emitter) {
    emit_float_map(emitter, true);
}

/// Emits one stored float map kernel; `divides` selects division over multiplication.
fn emit_float_map(emitter: &mut Emitter, divides: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter, divides);
        return;
    }
    emit_aarch64(emitter, divides);
}

/// Emits the AArch64 kernel.
fn emit_aarch64(emitter: &mut Emitter, divides: bool) {
    let prefix = if divides {
        "__rt_array_map_float_div"
    } else {
        "__rt_array_map_float_scale"
    };
    let operation = if divides { "fdiv" } else { "fmul" };
    let loop_label = format!("{prefix}_loop");
    let done = format!("{prefix}_done");

    emitter.blank();
    emitter.comment(&format!("--- runtime: {prefix} (stored float map) ---"));
    emitter.label_global(prefix);
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish a stable helper frame
    emitter.instruction("stp x19, x20, [sp, #-16]!");                           // preserve the source and operand bits
    emitter.instruction("stp x21, x22, [sp, #-16]!");                           // preserve the element count and result array
    emitter.instruction("stp x23, x24, [sp, #-16]!");                           // preserve the element index
    emitter.instruction("mov x19, x0");                                         // x19 = packed source array
    emitter.instruction("fmov x20, d0");                                        // x20 = literal operand bit pattern
    emitter.instruction("ldr x21, [x19]");                                      // x21 = source element count
    emitter.instruction("mov x0, x21");                                         // allocate exactly one slot per source element
    emitter.instruction("mov x1, #8");                                          // raw double slots are pointer-sized
    emitter.instruction("bl __rt_array_new");                                   // allocate the copy-on-write destination container
    emitter.instruction("mov x22, x0");                                         // x22 = raw float destination array
    emitter.instruction("ldr x9, [x22, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("mov x10, #0x80ff");                                    // preserve heap kind and copy-on-write metadata
    emitter.instruction("and x9, x9, x10");                                     // clear the default raw-int value_type bits
    emitter.instruction("mov x10, #2");                                         // value_type 2 = raw float slots
    emitter.instruction("lsl x10, x10, #8");                                    // move the float tag into the packed kind lane
    emitter.instruction("orr x9, x9, x10");                                     // combine stable metadata with the float tag
    emitter.instruction("str x9, [x22, #-8]");                                  // publish the raw-float indexed-array metadata
    emitter.instruction("mov x23, xzr");                                        // x23 = current element index

    emitter.label(&loop_label);
    emitter.instruction("cmp x23, x21");                                        // has every element been mapped?
    emitter.instruction(&format!("b.ge {done}"));                               // finish once the loop exhausts the source
    emitter.instruction("add x9, x19, #24");                                    // skip the source header to the payload base
    emitter.instruction("ldr x10, [x9, x23, lsl #3]");                          // load the current source integer
    emitter.instruction("scvtf d0, x10");                                       // convert the source integer to double
    emitter.instruction("fmov d1, x20");                                        // reload the literal operand bit pattern
    emitter.instruction(&format!("{operation} d0, d0, d1"));                    // apply the map operation in PHP's float domain
    emitter.instruction("add x9, x22, #24");                                    // skip the destination header to the payload base
    emitter.instruction("str d0, [x9, x23, lsl #3]");                           // store the mapped raw double
    emitter.instruction("add x23, x23, #1");                                    // advance to the next source element
    emitter.instruction(&format!("b {loop_label}"));                            // continue until every element is mapped

    emitter.label(&done);
    emitter.instruction("str x21, [x22]");                                      // publish the completed logical length
    emitter.instruction("mov x0, x22");                                         // return the raw float array
    emitter.instruction("ldp x23, x24, [sp], #16");                             // restore the element index
    emitter.instruction("ldp x21, x22, [sp], #16");                             // restore the element count and result array
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the source and operand bits
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the stored map result
}

/// Emits the x86_64 kernel.
fn emit_x86_64(emitter: &mut Emitter, divides: bool) {
    let prefix = if divides {
        "__rt_array_map_float_div"
    } else {
        "__rt_array_map_float_scale"
    };
    let operation = if divides { "divsd" } else { "mulsd" };
    let loop_label = format!("{prefix}_loop_x86");
    let done = format!("{prefix}_done_x86");

    emitter.blank();
    emitter.comment(&format!("--- runtime: {prefix} (stored float map) ---"));
    emitter.label_global(prefix);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("push r12");                                            // preserve the packed source array
    emitter.instruction("push r13");                                            // preserve the literal operand bit pattern
    emitter.instruction("push r14");                                            // preserve the source element count
    emitter.instruction("push r15");                                            // preserve the element index
    emitter.instruction("push rbx");                                            // preserve the result array pointer
    emitter.instruction("sub rsp, 8");                                          // keep the stack aligned for runtime calls
    emitter.instruction("mov r12, rdi");                                        // r12 = packed source array
    emitter.instruction("movq r13, xmm0");                                      // r13 = literal operand bit pattern
    emitter.instruction("mov r14, QWORD PTR [r12]");                            // r14 = source element count
    emitter.instruction("mov rdi, r14");                                        // allocate exactly one slot per source element
    emitter.instruction("mov rsi, 8");                                          // raw double slots are pointer-sized
    emitter.instruction("call __rt_array_new");                                 // allocate the copy-on-write destination container
    emitter.instruction("mov rbx, rax");                                        // rbx = raw float destination array
    emitter.instruction("mov r10, QWORD PTR [rbx - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve the heap marker, kind, and copy-on-write bits
    emitter.instruction("and r10, r11");                                        // clear the default raw-int value_type bits
    emitter.instruction("or r10, 0x200");                                       // value_type 2 = raw float slots
    emitter.instruction("mov QWORD PTR [rbx - 8], r10");                        // publish the raw-float indexed-array metadata
    emitter.instruction("xor r15d, r15d");                                      // initialize the element index to zero

    emitter.label(&loop_label);
    emitter.instruction("cmp r15, r14");                                        // has every element been mapped?
    emitter.instruction(&format!("jge {done}"));                                // finish once the loop exhausts the source
    emitter.instruction("lea r10, [r12 + 24]");                                 // skip the source header to the payload base
    emitter.instruction("mov r11, QWORD PTR [r10 + r15 * 8]");                  // load the current source integer
    emitter.instruction("cvtsi2sd xmm0, r11");                                  // convert the source integer to double
    emitter.instruction("movq xmm1, r13");                                      // reload the literal operand bit pattern
    emitter.instruction(&format!("{operation} xmm0, xmm1"));                    // apply the map operation in PHP's float domain
    emitter.instruction("lea r10, [rbx + 24]");                                 // skip the destination header to the payload base
    emitter.instruction("movsd QWORD PTR [r10 + r15 * 8], xmm0");               // store the mapped raw double
    emitter.instruction("add r15, 1");                                          // advance to the next source element
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue until every element is mapped

    emitter.label(&done);
    emitter.instruction("mov QWORD PTR [rbx], r14");                            // publish the completed logical length
    emitter.instruction("mov rax, rbx");                                        // return the raw float array
    emitter.instruction("add rsp, 8");                                          // release the alignment slot
    emitter.instruction("pop rbx");                                             // restore the result array pointer
    emitter.instruction("pop r15");                                             // restore the element index
    emitter.instruction("pop r14");                                             // restore the source element count
    emitter.instruction("pop r13");                                             // restore the literal operand bit pattern
    emitter.instruction("pop r12");                                             // restore the packed source array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the stored map result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits both raw float stores and value_type stamps.
    #[test]
    fn array_map_float_map_emits_all_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_map_float_scale(&mut emitter);
            emit_array_map_float_div(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_array_map_float_scale"), "{target}");
            assert!(asm.contains("__rt_array_map_float_div"), "{target}");
            assert!(asm.contains("__rt_array_new"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("cvtsi2sd xmm0, r11"), "{target}");
                assert!(asm.contains("mulsd xmm0, xmm1"), "{target}");
                assert!(asm.contains("divsd xmm0, xmm1"), "{target}");
                assert!(asm.contains("or r10, 0x200"), "{target}");
            } else {
                assert!(asm.contains("scvtf d0, x10"), "{target}");
                assert!(asm.contains("fmul d0, d0, d1"), "{target}");
                assert!(asm.contains("fdiv d0, d0, d1"), "{target}");
                assert!(asm.contains("mov x10, #2"), "{target}");
            }
        }
    }
}
