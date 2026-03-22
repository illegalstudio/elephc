use super::abi;
use super::context::{Context, LoopLabels};
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
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let end_label = ctx.next_label("if_end");

            // Evaluate condition
            emitter.blank();
            emitter.comment("if");
            emit_expr(condition, emitter, ctx, data);
            let mut next_label = ctx.next_label("if_else");
            emitter.instruction("cmp x0, #0");
            emitter.instruction(&format!("b.eq {}", next_label));

            // then body
            for s in then_body {
                emit_stmt(s, emitter, ctx, data);
            }
            emitter.instruction(&format!("b {}", end_label));

            // elseif clauses
            for (cond, body) in elseif_clauses {
                emitter.label(&next_label);
                emitter.comment("elseif");
                emit_expr(cond, emitter, ctx, data);
                next_label = ctx.next_label("if_else");
                emitter.instruction("cmp x0, #0");
                emitter.instruction(&format!("b.eq {}", next_label));

                for s in body {
                    emit_stmt(s, emitter, ctx, data);
                }
                emitter.instruction(&format!("b {}", end_label));
            }

            // else body (or fall-through label)
            emitter.label(&next_label);
            if let Some(body) = else_body {
                emitter.comment("else");
                for s in body {
                    emit_stmt(s, emitter, ctx, data);
                }
            }

            emitter.label(&end_label);
        }
        StmtKind::DoWhile { body, condition } => {
            let loop_start = ctx.next_label("dowhile_start");
            let loop_end = ctx.next_label("dowhile_end");
            let loop_cond = ctx.next_label("dowhile_cond");

            emitter.blank();
            emitter.comment("do...while");
            emitter.label(&loop_start);

            ctx.loop_stack.push(LoopLabels {
                continue_label: loop_cond.clone(),
                break_label: loop_end.clone(),
            });

            for s in body {
                emit_stmt(s, emitter, ctx, data);
            }

            ctx.loop_stack.pop();

            emitter.label(&loop_cond);
            emit_expr(condition, emitter, ctx, data);
            emitter.instruction("cmp x0, #0");
            emitter.instruction(&format!("b.ne {}", loop_start));
            emitter.label(&loop_end);
        }
        StmtKind::While { condition, body } => {
            let loop_start = ctx.next_label("while_start");
            let loop_end = ctx.next_label("while_end");

            emitter.blank();
            emitter.comment("while");
            emitter.label(&loop_start);
            emit_expr(condition, emitter, ctx, data);
            emitter.instruction("cmp x0, #0");
            emitter.instruction(&format!("b.eq {}", loop_end));

            ctx.loop_stack.push(LoopLabels {
                continue_label: loop_start.clone(),
                break_label: loop_end.clone(),
            });

            for s in body {
                emit_stmt(s, emitter, ctx, data);
            }

            ctx.loop_stack.pop();

            emitter.instruction(&format!("b {}", loop_start));
            emitter.label(&loop_end);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let loop_start = ctx.next_label("for_start");
            let loop_continue = ctx.next_label("for_cont");
            let loop_end = ctx.next_label("for_end");

            emitter.blank();
            emitter.comment("for");

            // Init
            if let Some(s) = init {
                emit_stmt(s, emitter, ctx, data);
            }

            emitter.label(&loop_start);

            // Condition
            if let Some(cond) = condition {
                emit_expr(cond, emitter, ctx, data);
                emitter.instruction("cmp x0, #0");
                emitter.instruction(&format!("b.eq {}", loop_end));
            }

            ctx.loop_stack.push(LoopLabels {
                continue_label: loop_continue.clone(),
                break_label: loop_end.clone(),
            });

            // Body
            for s in body {
                emit_stmt(s, emitter, ctx, data);
            }

            ctx.loop_stack.pop();

            // Update + loop back
            emitter.label(&loop_continue);
            if let Some(s) = update {
                emit_stmt(s, emitter, ctx, data);
            }
            emitter.instruction(&format!("b {}", loop_start));
            emitter.label(&loop_end);
        }
        StmtKind::Break => {
            let labels = ctx.loop_stack.last().expect("break outside loop");
            emitter.instruction(&format!("b {}", labels.break_label));
        }
        StmtKind::FunctionDecl { .. } => {
            // Emitted separately in codegen/mod.rs
        }
        StmtKind::Return(expr) => {
            emitter.blank();
            emitter.comment("return");
            if let Some(e) = expr {
                emit_expr(e, emitter, ctx, data);
            }
            if let Some(label) = &ctx.return_label {
                emitter.instruction(&format!("b {}", label));
            }
        }
        StmtKind::ExprStmt(expr) => {
            emitter.blank();
            emit_expr(expr, emitter, ctx, data);
            // result discarded
        }
        StmtKind::Continue => {
            let labels = ctx.loop_stack.last().expect("continue outside loop");
            emitter.instruction(&format!("b {}", labels.continue_label));
        }
    }
}
