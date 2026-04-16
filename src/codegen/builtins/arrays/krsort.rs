use crate::codegen::abi;
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
    emitter.comment("krsort()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- sort associative array by keys descending --
    abi::emit_call_label(emitter, "__rt_krsort");                               // call the target-aware runtime helper that sorts associative-array keys descending in place

    Some(PhpType::Void)
}
