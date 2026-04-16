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
    emitter.comment("ptr_get() — dereference pointer");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");                    // abort with fatal error on null pointer dereference before loading from pointer memory
    // -- load 8 bytes at the pointer address --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [x0]");                                // load one machine-word integer payload through the validated pointer on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rax]");                    // load one machine-word integer payload through the validated pointer on x86_64
        }
    }
    Some(PhpType::Int)
}
