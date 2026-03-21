use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use super::expr::emit_expr;
use crate::parser::ast::Stmt;

pub fn emit_stmt(
    stmt: &Stmt,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match stmt {
        Stmt::Echo(expr) => {
            emitter.blank();
            emitter.comment("echo");
            emit_expr(expr, emitter, ctx, data);
            // sys_write(stdout, x1, x2)
            emitter.instruction("mov x0, #1");
            emitter.instruction("mov x16, #4");
            emitter.instruction("svc #0x80");
        }
        Stmt::Assign { .. } => {
            // TODO: implement in Phase 2
        }
    }
}
