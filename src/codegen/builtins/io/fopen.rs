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
    emitter.comment("fopen()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push filename ptr/len while the mode expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the mode pointer into the secondary runtime string-argument pair
            emitter.instruction("mov x4, x2");                                  // move the mode length into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the filename ptr/len after evaluating the mode expression
            abi::emit_call_label(emitter, "__rt_fopen");                        // open the file through the target-aware runtime helper
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the filename ptr/len while the mode expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the mode pointer into the x86_64 secondary runtime string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the mode length into the x86_64 secondary runtime string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the filename ptr/len after evaluating the mode expression
            abi::emit_call_label(emitter, "__rt_fopen");                        // open the file through the target-aware runtime helper
        }
    }
    Some(PhpType::Int)
}
