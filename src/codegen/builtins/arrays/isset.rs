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
    emitter.comment("isset()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- compiled variables always exist, so isset returns true --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #1");                                  // return 1 (true) since the compiled variable is always set
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 1");                                  // return 1 (true) since the compiled variable is always set
        }
    }

    Some(PhpType::Int)
}
