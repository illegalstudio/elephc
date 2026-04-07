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
    emitter.comment("fclose()");
    // -- evaluate fd argument --
    emit_expr(&args[0], emitter, ctx, data);
    // -- invoke close syscall --
    let success = ctx.next_label("fclose_ok");
    let done = ctx.next_label("fclose_done");
    emitter.syscall(6);
    emitter.instruction("cmp x0, #0");                                          // check if close succeeded
    emitter.instruction(&format!("b.eq {}", success));                          // branch if no error
    emitter.instruction("mov x0, #0");                                          // return false on error
    emitter.instruction(&format!("b {}", done));                                // skip success path
    emitter.label(&success);
    emitter.instruction("mov x0, #1");                                          // return true on success
    emitter.label(&done);
    Some(PhpType::Bool)
}
