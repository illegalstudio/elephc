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
    emitter.comment("fnmatch()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the pattern ptr/len while the filename expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the filename pointer into the secondary runtime string-argument pair
            emitter.instruction("mov x4, x2");                                  // move the filename length into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the pattern ptr/len after evaluating the filename expression
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the pattern ptr/len while the filename expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the filename pointer into the x86_64 secondary runtime string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the filename length into the x86_64 secondary runtime string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the pattern ptr/len after evaluating the filename expression
        }
    }
    abi::emit_call_label(emitter, "__rt_fnmatch");                              // call the target-aware runtime helper that performs shell-glob matching
    Some(PhpType::Bool)
}
