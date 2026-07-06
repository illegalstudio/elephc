//! Purpose:
//! Emits runtime helpers for checked integer add/sub/mul with PHP overflow-to-float promotion.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Helpers take two raw I64 values and return a boxed Mixed cell (int or float).
//! - On overflow, the result is promoted to double to match PHP semantics.
//! - These helpers are used for non-constant integer arithmetic where the type
//!   checker cannot prove the result fits in int at compile time.
//! - Each entry point is fully self-contained (no cross-function local-label branches)
//!   to survive macOS `.subsections_via_symbols` dead-stripping.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the checked integer add/sub/mul helpers for both AArch64 and x86_64.
///
/// Input (AArch64):  x0 = left I64, x1 = right I64
/// Input (x86_64):   rdi = left I64, rsi = right I64
/// Output: boxed Mixed pointer in the integer result register (x0 / rax)
pub fn emit_int_checked_binops(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_int_checked_binops_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: int_checked_binops ---");

    emit_aarch64_checked(emitter, "__rt_int_add_checked", 0);
    emit_aarch64_checked(emitter, "__rt_int_sub_checked", 1);
    emit_aarch64_checked(emitter, "__rt_int_mul_checked", 2);
}

/// Emits one AArch64 checked integer helper as a fully self-contained function.
///
/// Allocates a 48-byte frame, saves FP/LR, performs the arithmetic with overflow
/// detection, boxes the result via `__rt_mixed_from_value`, restores the frame, and returns.
/// Each helper is independent to avoid cross-function local-label branching that
/// breaks under macOS `.subsections_via_symbols`.
fn emit_aarch64_checked(emitter: &mut Emitter, label: &str, opcode: i64) {
    emitter.label_global(label);
    emitter.instruction("sub sp, sp, #48");                                     // allocate a helper frame for operands and saved FP state
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the left integer operand
    emitter.instruction("str x1, [sp, #8]");                                    // save the right integer operand

    // -- integer path with PHP overflow promotion --
    match opcode {
        0 => {
            emitter.instruction("ldr x1, [sp, #0]");                            // reload the left integer operand
            emitter.instruction("ldr x2, [sp, #8]");                            // reload the right integer operand
            emitter.instruction("adds x0, x1, x2");                             // compute integer addition and set overflow flags
            let oob_label = format!("{}_overflow", label);
            emitter.instruction(&format!("b.vs {}", oob_label));                // promote to double when signed addition overflowed
            let done_label = format!("{}_box_int", label);
            emitter.instruction(&format!("b {}", done_label));                  // box the in-range integer result
            emit_aarch64_overflow(emitter, label, opcode, &oob_label);
            emit_aarch64_box_int(emitter, label, &done_label);
            emit_aarch64_done(emitter, label);
        }
        1 => {
            emitter.instruction("ldr x1, [sp, #0]");                            // reload the left integer operand
            emitter.instruction("ldr x2, [sp, #8]");                            // reload the right integer operand
            emitter.instruction("subs x0, x1, x2");                             // compute integer subtraction and set overflow flags
            let oob_label = format!("{}_overflow", label);
            emitter.instruction(&format!("b.vs {}", oob_label));                // promote to double when signed subtraction overflowed
            let done_label = format!("{}_box_int", label);
            emitter.instruction(&format!("b {}", done_label));                  // box the in-range integer result
            emit_aarch64_overflow(emitter, label, opcode, &oob_label);
            emit_aarch64_box_int(emitter, label, &done_label);
            emit_aarch64_done(emitter, label);
        }
        2 => {
            emitter.instruction("ldr x1, [sp, #0]");                            // reload the left integer operand
            emitter.instruction("ldr x2, [sp, #8]");                            // reload the right integer operand
            emitter.instruction("mul x0, x1, x2");                              // compute the low half of the signed integer product
            emitter.instruction("smulh x3, x1, x2");                            // compute the high half needed for overflow detection
            emitter.instruction("cmp x3, x0, asr #63");                         // high half must equal the sign extension of the low half
            let oob_label = format!("{}_overflow", label);
            emitter.instruction(&format!("b.ne {}", oob_label));                // promote to double when signed multiplication overflowed
            let done_label = format!("{}_box_int", label);
            emitter.instruction(&format!("b {}", done_label));                  // box the in-range integer result
            emit_aarch64_overflow(emitter, label, opcode, &oob_label);
            emit_aarch64_box_int(emitter, label, &done_label);
            emit_aarch64_done(emitter, label);
        }
        _ => unreachable!(),
    }
}

/// Emits the overflow-to-float promotion path for one AArch64 checked helper.
fn emit_aarch64_overflow(emitter: &mut Emitter, label: &str, opcode: i64, oob_label: &str) {
    emitter.label(oob_label);
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the original left integer operand
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the original right integer operand
    emitter.instruction("scvtf d0, x1");                                        // convert the original left integer to double for PHP overflow promotion
    emitter.instruction("scvtf d1, x2");                                        // convert the original right integer to double for PHP overflow promotion
    match opcode {
        0 => {
            emitter.instruction("fadd d0, d0, d1");                             // compute the double addition result
        }
        1 => {
            emitter.instruction("fsub d0, d0, d1");                             // compute the double subtraction result
        }
        2 => {
            emitter.instruction("fmul d0, d0, d1");                             // compute the double multiplication result
        }
        _ => unreachable!(),
    }
    emitter.instruction("fmov x1, d0");                                         // move the double bits into the Mixed helper payload register
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the double result into a Mixed cell
    emitter.instruction(&format!("b {}_done", label));                          // restore the helper frame and return the boxed result
}

