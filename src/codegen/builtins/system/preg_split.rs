//! Purpose:
//! Emits PHP `preg_split` PCRE-style regex builtin calls.
//! Connects pattern/subject arguments and optional match arrays to runtime regex helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Match arrays and false/error results must use PHP-compatible Mixed array payloads.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

const PREG_SPLIT_FORCE_MIXED_RESULT: i64 = 1 << 30;

/// Emits the `preg_split` builtin call.
///
/// # Arguments
/// - `args[0]`: pattern string
/// - `args[1]`: subject string
/// - `args[2]`: optional limit
/// - `args[3]`: optional flags
///
/// # ABI Details
/// - ARM64: pattern in x1/x2, subject in x3/x4, limit in x5, flags in x6, result array pointer in x0
/// - x86_64: pattern in rdi/rsi, subject in rdx/rcx, limit in r8, flags in r9, result array pointer in rax
///
/// # Returns
/// `Array<string>` when no flags argument is present, otherwise `Array<mixed>`
/// so dynamic offset-capture flags cannot make the runtime layout disagree with
/// static codegen.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("preg_split()");

    match emitter.target.arch {
        Arch::AArch64 => {
            // -- evaluate arguments in PHP source order --
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push pattern ptr and len
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push subject ptr and len
            if let Some(limit) = args.get(2) {
                emit_expr(limit, emitter, ctx, data);
            } else {
                abi::emit_load_int_immediate(emitter, "x0", -1);
            }
            abi::emit_push_reg(emitter, "x0");
            if let Some(flags) = args.get(3) {
                emit_expr(flags, emitter, ctx, data);
                abi::emit_load_int_immediate(emitter, "x9", PREG_SPLIT_FORCE_MIXED_RESULT);
                emitter.instruction("orr x0, x0, x9");                          // force boxed-Mixed result slots for dynamic split flags
            } else {
                abi::emit_load_int_immediate(emitter, "x0", 0);
            }
            abi::emit_push_reg(emitter, "x0");
            abi::emit_pop_reg(emitter, "x6");
            abi::emit_pop_reg(emitter, "x5");
            emitter.instruction("ldp x3, x4, [sp], #16");                       // pop subject ptr/len into x3/x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop pattern ptr/len into x1/x2
            emitter.instruction("bl __rt_preg_split");                          // regex split → x0=array pointer
        }
        Arch::X86_64 => {
            emit_expr(&args[0], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                    // push pattern ptr and len
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                    // push subject ptr and len
            if let Some(limit) = args.get(2) {
                emit_expr(limit, emitter, ctx, data);
            } else {
                abi::emit_load_int_immediate(emitter, "rax", -1);
            }
            abi::emit_push_reg(emitter, "rax");
            if let Some(flags) = args.get(3) {
                emit_expr(flags, emitter, ctx, data);
                abi::emit_load_int_immediate(emitter, "r10", PREG_SPLIT_FORCE_MIXED_RESULT);
                emitter.instruction("or rax, r10");                             // force boxed-Mixed result slots for dynamic split flags
            } else {
                abi::emit_load_int_immediate(emitter, "rax", 0);
            }
            abi::emit_push_reg(emitter, "rax");
            abi::emit_pop_reg(emitter, "r9");
            abi::emit_pop_reg(emitter, "r8");
            abi::emit_pop_reg_pair(emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");
            abi::emit_call_label(emitter, "__rt_preg_split");                  // regex split → rax=array pointer
        }
    }

    let elem_ty = if args.len() >= 4 {
        PhpType::Mixed
    } else {
        PhpType::Str
    };
    Some(PhpType::Array(Box::new(elem_ty)))
}
