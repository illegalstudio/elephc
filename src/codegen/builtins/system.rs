use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "exit" | "die" => {
            emitter.comment("exit()");
            if let Some(arg) = args.first() {
                emit_expr(arg, emitter, ctx, data);
            } else {
                emitter.instruction("mov x0, #0");
            }
            emitter.instruction("mov x16, #1");
            emitter.instruction("svc #0x80");
            Some(PhpType::Void)
        }
        _ => None,
    }
}
