//! Purpose:
//! Emits PHP `stream_select` calls.
//! Waits for readiness across three resource arrays and reports the ready count.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The three array arguments are by-reference: `__rt_stream_select` compacts
//!   each in place to its ready subset (no reallocation), so the caller's
//!   variables observe the result without a pointer write-back.
//! - Arguments are evaluated in source order, then materialized into the
//!   five `__rt_stream_select` argument registers.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_select()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_select()");
    let result = abi::int_result_reg(emitter);

    // -- evaluate the five arguments in source order, preserving each --
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, result); // preserve the read array
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, result); // preserve the write array
    emit_expr(&args[2], emitter, ctx, data);
    abi::emit_push_reg(emitter, result); // preserve the except array
    emit_expr(&args[3], emitter, ctx, data);
    abi::emit_push_reg(emitter, result); // preserve the seconds timeout
    if args.len() >= 5 {
        emit_expr(&args[4], emitter, ctx, data);
    } else {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #0"),                 // omitted microseconds default to 0
            Arch::X86_64 => emitter.instruction("xor eax, eax"),                // omitted microseconds default to 0
        }
    }

    // -- materialize the arguments into the runtime-helper registers --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x4, x0");                                  // microseconds become the fifth helper argument
            abi::emit_pop_reg(emitter, "x3"); // seconds become the fourth helper argument
            abi::emit_pop_reg(emitter, "x2"); // except array becomes the third helper argument
            abi::emit_pop_reg(emitter, "x1"); // write array becomes the second helper argument
            abi::emit_pop_reg(emitter, "x0"); // read array becomes the first helper argument
        }
        Arch::X86_64 => {
            emitter.instruction("mov r8, rax");                                 // microseconds become the fifth SysV helper argument
            abi::emit_pop_reg(emitter, "rcx"); // seconds become the fourth SysV helper argument
            abi::emit_pop_reg(emitter, "rdx"); // except array becomes the third SysV helper argument
            abi::emit_pop_reg(emitter, "rsi"); // write array becomes the second SysV helper argument
            abi::emit_pop_reg(emitter, "rdi"); // read array becomes the first SysV helper argument
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_select");
    Some(PhpType::Int)
}
