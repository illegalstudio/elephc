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
    emitter.comment("ptr_read8() — read one byte at pointer address");
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("bl __rt_ptr_check_nonnull");                           // abort with fatal error on null pointer dereference
    emitter.instruction("ldrb w0, [x0]");                                        // load one unsigned byte and zero-extend to x0
    Some(PhpType::Int)
}
