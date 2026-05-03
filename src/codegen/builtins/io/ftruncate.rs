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
    emitter.comment("ftruncate()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve fd while size is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x1, x0");                                  // size → second runtime arg
            emitter.instruction("ldr x0, [sp], #16");                           // restore fd into the first runtime arg
        }
        Arch::X86_64 => {
            emitter.instruction("push rax");                                    // preserve fd
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdx, rax");                                // size → secondary integer arg slot
            emitter.instruction("pop rax");                                     // restore fd
        }
    }
    abi::emit_call_label(emitter, "__rt_ftruncate");                            // call libc ftruncate(fd, size) wrapper
    Some(PhpType::Bool)
}
