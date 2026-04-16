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
    emitter.comment("microtime(true)");
    abi::emit_call_label(emitter, "__rt_microtime");                            // call the target-aware runtime helper that returns the current Unix timestamp with microsecond precision in the native float result register
    Some(PhpType::Float)
}
