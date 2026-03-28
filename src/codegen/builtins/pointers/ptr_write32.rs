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
    emitter.comment("ptr_write32() — write one 32-bit word at pointer address");
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("bl __rt_ptr_check_nonnull");                           // abort with fatal error on null pointer dereference
    emitter.instruction("str x0, [sp, #-16]!");                                 // save target pointer while value is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov w1, w0");                                          // keep the low 32 bits of the integer value
    emitter.instruction("ldr x0, [sp], #16");                                   // restore target pointer
    emitter.instruction("str w1, [x0]");                                        // store one 32-bit word at the pointer address
    Some(PhpType::Void)
}
