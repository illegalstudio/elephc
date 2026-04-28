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
    emitter.comment("fileperms()");
    emit_expr(&args[0], emitter, ctx, data);                                    // evaluate path string → x1=ptr, x2=len
    abi::emit_call_label(emitter, "__rt_fileperms");                            // call stat() and extract st_mode permission bits
    Some(PhpType::Int)
}
