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
    emitter.comment("ptr_is_null()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- check if pointer is null (0x0) --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // compare the pointer payload against the null sentinel on AArch64
            emitter.instruction("cset x0, eq");                                 // materialize 1 when the pointer is null and 0 otherwise on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // compare the pointer payload against the null sentinel on x86_64
            emitter.instruction("sete al");                                     // materialize the boolean null result in the low byte register
            emitter.instruction("movzx rax, al");                               // widen the boolean null result back into the x86_64 integer result register
        }
    }
    Some(PhpType::Bool)
}
