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
    emitter.comment("ord()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- return ASCII value of first character (or 0 for empty string) --
    let empty_label = ctx.next_label("ord_empty");
    let done_label = ctx.next_label("ord_done");
    emitter.instruction(&format!("cbz x2, {empty_label}"));                     // if string length is 0, return 0
    emitter.instruction("ldrb w0, [x1]");                                       // load first byte from string ptr as unsigned int
    emitter.instruction(&format!("b {done_label}"));                            // skip the empty-string fallback
    emitter.label(&empty_label);
    emitter.instruction("mov x0, #0");                                          // empty string → return 0
    emitter.label(&done_label);

    Some(PhpType::Int)
}