/// Emits the in-range integer boxing path for one AArch64 checked helper.
fn emit_aarch64_box_int(emitter: &mut Emitter, _label: &str, box_label: &str) {
    emitter.label(box_label);
    emitter.instruction("mov x1, x0");                                          // move the integer result into the Mixed helper payload register
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer result into a Mixed cell
}

/// Emits the frame-restore and return epilogue for one AArch64 checked helper.
fn emit_aarch64_done(emitter: &mut Emitter, label: &str) {
    emitter.label(&format!("{}_done", label));
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return to generated code with boxed Mixed result in x0
}

/// Emits the Linux x86_64 checked integer add/sub/mul helpers.
fn emit_int_checked_binops_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: int_checked_binops ---");

    emit_x86_64_checked(emitter, "__rt_int_add_checked", 0);
    emit_x86_64_checked(emitter, "__rt_int_sub_checked", 1);
    emit_x86_64_checked(emitter, "__rt_int_mul_checked", 2);
}

/// Emits one x86_64 checked integer helper as a fully self-contained function.
fn emit_x86_64_checked(emitter: &mut Emitter, label: &str, opcode: i64) {
    emitter.label_global(label);
    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate aligned helper slots for operands and saved FP state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left integer operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right integer operand

    // -- preserve original operands for overflow promotion --
    emitter.instruction("mov r8, rdi");                                         // preserve the original left integer for overflow promotion
    emitter.instruction("mov r9, rsi");                                         // preserve the original right integer for overflow promotion

    match opcode {
        0 => {
            emitter.instruction("add rdi, rsi");                                // compute integer addition and set overflow flags
            let oob_label = format!("{}_overflow", label);
            emitter.instruction(&format!("jo {}", oob_label));                  // promote to double when signed addition overflowed
            let box_label = format!("{}_box_int", label);
            emitter.instruction(&format!("jmp {}", box_label));                 // box the in-range integer result
            emit_x86_64_overflow(emitter, label, opcode, &oob_label);
            emit_x86_64_box_int(emitter, label, &box_label, "rdi");
            emit_x86_64_done(emitter, label);
        }
        1 => {
            emitter.instruction("sub rdi, rsi");                                // compute integer subtraction and set overflow flags
            let oob_label = format!("{}_overflow", label);
            emitter.instruction(&format!("jo {}", oob_label));                  // promote to double when signed subtraction overflowed
            let box_label = format!("{}_box_int", label);
            emitter.instruction(&format!("jmp {}", box_label));                 // box the in-range integer result
            emit_x86_64_overflow(emitter, label, opcode, &oob_label);
            emit_x86_64_box_int(emitter, label, &box_label, "rdi");
            emit_x86_64_done(emitter, label);
        }
        2 => {
            emitter.instruction("mov rax, rdi");                                // move the left operand into rax for one-operand signed multiply
            emitter.instruction("imul rsi");                                    // compute signed multiplication and set overflow flags
            let oob_label = format!("{}_overflow", label);
            emitter.instruction(&format!("jo {}", oob_label));                  // promote to double when signed multiplication overflowed
            let box_label = format!("{}_box_int", label);
            emitter.instruction(&format!("jmp {}", box_label));                 // box the in-range integer result
            emit_x86_64_overflow(emitter, label, opcode, &oob_label);
            emit_x86_64_box_int(emitter, label, &box_label, "rax");
            emit_x86_64_done(emitter, label);
        }
        _ => unreachable!(),
    }
}

/// Emits the overflow-to-float promotion path for one x86_64 checked helper.
fn emit_x86_64_overflow(emitter: &mut Emitter, label: &str, opcode: i64, oob_label: &str) {
    emitter.label(oob_label);
    emitter.instruction("cvtsi2sd xmm0, r8");                                   // convert the original left integer to double for PHP overflow promotion
    emitter.instruction("cvtsi2sd xmm1, r9");                                   // convert the original right integer to double for PHP overflow promotion
    match opcode {
        0 => {
            emitter.instruction("addsd xmm0, xmm1");                            // compute the double addition result
        }
        1 => {
            emitter.instruction("subsd xmm0, xmm1");                            // compute the double subtraction result
        }
        2 => {
            emitter.instruction("mulsd xmm0, xmm1");                            // compute the double multiplication result
        }
        _ => unreachable!(),
    }
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into the Mixed helper payload register
    emitter.instruction("xor rsi, rsi");                                        // double payloads do not use a high word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the double result into a Mixed cell
    emitter.instruction(&format!("jmp {}_done", label));                        // restore the helper frame and return the boxed result
}

/// Emits the in-range integer boxing path for one x86_64 checked helper.
fn emit_x86_64_box_int(emitter: &mut Emitter, _label: &str, box_label: &str, result_reg: &str) {
    emitter.label(box_label);
    emitter.instruction(&format!("mov rdi, {}", result_reg));                   // move the integer result into the Mixed helper payload register
    emitter.instruction("xor rsi, rsi");                                        // integer payloads do not use a high word
    emitter.instruction("mov rax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer result into a Mixed cell
}

/// Emits the frame-restore and return epilogue for one x86_64 checked helper.
fn emit_x86_64_done(emitter: &mut Emitter, label: &str) {
    emitter.label(&format!("{}_done", label));
    emitter.instruction("add rsp, 48");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code with boxed Mixed result in rax
}
