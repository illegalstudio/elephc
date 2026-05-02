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
    emitter.comment("dirname()");
    emit_expr(&args[0], emitter, ctx, data);
    if args.len() == 1 {
        abi::emit_call_label(emitter, "__rt_dirname");                          // call the target-aware runtime helper that returns the parent-directory portion
        return Some(PhpType::Str);
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the path ptr/len while the levels expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // move the requested parent depth into the runtime levels register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the path ptr/len after evaluating the levels expression
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the path ptr/len while the levels expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the requested parent depth into the x86_64 runtime levels register
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the path ptr/len after evaluating the levels expression
        }
    }
    abi::emit_call_label(emitter, "__rt_dirname_levels");                       // call the target-aware runtime helper that applies dirname() repeatedly
    Some(PhpType::Str)
}
