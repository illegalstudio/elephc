use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("json_last_error_msg()");
    abi::emit_call_label(emitter, "__rt_json_last_error_msg");
    Some(PhpType::Str)
}
