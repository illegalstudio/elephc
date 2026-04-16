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
    emitter.comment("ptr_offset()");
    // -- evaluate pointer expression --
    let ptr_ty = emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the base pointer while the byte-offset expression is evaluated

    // -- evaluate byte offset --
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // copy the byte offset into a scratch integer register on AArch64
            abi::emit_pop_reg(emitter, "x0");                                   // restore the base pointer after the byte-offset expression has been evaluated
            emitter.instruction("add x0, x0, x1");                              // compute the derived pointer address on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rax");                                // copy the byte offset into a scratch integer register on x86_64
            abi::emit_pop_reg(emitter, "rax");                                  // restore the base pointer after the byte-offset expression has been evaluated
            emitter.instruction("add rax, rcx");                                // compute the derived pointer address on x86_64
        }
    }
    Some(match ptr_ty {
        PhpType::Pointer(tag) => PhpType::Pointer(tag),
        _ => PhpType::Pointer(None),
    })
}
