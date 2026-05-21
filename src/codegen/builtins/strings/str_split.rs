//! Purpose:
//! Emits PHP `str_split` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("str_split()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the source string while evaluating the optional chunk-length expression
            if args.len() >= 2 {
                super::args::emit_int_arg(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x0");                              // move the requested chunk length into the AArch64 helper argument register
            } else {
                emitter.instruction("mov x3, #1");                              // default to one-byte chunks when str_split() omits the chunk length
            }
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the source string after evaluating the optional chunk-length expression
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the source string while evaluating the optional chunk-length expression
            if args.len() >= 2 {
                super::args::emit_int_arg(&args[1], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the requested chunk length into the extra x86_64 helper argument register
            } else {
                emitter.instruction("mov rdi, 1");                              // default to one-byte chunks when str_split() omits the chunk length
            }
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the source string into the x86_64 string-helper input registers
        }
    }
    abi::emit_call_label(emitter, "__rt_str_split");                            // split the source string into fixed-size chunks through the target-aware runtime helper
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
