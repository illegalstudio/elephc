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
    emitter.comment("preg_replace()");

    match emitter.target.arch {
        Arch::AArch64 => {
            // -- evaluate subject string (arg 2) first --
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push subject ptr and len

            // -- evaluate replacement string (arg 1) --
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push replacement ptr and len

            // -- evaluate pattern string (arg 0) --
            emit_expr(&args[0], emitter, ctx, data);

            // -- pop replacement into x3/x4 --
            emitter.instruction("ldp x3, x4, [sp], #16");                       // pop replacement ptr/len into x3/x4

            // -- pop subject into x5/x6 --
            emitter.instruction("ldp x5, x6, [sp], #16");                       // pop subject ptr/len into x5/x6

            // -- call runtime: x1/x2=pattern, x3/x4=replacement, x5/x6=subject --
            emitter.instruction("bl __rt_preg_replace");                        // regex replace → x1=result ptr, x2=result len
        }
        Arch::X86_64 => {
            emit_expr(&args[2], emitter, ctx, data);
            crate::codegen::abi::emit_push_reg_pair(emitter, "rax", "rdx");     // push subject ptr and len
            emit_expr(&args[1], emitter, ctx, data);
            crate::codegen::abi::emit_push_reg_pair(emitter, "rax", "rdx");     // push replacement ptr and len
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // pass the pattern pointer in the first SysV integer argument register
            emitter.instruction("mov rsi, rdx");                                // pass the pattern length in the second SysV integer argument register
            crate::codegen::abi::emit_pop_reg_pair(emitter, "rdx", "rcx");      // pop replacement ptr/len into the next SysV integer argument registers
            crate::codegen::abi::emit_pop_reg_pair(emitter, "r8", "r9");        // pop subject ptr/len into the remaining SysV integer argument registers
            crate::codegen::abi::emit_call_label(emitter, "__rt_preg_replace"); // regex replace → rax=result ptr, rdx=result len
        }
    }

    Some(PhpType::Str)
}
