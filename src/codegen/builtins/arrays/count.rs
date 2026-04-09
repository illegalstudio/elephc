use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("count()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- read element count from array header --
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_load_from_address(emitter, result_reg, result_reg, 0);            // load array length from the first field of the array header

    Some(PhpType::Int)
}
