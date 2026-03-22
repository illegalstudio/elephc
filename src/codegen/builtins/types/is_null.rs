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
    emitter.comment("is_null()");
    // -- check if value equals the null sentinel (0x7FFFFFFFFFFFFFFFE) --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("movz x9, #0xFFFE");                            // load null sentinel bits [15:0]
    emitter.instruction("movk x9, #0xFFFF, lsl #16");                   // load null sentinel bits [31:16]
    emitter.instruction("movk x9, #0xFFFF, lsl #32");                   // load null sentinel bits [47:32]
    emitter.instruction("movk x9, #0x7FFF, lsl #48");                   // load null sentinel bits [63:48]
    emitter.instruction("cmp x0, x9");                                  // compare value against null sentinel
    emitter.instruction("cset x0, eq");                                 // x0 = 1 if value is null, 0 otherwise
    Some(PhpType::Bool)
}
