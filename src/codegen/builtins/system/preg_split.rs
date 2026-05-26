//! Purpose:
//! Emits PHP `preg_split` PCRE-style regex builtin calls.
//! Connects pattern/subject arguments and optional match arrays to runtime regex helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Match arrays and false/error results must use PHP-compatible Mixed array payloads.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `preg_split` builtin call.
///
/// # Arguments
/// - `args[0]`: pattern string
/// - `args[1]`: subject string
///
/// # ABI Details
/// - ARM64: pattern in x1/x2, subject in x3/x4, result array pointer in x0
/// - x86_64: pattern in rdi/rdx, subject in rsi/rcx, result array pointer in rax
///
/// # Returns
/// `PhpType::Array(Box::new(PhpType::Str))` — array of string segments from the split.
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
            // -- evaluate subject string (arg 1) first --
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push subject ptr and len

            // -- evaluate pattern string (arg 0) --
            emit_expr(&args[0], emitter, ctx, data);

            // -- pop subject into x3/x4 --
            emitter.instruction("ldp x3, x4, [sp], #16");                       // pop subject ptr/len into x3/x4

            // -- call runtime: x1/x2=pattern, x3/x4=subject --
            emitter.instruction("bl __rt_preg_split");                          // regex split → x0=array pointer
        }
        Arch::X86_64 => {
            emit_expr(&args[1], emitter, ctx, data);
            crate::codegen::abi::emit_push_reg_pair(emitter, "rax", "rdx");     // push subject ptr and len
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // pass the pattern pointer in the first SysV integer argument register
            emitter.instruction("mov rsi, rdx");                                // pass the pattern length in the second SysV integer argument register
            crate::codegen::abi::emit_pop_reg_pair(emitter, "rdx", "rcx");      // pop subject ptr/len into the remaining SysV integer argument registers
            crate::codegen::abi::emit_call_label(emitter, "__rt_preg_split");   // regex split → rax=array pointer
        }
    }

    Some(PhpType::Array(Box::new(PhpType::Str)))
}
