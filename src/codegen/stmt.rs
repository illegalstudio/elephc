use super::abi;
use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use super::expr::emit_expr;
use crate::parser::ast::{Stmt, StmtKind};

pub fn emit_stmt(
    stmt: &Stmt,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match &stmt.kind {
        StmtKind::Echo(expr) => {
            emitter.blank();
            emitter.comment("echo");
            let ty = emit_expr(expr, emitter, ctx, data);
            abi::emit_write_stdout(emitter, &ty);
        }
        StmtKind::Assign { name, value } => {
            emitter.blank();
            emitter.comment(&format!("${} = ...", name));
            let ty = emit_expr(value, emitter, ctx, data);

            let var = ctx.variables.get(name).expect("variable not pre-allocated");
            let offset = var.stack_offset;

            abi::emit_store(emitter, &ty, offset);
        }
    }
}
