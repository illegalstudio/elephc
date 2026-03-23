use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("function_exists()");

    // -- resolve function name at compile time --
    let func_name = match &args[0].kind {
        ExprKind::StringLiteral(name) => name.clone(),
        _ => panic!("function_exists() argument must be a string literal"),
    };

    // -- emit constant true/false based on whether function is known --
    if ctx.functions.contains_key(&func_name) {
        emitter.instruction("mov x0, #1");                                      // function exists → return true
    } else {
        emitter.instruction("mov x0, #0");                                      // function not found → return false
    }

    Some(PhpType::Bool)
}
