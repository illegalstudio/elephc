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
    emitter.comment("is_string()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- return true/false based on compile-time type --
    let val = if ty == PhpType::Str { 1 } else { 0 };
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", val));                  // set result: 1 if string, 0 otherwise
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", val));                  // set result: 1 if string, 0 otherwise
        }
    }
    Some(PhpType::Bool)
}
