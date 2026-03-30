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
    emitter.comment("gettype()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    let type_str = match &ty {
        PhpType::Int => "integer",
        PhpType::Float => "double",
        PhpType::Str => "string",
        PhpType::Bool => "boolean",
        PhpType::Void => "NULL",
        PhpType::Mixed => "mixed",
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array",
        PhpType::Callable => "callable",
        PhpType::Object(_) => "object",
        PhpType::Pointer(_) => "pointer",
    };
    // -- load pointer and length of type name string --
    let (label, len) = data.add_string(type_str.as_bytes());
    emitter.instruction(&format!("adrp x1, {}@PAGE", label));                   // load page address of type name string
    emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));             // add page offset to get full address
    emitter.instruction(&format!("mov x2, #{}", len));                          // load string length into x2
    Some(PhpType::Str)
}
