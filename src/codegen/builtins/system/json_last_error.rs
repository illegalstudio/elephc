use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("json_last_error()");
    // -- always return 0 (JSON_ERROR_NONE) --
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, 0");                                      // return 0 = JSON_ERROR_NONE in the x86_64 integer result register
    } else {
        emitter.instruction("mov x0, #0");                                      // return 0 = JSON_ERROR_NONE in the ARM64 integer result register
    }
    Some(PhpType::Int)
}
