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
    emitter.comment("fread()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the file descriptor while the length expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the requested byte count into the fread helper length register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the file descriptor into the fread helper fd register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move the requested byte count into the second SysV fread helper argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the file descriptor into the first SysV fread helper argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_fread");                                // read bytes through the target-aware runtime helper and return an elephc string
    Some(PhpType::Str)
}
