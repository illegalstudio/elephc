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
    emitter.comment("strtotime()");

    // -- evaluate date string argument --
    emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the input string pointer into the first SysV string-argument register
        emitter.instruction("mov rsi, rdx");                                    // move the input string length into the paired SysV string-argument register
    }

    // -- call runtime to parse date string and return timestamp --
    abi::emit_call_label(emitter, "__rt_strtotime");                            // parse the supported date/time string formats through the target-aware runtime helper

    Some(PhpType::Int)
}
