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
    emitter.comment("ptr_set() — write value at pointer address");
    // -- evaluate pointer --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("bl __rt_ptr_check_nonnull");                           // abort with fatal error on null pointer dereference
    emitter.instruction("str x0, [sp, #-16]!");                                 // save pointer on stack

    // -- evaluate value to write --
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov x1, x0");                                          // x1 = value to write

    // -- store value at pointer address --
    emitter.instruction("ldr x0, [sp], #16");                                   // restore pointer
    emitter.instruction("str x1, [x0]");                                        // store value at pointer address
    Some(PhpType::Void)
}
