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
    emitter.comment("glob()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_glob");                                 // call the target-aware runtime helper that expands the glob pattern into a string array
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
