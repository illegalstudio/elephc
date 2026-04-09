use crate::codegen::context::{Context, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::stmt::emit_stmt;
use crate::parser::ast::{Expr, Stmt};
use crate::types::PhpType;

pub(super) fn emit_switch_stmt(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let switch_end = ctx.next_label("switch_end");
    emitter.blank();
    emitter.comment("switch");

    let subj_ty = emit_expr(subject, emitter, ctx, data);
    match &subj_ty {
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save string subject
        }
        _ => {
            emitter.instruction("str x0, [sp, #-16]!");                         // save int/bool subject
        }
    }

    let mut body_labels = Vec::new();
    for (i, (values, _)) in cases.iter().enumerate() {
        let body_label = ctx.next_label(&format!("case_{}", i));
        for val in values {
            let val_ty = emit_expr(val, emitter, ctx, data);
            match &subj_ty {
                PhpType::Str => {
                    emitter.instruction("mov x3, x1");                          // pattern ptr
                    emitter.instruction("mov x4, x2");                          // pattern len
                    emitter.instruction("ldp x1, x2, [sp]");                    // peek subject string
                    emitter.instruction("bl __rt_str_eq");                      // compare -> x0=1 if equal
                }
                _ => {
                    emitter.instruction("ldr x9, [sp]");                        // peek subject
                    emitter.instruction("cmp x9, x0");                          // compare
                    emitter.instruction("cset x0, eq");                         // x0=1 if equal
                }
            }
            emitter.instruction(&format!("cbnz x0, {}", body_label));           // jump to case body if match
            let _ = val_ty;
        }
        body_labels.push(body_label);
    }

    let default_label = ctx.next_label("switch_default");
    if default.is_some() {
        emitter.instruction(&format!("b {}", default_label));                   // jump to default case
    } else {
        emitter.instruction(&format!("b {}", switch_end));                      // jump to end (no default)
    }

    ctx.loop_stack.push(LoopLabels {
        continue_label: switch_end.clone(),
        break_label: switch_end.clone(),
        sp_adjust: 16,
    });
    for (i, (_, body)) in cases.iter().enumerate() {
        emitter.label(&body_labels[i]);
        for s in body {
            emit_stmt(s, emitter, ctx, data);
        }
    }

    if let Some(def_body) = default {
        emitter.label(&default_label);
        for s in def_body {
            emit_stmt(s, emitter, ctx, data);
        }
    }

    ctx.loop_stack.pop();
    emitter.label(&switch_end);
    emitter.instruction("add sp, sp, #16");                                     // pop saved subject
}
