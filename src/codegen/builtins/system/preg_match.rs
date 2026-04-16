use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("preg_match()");

    match emitter.target.arch {
        Arch::AArch64 => {
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push subject ptr and len
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("ldp x3, x4, [sp], #16");                       // pop subject ptr/len into x3/x4
            emitter.instruction("bl __rt_preg_match");                          // regex match → x0=1 if matched, 0 if not
        }
        Arch::X86_64 => {
            emit_expr(&args[1], emitter, ctx, data);
            crate::codegen::abi::emit_push_reg_pair(emitter, "rax", "rdx");     // push subject ptr and len
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // pass the pattern pointer in the first SysV integer argument register
            emitter.instruction("mov rsi, rdx");                                // pass the pattern length in the second SysV integer argument register
            crate::codegen::abi::emit_pop_reg_pair(emitter, "rdx", "rcx");      // pop subject ptr/len into the remaining SysV argument registers
            crate::codegen::abi::emit_call_label(emitter, "__rt_preg_match");   // regex match → rax=1 if matched, 0 if not
        }
    }

    Some(PhpType::Int)
}
