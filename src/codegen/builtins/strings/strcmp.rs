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
    emitter.comment("strcmp()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the first string pointer and length while evaluating the second string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the second string pointer into the third string-helper argument register
            emitter.instruction("mov x4, x2");                                  // move the second string length into the fourth string-helper argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the first string pointer and length after evaluating the second string
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the first string pointer and length while evaluating the second string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // move the second string length into the fourth SysV string-helper argument register
            emitter.instruction("mov rdx, rax");                                // move the second string pointer into the third SysV string-helper argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the first string pointer and length into the first two SysV helper argument registers
        }
    }
    abi::emit_call_label(emitter, "__rt_strcmp");                               // compare both strings lexicographically through the shared runtime helper

    Some(PhpType::Int)
}
