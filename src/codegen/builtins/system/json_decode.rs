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
    emitter.comment("json_decode()");

    // -- evaluate the JSON string argument --
    emit_expr(&args[0], emitter, ctx, data);
    // x1=string ptr, x2=string len

    // -- call runtime to decode JSON string value --
    abi::emit_call_label(emitter, "__rt_json_decode");                         // decode the JSON string into a borrowed or concat-backed string slice for the active target ABI

    Some(PhpType::Str)
}
