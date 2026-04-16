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
    emitter.comment("ptr_read32() — read one 32-bit word at pointer address");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");                    // abort with a fatal error on null pointer dereference before reading from memory
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr w0, [x0]");                                // load one 32-bit word and zero-extend it through the AArch64 integer result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov eax, DWORD PTR [rax]");                    // load one 32-bit word and zero-extend it through the x86_64 integer result register
        }
    }
    Some(PhpType::Int)
}
