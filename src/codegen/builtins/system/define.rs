use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    _emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    // define("NAME", value) — store constant for compile-time resolution
    let const_name = match &args[0].kind {
        ExprKind::StringLiteral(s) => s.clone(),
        _ => panic!("define() first argument must be a string literal"),
    };

    let ty = match &args[1].kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        _ => PhpType::Int,
    };

    ctx.constants.insert(const_name, (args[1].kind.clone(), ty));

    Some(PhpType::Void)
}
