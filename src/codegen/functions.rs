use std::collections::HashMap;

use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use super::stmt;
use crate::parser::ast::{ExprKind, StmtKind};
use crate::types::{FunctionSig, PhpType};

pub fn emit_function(
    emitter: &mut Emitter,
    data: &mut DataSection,
    name: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
) {
    let label = format!("_fn_{}", name);
    let epilogue_label = format!("_fn_{}_epilogue", name);

    let mut ctx = Context::new();
    ctx.return_label = Some(epilogue_label.clone());
    ctx.functions = all_functions.clone();

    for (pname, pty) in &sig.params {
        ctx.alloc_var(pname, pty.clone());
    }

    collect_local_vars(body, &mut ctx, sig);

    let vars_size = ctx.stack_offset;
    let frame_size = super::align16(vars_size + 16);

    emitter.raw(".align 2");
    emitter.label(&label);
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16));
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));

    let mut reg_idx = 0usize;
    for (pname, pty) in &sig.params {
        let var = ctx.variables.get(pname).unwrap();
        let offset = var.stack_offset;
        match pty {
            PhpType::Int => {
                emitter.comment(&format!("param ${} from x{}", pname, reg_idx));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", reg_idx, offset));
                reg_idx += 1;
            }
            PhpType::Str => {
                emitter.comment(&format!("param ${} from x{},x{}", pname, reg_idx, reg_idx + 1));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", reg_idx, offset));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", reg_idx + 1, offset - 8));
                reg_idx += 2;
            }
            PhpType::Void => {}
            PhpType::Array(_) => {
                emitter.comment(&format!("param ${} from x{}", pname, reg_idx));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", reg_idx, offset));
                reg_idx += 1;
            }
        }
    }

    for s in body {
        stmt::emit_stmt(s, emitter, &mut ctx, data);
    }

    emitter.label(&epilogue_label);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16));
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));
    emitter.instruction("ret");
    emitter.blank();
}

/// Pre-scan function body for variable assignments to allocate stack slots.
pub fn collect_local_vars(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &mut Context,
    sig: &FunctionSig,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                if !ctx.variables.contains_key(name) {
                    let ty = infer_local_type(value, sig);
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_local_vars(then_body, ctx, sig);
                for (_, body) in elseif_clauses {
                    collect_local_vars(body, ctx, sig);
                }
                if let Some(body) = else_body {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::Foreach { value_var, body, array, .. } => {
                if !ctx.variables.contains_key(value_var) {
                    let elem_ty = match infer_local_type(array, sig) {
                        PhpType::Array(t) => *t,
                        _ => PhpType::Int,
                    };
                    ctx.alloc_var(value_var, elem_ty);
                }
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::ArrayAssign { .. } | StmtKind::ArrayPush { .. } => {}
            StmtKind::DoWhile { body, .. } | StmtKind::While { body, .. } => {
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::For { init, update, body, .. } => {
                if let Some(s) = init {
                    collect_local_vars(&[*s.clone()], ctx, sig);
                }
                if let Some(s) = update {
                    collect_local_vars(&[*s.clone()], ctx, sig);
                }
                collect_local_vars(body, ctx, sig);
            }
            _ => {}
        }
    }
}

fn infer_local_type(
    expr: &crate::parser::ast::Expr,
    sig: &FunctionSig,
) -> PhpType {
    match &expr.kind {
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::ArrayLiteral(elems) => {
            let elem_ty = if elems.is_empty() {
                PhpType::Int
            } else {
                infer_local_type(&elems[0], sig)
            };
            PhpType::Array(Box::new(elem_ty))
        }
        ExprKind::ArrayAccess { array, .. } => match infer_local_type(array, sig) {
            PhpType::Array(t) => *t,
            _ => PhpType::Int,
        },
        ExprKind::Negate(_) => PhpType::Int,
        ExprKind::BinaryOp { op, .. } => match op {
            crate::parser::ast::BinOp::Concat => PhpType::Str,
            _ => PhpType::Int,
        },
        ExprKind::FunctionCall { .. } => PhpType::Int,
        _ => PhpType::Int,
    }
}
