//! Purpose:
//! Lowers runtime checks for spread array lengths and required parameter bounds.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen_support::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, context::Context, data_section::DataSection};
use crate::types::call_args::SpreadBoundsCheck;

/// Iterates over `SpreadBoundsCheck` descriptors, emits the spread expression evaluation,
/// then emits a bounds check that branches to `fail_label` if the array length does not
/// cover all required positional slots, or to `ok_label` if it does. On failure, calls
/// `emit_named_spread_length_abort`.
pub(crate) fn emit_spread_length_checks(
    checks: &[SpreadBoundsCheck],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    for check in checks {
        let ok_label = ctx.next_label("named_spread_len_ok");
        let underflow_label = ctx.next_label("named_spread_len_underflow");
        let overflow_label = ctx.next_label("named_spread_len_overflow");
        emitter.comment("validate named-argument spread length");
        let _ = super::super::super::emit_expr(&check.spread_expr, emitter, ctx, data);
        match emitter.target.arch {
            crate::codegen_support::platform::Arch::AArch64 => {
                emitter.instruction("ldr x9, [x0]");                            // load the logical spread-array length before using synthetic positional reads
                emit_array_length_bounds_check(
                    "x9",
                    check.min_len,
                    check.max_len,
                    &underflow_label,
                    &overflow_label,
                    &ok_label,
                    emitter,
                );
            }
            crate::codegen_support::platform::Arch::X86_64 => {
                emitter.instruction("mov r10, QWORD PTR [rax]");                // load the logical spread-array length before using synthetic positional reads
                emit_array_length_bounds_check(
                    "r10",
                    check.min_len,
                    check.max_len,
                    &underflow_label,
                    &overflow_label,
                    &ok_label,
                    emitter,
                );
            }
        }
        emitter.label(&underflow_label);
        emit_named_spread_length_abort(emitter, data);
        emitter.label(&overflow_label);
        if let Some(param_name) = check.max_len_param_name.as_deref() {
            emit_named_spread_duplicate_abort(emitter, data, param_name);
        } else {
            emit_named_spread_length_abort(emitter, data);
        }
        emitter.label(&ok_label);
    }
}

/// Loads the integer constant `min_len` into a scratch register and compares it again `length_reg`.
/// Branches to `underflow_fail_label` if `length_reg < min_len`. If `max_len` is `Some`,
/// compares `length_reg` against it and branches to `overflow_fail_label` when the
/// spread would overwrite a later named argument; otherwise it branches to `ok_label`.
pub(super) fn emit_array_length_bounds_check(
    length_reg: &str,
    min_len: usize,
    max_len: Option<usize>,
    underflow_fail_label: &str,
    overflow_fail_label: &str,
    ok_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x10", min_len as i64);
            emitter.instruction(&format!("cmp {}, x10", length_reg));           // ensure the array covers every required positional slot
            emitter.instruction(&format!("b.lt {}", underflow_fail_label));     // report a missing required argument instead of reading past the payload
            if let Some(max_len) = max_len {
                abi::emit_load_int_immediate(emitter, "x10", max_len as i64);
                emitter.instruction(&format!("cmp {}, x10", length_reg));       // ensure the array does not overwrite the next named slot
                emitter.instruction(&format!("b.le {}", ok_label));             // continue when the array length is within the allowed bounds
                emitter.instruction(&format!("b {}", overflow_fail_label));     // report the named parameter overwritten by the spread prefix
            } else {
                emitter.instruction(&format!("b {}", ok_label));                // variadic calls allow remaining spread values to flow into ...$rest
            }
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "r11", min_len as i64);
            emitter.instruction(&format!("cmp {}, r11", length_reg));           // ensure the array covers every required positional slot
            emitter.instruction(&format!("jl {}", underflow_fail_label));       // report a missing required argument instead of reading past the payload
            if let Some(max_len) = max_len {
                abi::emit_load_int_immediate(emitter, "r11", max_len as i64);
                emitter.instruction(&format!("cmp {}, r11", length_reg));       // ensure the array does not overwrite the next named slot
                emitter.instruction(&format!("jle {}", ok_label));              // continue when the array length is within the allowed bounds
                emitter.instruction(&format!("jmp {}", overflow_fail_label));   // report the named parameter overwritten by the spread prefix
            } else {
                emitter.instruction(&format!("jmp {}", ok_label));              // variadic calls allow remaining spread values to flow into ...$rest
            }
        }
    }
}

/// Writes a fixed diagnostic message to stderr and then exits the process with code 1.
pub(super) fn emit_named_spread_length_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) =
        data.add_string(b"Fatal error: named argument spread length mismatch\n");
    match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the named-argument spread diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the named-argument spread diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal named-argument spread diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Writes the PHP-compatible duplicate named-parameter diagnostic and exits with code 1.
pub(super) fn emit_named_spread_duplicate_abort(
    emitter: &mut Emitter,
    data: &mut DataSection,
    param_name: &str,
) {
    let message = format!(
        "Fatal error: Named parameter ${} overwrites previous argument\n",
        param_name
    );
    let (message_label, message_len) = data.add_string(message.as_bytes());
    match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the duplicate named-argument diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the duplicate named-argument diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal duplicate named-argument diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}
