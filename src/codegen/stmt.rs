use super::abi;
use super::context::{Context, LoopLabels};
use crate::types::PhpType;
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
            match &ty {
                PhpType::Void => {
                    // null — don't print anything
                }
                PhpType::Bool => {
                    // echo false → nothing, echo true → "1"
                    let skip_label = ctx.next_label("echo_skip_false");
                    emitter.instruction(&format!("cbz x0, {}", skip_label));
                    abi::emit_write_stdout(emitter, &ty);
                    emitter.label(&skip_label);
                }
                PhpType::Int => {
                    // Runtime null check
                    let skip_label = ctx.next_label("echo_skip_null");
                    emitter.instruction("movz x9, #0xFFFE");
                    emitter.instruction("movk x9, #0xFFFF, lsl #16");
                    emitter.instruction("movk x9, #0xFFFF, lsl #32");
                    emitter.instruction("movk x9, #0x7FFF, lsl #48");
                    emitter.instruction("cmp x0, x9");
                    emitter.instruction(&format!("b.eq {}", skip_label));
                    abi::emit_write_stdout(emitter, &ty);
                    emitter.label(&skip_label);
                }
                PhpType::Float => {
                    abi::emit_write_stdout(emitter, &ty);
                }
                _ => {
                    abi::emit_write_stdout(emitter, &ty);
                }
            }
        }
        StmtKind::Assign { name, value } => {
            emitter.blank();
            emitter.comment(&format!("${} = ...", name));
            let ty = emit_expr(value, emitter, ctx, data);

            let var = ctx.variables.get(name).expect("variable not pre-allocated");
            let offset = var.stack_offset;

            abi::emit_store(emitter, &ty, offset);

            // Update variable type if it changed (e.g. int /= produces float)
            if var.ty != ty {
                ctx.variables.get_mut(name).unwrap().ty = ty;
            }
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
            let cond_ty = emit_expr(condition, emitter, ctx, data);
            super::expr::coerce_null_to_zero(emitter, &cond_ty);
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
                let cond_ty = emit_expr(cond, emitter, ctx, data);
                super::expr::coerce_null_to_zero(emitter, &cond_ty);
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
        StmtKind::ArrayAssign { array, index, value } => {
            emitter.blank();
            emitter.comment(&format!("${}[...] = ...", array));
            let var = ctx.variables.get(array).expect("undefined variable");
            let offset = var.stack_offset;
            let elem_ty = match &var.ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            // Load array pointer
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
            emitter.instruction("str x0, [sp, #-16]!"); // save array ptr
            // Evaluate index
            emit_expr(index, emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!"); // save index
            // Evaluate value
            let val_ty = emit_expr(value, emitter, ctx, data);
            // Pop index and array ptr
            emitter.instruction("ldr x9, [sp], #16"); // index
            emitter.instruction("ldr x10, [sp], #16"); // array ptr
            match &elem_ty {
                PhpType::Int => {
                    emitter.instruction("add x10, x10, #24");
                    emitter.instruction("str x0, [x10, x9, lsl #3]");
                }
                PhpType::Str => {
                    emitter.instruction("lsl x9, x9, #4");
                    emitter.instruction("add x10, x10, x9");
                    emitter.instruction("add x10, x10, #24");
                    emitter.instruction("str x1, [x10]");
                    emitter.instruction("str x2, [x10, #8]");
                }
                _ => {}
            }
            let _ = val_ty;
        }
        StmtKind::ArrayPush { array, value } => {
            emitter.blank();
            emitter.comment(&format!("${}[] = ...", array));
            let var = ctx.variables.get(array).expect("undefined variable");
            let offset = var.stack_offset;
            let elem_ty = match &var.ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            // Load array pointer
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
            emitter.instruction("str x0, [sp, #-16]!"); // save
            // Evaluate value
            emit_expr(value, emitter, ctx, data);
            emitter.instruction("ldr x9, [sp], #16"); // array ptr
            match &elem_ty {
                PhpType::Int => {
                    emitter.instruction("mov x1, x0");
                    emitter.instruction("mov x0, x9");
                    emitter.instruction("bl __rt_array_push_int");
                }
                PhpType::Str => {
                    emitter.instruction("mov x0, x9");
                    emitter.instruction("bl __rt_array_push_str");
                }
                _ => {}
            }
        }
        StmtKind::Foreach {
            array,
            value_var,
            body,
        } => {
            let loop_start = ctx.next_label("foreach_start");
            let loop_end = ctx.next_label("foreach_end");
            let loop_cont = ctx.next_label("foreach_cont");

            emitter.blank();
            emitter.comment("foreach");

            // Evaluate array
            let arr_ty = emit_expr(array, emitter, ctx, data);
            let elem_ty = match &arr_ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            // Save array ptr and length on stack
            emitter.instruction("str x0, [sp, #-16]!"); // array ptr
            emitter.instruction("ldr x9, [x0]"); // length
            emitter.instruction("str x9, [sp, #-16]!"); // length
            emitter.instruction("str xzr, [sp, #-16]!"); // index = 0

            emitter.label(&loop_start);
            // Load index and length
            emitter.instruction("ldr x0, [sp]"); // index
            emitter.instruction("ldr x1, [sp, #16]"); // length
            emitter.instruction("cmp x0, x1");
            emitter.instruction(&format!("b.ge {}", loop_end));

            // Load element at index into $value_var
            emitter.instruction("ldr x9, [sp, #32]"); // array ptr
            let val_var = ctx.variables.get(value_var).expect("foreach var");
            let val_offset = val_var.stack_offset;
            match &elem_ty {
                PhpType::Int => {
                    emitter.instruction("add x9, x9, #24");
                    emitter.instruction("ldr x0, [x9, x0, lsl #3]");
                    emitter.instruction(&format!("stur x0, [x29, #-{}]", val_offset));
                }
                PhpType::Str => {
                    emitter.instruction("lsl x10, x0, #4");
                    emitter.instruction("add x9, x9, x10");
                    emitter.instruction("add x9, x9, #24");
                    emitter.instruction("ldr x1, [x9]");
                    emitter.instruction("ldr x2, [x9, #8]");
                    emitter.instruction(&format!("stur x1, [x29, #-{}]", val_offset));
                    emitter.instruction(&format!("stur x2, [x29, #-{}]", val_offset - 8));
                }
                _ => {}
            }

            ctx.loop_stack.push(LoopLabels {
                continue_label: loop_cont.clone(),
                break_label: loop_end.clone(),
            });

            for s in body {
                emit_stmt(s, emitter, ctx, data);
            }

            ctx.loop_stack.pop();

            // Increment index
            emitter.label(&loop_cont);
            emitter.instruction("ldr x0, [sp]");
            emitter.instruction("add x0, x0, #1");
            emitter.instruction("str x0, [sp]");
            emitter.instruction(&format!("b {}", loop_start));

            emitter.label(&loop_end);
            // Clean up stack (3 x 16 bytes)
            emitter.instruction("add sp, sp, #48");
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
            let cond_ty = emit_expr(condition, emitter, ctx, data);
            super::expr::coerce_null_to_zero(emitter, &cond_ty);
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
            let cond_ty = emit_expr(condition, emitter, ctx, data);
            super::expr::coerce_null_to_zero(emitter, &cond_ty);
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
                let cond_ty = emit_expr(cond, emitter, ctx, data);
                super::expr::coerce_null_to_zero(emitter, &cond_ty);
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
        StmtKind::Include { .. } => {
            // Should have been resolved before codegen
            panic!("Unresolved include statement in codegen");
        }
    }
}
