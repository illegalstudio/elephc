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
    emitter.comment("chr()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- convert ASCII code to single-character string --
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the integer character code into the first SysV runtime argument register before materializing the one-byte string
    }
    abi::emit_call_label(emitter, "__rt_chr");                                  // call the target-aware runtime helper that writes one byte into concat storage and returns it as a string

    Some(PhpType::Str)
}
