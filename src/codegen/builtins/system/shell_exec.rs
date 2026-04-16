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
    emitter.comment("shell_exec()");
    // -- evaluate command string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- call runtime to execute command and capture output --
    abi::emit_call_label(emitter, "__rt_shell_exec");                           // execute command via the target-aware shell helper → ptr/len result regs
    Some(PhpType::Str)
}
