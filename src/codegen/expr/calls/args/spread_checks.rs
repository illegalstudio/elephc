//! Purpose:
//! Lowers runtime checks for spread array lengths and required parameter bounds.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection};
use crate::types::call_args::SpreadBoundsCheck;

pub(crate) fn emit_spread_length_checks(
    checks: &[SpreadBoundsCheck],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    for check in checks {
        let ok_label = ctx.next_label("named_spread_len_ok");
        let fail_label = ctx.next_label("named_spread_len_fail");
        emitter.comment("validate named-argument spread length");
        let _ = super::super::super::emit_expr(&check.spread_expr, emitter, ctx, data);
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("ldr x9, [x0]");                            // load the logical spread-array length before using synthetic positional reads
                emit_array_length_bounds_check("x9", check.min_len, check.max_len, &fail_label, &ok_label, emitter);
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("mov r10, QWORD PTR [rax]");                // load the logical spread-array length before using synthetic positional reads
                emit_array_length_bounds_check("r10", check.min_len, check.max_len, &fail_label, &ok_label, emitter);
            }
        }
        emitter.label(&fail_label);
        emit_named_spread_length_abort(emitter, data);
        emitter.label(&ok_label);
    }
}

pub(super) fn emit_array_length_bounds_check(
    length_reg: &str,
    min_len: usize,
    max_len: Option<usize>,
    fail_label: &str,
    ok_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x10", min_len as i64);
            emitter.instruction(&format!("cmp {}, x10", length_reg));           // ensure the array covers every required positional slot
            emitter.instruction(&format!("b.lt {}", fail_label));               // report a missing required argument instead of reading past the payload
            if let Some(max_len) = max_len {
                abi::emit_load_int_immediate(emitter, "x10", max_len as i64);
                emitter.instruction(&format!("cmp {}, x10", length_reg));       // ensure the array does not overwrite the next named slot
                emitter.instruction(&format!("b.le {}", ok_label));             // continue when the array length is within the allowed bounds
            } else {
                emitter.instruction(&format!("b {}", ok_label));                // variadic calls allow remaining spread values to flow into ...$rest
            }
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "r11", min_len as i64);
            emitter.instruction(&format!("cmp {}, r11", length_reg));           // ensure the array covers every required positional slot
            emitter.instruction(&format!("jl {}", fail_label));                 // report a missing required argument instead of reading past the payload
            if let Some(max_len) = max_len {
                abi::emit_load_int_immediate(emitter, "r11", max_len as i64);
                emitter.instruction(&format!("cmp {}, r11", length_reg));       // ensure the array does not overwrite the next named slot
                emitter.instruction(&format!("jle {}", ok_label));              // continue when the array length is within the allowed bounds
            } else {
                emitter.instruction(&format!("jmp {}", ok_label));              // variadic calls allow remaining spread values to flow into ...$rest
            }
        }
    }
}

pub(super) fn emit_named_spread_length_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) =
        data.add_string(b"Fatal error: named argument spread length mismatch\n");
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the named-argument spread diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the named-argument spread diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal named-argument spread diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}
