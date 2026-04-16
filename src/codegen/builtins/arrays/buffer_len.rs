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
    let buf_ty = emit_expr(&args[0], emitter, ctx, data);
    if !matches!(buf_ty, PhpType::Buffer(_)) {
        emitter.comment("WARNING: buffer_len() received a non-buffer argument");
    }
    abi::emit_call_label(emitter, "__rt_buffer_len");                           // load the logical element count from the buffer header through the target-aware runtime helper
    Some(PhpType::Int)
}
