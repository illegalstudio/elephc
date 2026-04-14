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
    emitter.comment("ptr_write8() — write one byte at pointer address");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");                    // abort with a fatal error on null pointer dereference before writing to memory
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the target pointer while the value expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w1, w0");                                  // keep only the low 8 bits of the integer value in a scratch AArch64 register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the target pointer after evaluating the written value
            emitter.instruction("strb w1, [x0]");                               // store one byte at the destination pointer on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction("mov cl, al");                                  // keep only the low 8 bits of the integer value in a scratch x86_64 register
            abi::emit_pop_reg(emitter, "rax");                                  // restore the target pointer after evaluating the written value
            emitter.instruction("mov BYTE PTR [rax], cl");                      // store one byte at the destination pointer on x86_64
        }
    }
    Some(PhpType::Void)
}
