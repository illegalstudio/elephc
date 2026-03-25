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
    emitter.comment("putenv()");
    // -- evaluate the KEY=VALUE string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- convert to C string and call putenv --
    emitter.instruction("bl __rt_cstr");                                        // null-terminate the string → x0=cstr
    emitter.instruction("bl _putenv");                                          // set env var, returns 0 on success
    // -- convert return value to bool (0=success → true=1) --
    emitter.instruction("cmp x0, #0");                                          // check if putenv returned 0 (success)
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if success, 0 if failure
    Some(PhpType::Bool)
}
