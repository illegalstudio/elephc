//! Purpose:
//! Compares a converted object with the value a warning handler left in reference storage.
//!
//! Called from:
//! - The ordinary object-versus-number arms in `super`.
//!
//! Key details:
//! - The original numeric tag selects int or float one before the complete comparison retries.
//! - A borrowed three-word Mixed cell lives on the stack; no heap owner is created.
//! - Parent frame offsets are AArch64 112 bytes and x86_64 96 bytes.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Retries PHP equality with numeric one and the current post-warning operand type.
pub(super) fn emit_compare_converted_object(emitter: &mut Emitter, left_object: bool) {
    emit_reload_numeric_operand(emitter, left_object);
    match emitter.target.arch {
        Arch::AArch64 => {
            if left_object {
                emitter.instruction("mov x1, x0");                              // preserve the current right boxed operand
            }
            let tag = if left_object { 48 } else { 24 };
            emitter.instruction(&format!("ldr x9, [sp, #{tag}]"));              // preserve the original numeric conversion kind
            emitter.instruction("ldr x2, [sp, #16]");                           // preserve structural recursion depth across scalar conversion
            emitter.instruction("mov x10, #1");                                 // prepare the converted integer payload
            emitter.instruction("fmov d0, #1.0");                               // prepare the converted floating payload
            emitter.instruction("fmov x11, d0");                                // hold the floating one bits in an integer register
            emitter.instruction("cmp x9, #2");                                  // did the original numeric operand request float conversion?
            emitter.instruction("csel x10, x11, x10, eq");                      // select the payload matching the saved numeric tag
            emitter.instruction("sub sp, sp, #32");                             // reserve an aligned borrowed Mixed cell
            emitter.instruction("str x9, [sp]");                                // store the converted object numeric tag
            emitter.instruction("str x10, [sp, #8]");                           // store numeric one in the selected representation
            emitter.instruction("str xzr, [sp, #16]");                          // clear the unused high payload word
            if left_object {
                emitter.instruction("mov x0, sp");                              // place converted one on the original object side
            } else {
                emitter.instruction("mov x1, sp");                              // place converted one on the original object side
            }
            abi::emit_call_label(emitter, "__rt_mixed_loose_eq_d");
            emitter.instruction("add sp, sp, #32");                             // retire the borrowed cell while preserving equality
        }
        Arch::X86_64 => {
            let numeric = if left_object { "rsi" } else { "rdi" };
            let converted = if left_object { "rdi" } else { "rsi" };
            let tag = if left_object { 56 } else { 32 };
            abi::emit_reg_move(emitter, numeric, "rax");
            emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {tag}]"));  // preserve the original numeric conversion kind
            emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");               // preserve structural recursion depth across scalar conversion
            emitter.instruction("mov rax, 1");                                  // prepare the converted integer payload
            emitter.instruction("movabs r10, 0x3ff0000000000000");              // prepare the converted floating payload bits
            emitter.instruction("cmp rcx, 2");                                  // did the original numeric operand request float conversion?
            emitter.instruction("cmove rax, r10");                              // select the payload matching the saved numeric tag
            emitter.instruction("sub rsp, 32");                                 // reserve an aligned borrowed Mixed cell
            emitter.instruction("mov QWORD PTR [rsp], rcx");                    // store the converted object numeric tag
            emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // store numeric one in the selected representation
            emitter.instruction("mov QWORD PTR [rsp + 16], 0");                 // clear the unused high payload word
            abi::emit_reg_move(emitter, converted, "rsp");
            abi::emit_call_label(emitter, "__rt_mixed_loose_eq_d");
            emitter.instruction("add rsp, 32");                                 // retire the borrowed cell while preserving equality
        }
    }
}

/// Reloads the current numeric box after a warning may have replaced reference storage.
fn emit_reload_numeric_operand(emitter: &mut Emitter, right: bool) {
    let done = if right {
        "__rt_mle_numeric_right_loaded"
    } else {
        "__rt_mle_numeric_left_loaded"
    };
    match emitter.target.arch {
        Arch::AArch64 => {
            let operand = if right { 8 } else { 0 };
            let reference = if right { 88 } else { 80 };
            emitter.instruction(&format!("ldr x0, [sp, #{operand}]"));          // recover the original value for non-reference operands
            emitter.instruction(&format!("ldr x9, [sp, #{reference}]"));        // recover optional stable reference storage
            emitter.instruction(&format!("cbz x9, {done}"));                    // value operands retain their original comparison snapshot
            emitter.instruction("ldr x0, [x9]");                                // reload the box currently owned by the reference cell
        }
        Arch::X86_64 => {
            let operand = if right { 16 } else { 8 };
            let reference = if right { 96 } else { 88 };
            emitter.instruction(
                &format!("mov rax, QWORD PTR [rbp - {operand}]")
            );                                                                  // recover the original value for non-reference operands
            emitter.instruction(
                &format!("mov r10, QWORD PTR [rbp - {reference}]")
            );                                                                  // recover optional stable reference storage
            emitter.instruction("test r10, r10");                               // distinguish references from value operands
            emitter.instruction(&format!("je {done}"));                         // value operands retain their original comparison snapshot
            emitter.instruction("mov rax, QWORD PTR [r10]");                    // reload the box currently owned by the reference cell
        }
    }
    emitter.label(done);
}
