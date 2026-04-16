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
    emitter.comment("tempnam()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push dir ptr and length onto the stack while the prefix expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the prefix pointer into the third ARM64 string-argument slot
            emitter.instruction("mov x4, x2");                                  // move the prefix length into the fourth ARM64 string-argument slot
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the directory string pair after evaluating the prefix expression
            abi::emit_call_label(emitter, "__rt_tempnam");                      // call the target-aware runtime helper that builds the temp filename
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the directory string pair while the prefix expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the prefix pointer into the third x86_64 string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the prefix length into the fourth x86_64 string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the directory string pair after evaluating the prefix expression
            abi::emit_call_label(emitter, "__rt_tempnam");                      // call the target-aware runtime helper that builds the temp filename
        }
    }
    Some(PhpType::Str)
}
