use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
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
    emitter.instruction("str x0, [sp, #-16]!");                                 // save pointer on stack

    // -- evaluate byte offset --
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov x1, x0");                                          // x1 = byte offset

    // -- add offset to pointer --
    emitter.instruction("ldr x0, [sp], #16");                                   // restore pointer
    emitter.instruction("add x0, x0, x1");                                      // x0 = pointer + byte offset
    Some(match ptr_ty {
        PhpType::Pointer(tag) => PhpType::Pointer(tag),
        _ => PhpType::Pointer(None),
    })
}
