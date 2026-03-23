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
    emitter.comment("array_product()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- call runtime to compute product of all array elements --
    emitter.instruction("bl __rt_array_product");                               // call runtime: multiply array elements → x0=product

    Some(PhpType::Int)
}
