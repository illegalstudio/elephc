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
    emitter.comment("intval()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty == PhpType::Str {
        // -- convert string to integer --
        abi::emit_call_label(emitter, "__rt_atoi");                             // parse the current string result through the target-aware atoi runtime helper
    }
    Some(PhpType::Int)
}
