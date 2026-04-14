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
    emitter.comment("ptr_null()");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // materialize the null pointer sentinel in the AArch64 integer result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 0");                                  // materialize the null pointer sentinel in the x86_64 integer result register
        }
    }
    Some(PhpType::Pointer(None))
}
