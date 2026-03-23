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
    emitter.comment("exit()");
    if let Some(arg) = args.first() {
        emit_expr(arg, emitter, ctx, data);
    } else {
        // -- default exit code when no argument given --
        emitter.instruction("mov x0, #0");                                      // set exit code to 0 (success)
    }
    // -- invoke macOS kernel exit syscall --
    emitter.instruction("mov x16, #1");                                         // load syscall number 1 (SYS_exit) into x16
    emitter.instruction("svc #0x80");                                           // trigger supervisor call to execute kernel exit

    Some(PhpType::Void)
}
