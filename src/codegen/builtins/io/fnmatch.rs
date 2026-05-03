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
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the filename ptr/len while the flags expression is evaluated
            if let Some(flags) = args.get(2) {
                emit_expr(flags, emitter, ctx, data);
                emitter.instruction("mov x5, x0");                              // move the runtime flags into the fnmatch helper flag register
            } else {
                emitter.instruction("mov x5, #0");                              // default flags = 0
            }
            emitter.instruction("ldp x3, x4, [sp], #16");                       // restore the filename ptr/len into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the pattern ptr/len after evaluating the filename expression
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the pattern ptr/len while the filename expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the filename ptr/len while the flags expression is evaluated
            if let Some(flags) = args.get(2) {
                emit_expr(flags, emitter, ctx, data);
                emitter.instruction("mov rcx, rax");                            // move the runtime flags into the fnmatch helper flag register
            } else {
                emitter.instruction("xor ecx, ecx");                            // default flags = 0
            }
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the filename ptr/len into the secondary runtime string-argument slots
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the pattern ptr/len after evaluating the filename expression
        }
    }
    abi::emit_call_label(emitter, "__rt_fnmatch");                              // call the target-aware runtime helper that performs shell-glob matching
    Some(PhpType::Bool)
}
