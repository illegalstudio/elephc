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
    emit_function_with_label(emitter, data, &label, &epilogue_label, sig, body, all_functions);
}

pub fn emit_closure(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
) {
    let epilogue_label = format!("{}_epilogue", label);
    emit_function_with_label(emitter, data, label, &epilogue_label, sig, body, all_functions);
}

fn emit_function_with_label(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    epilogue_label: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
) {

    let mut ctx = Context::new();
    ctx.return_label = Some(epilogue_label.to_string());
    ctx.functions = all_functions.clone();

    for (pname, pty) in &sig.params {
        ctx.alloc_var(pname, pty.clone());
    }

    collect_local_vars(body, &mut ctx, sig);

    let vars_size = ctx.stack_offset;
    let frame_size = super::align16(vars_size + 16);

    // -- function prologue: set up stack frame --
    emitter.raw(".align 2");
    emitter.label(&label);
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack for locals
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16));  // save caller's frame ptr & return addr
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));         // set new frame pointer

    // -- save parameters from registers to local stack slots --
    // ARM64 ABI: int/bool/array args in x0-x7, float args in d0-d7
    // Strings use two consecutive int registers (ptr + len)
    let mut int_reg_idx = 0usize;
    let mut float_reg_idx = 0usize;
    for (pname, pty) in &sig.params {
        let var = ctx.variables.get(pname).unwrap();
        let offset = var.stack_offset;
        match pty {
            PhpType::Bool | PhpType::Int => {
                emitter.comment(&format!("param ${} from x{}", pname, int_reg_idx));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", int_reg_idx, offset)); // save int/bool param
                int_reg_idx += 1;
            }
            PhpType::Float => {
                emitter.comment(&format!("param ${} from d{}", pname, float_reg_idx));
                emitter.instruction(&format!("stur d{}, [x29, #-{}]", float_reg_idx, offset)); // save float param
                float_reg_idx += 1;
            }
            PhpType::Str => {
                emitter.comment(&format!("param ${} from x{},x{}", pname, int_reg_idx, int_reg_idx + 1));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", int_reg_idx, offset)); // save string pointer
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", int_reg_idx + 1, offset - 8)); // save string length
                int_reg_idx += 2;
            }
            PhpType::Void => {}
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable => {
                emitter.comment(&format!("param ${} from x{}", pname, int_reg_idx));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", int_reg_idx, offset)); // save array/callable heap ptr
                int_reg_idx += 1;
            }
        }
    }

    // -- emit function body statements --
    for s in body {
        stmt::emit_stmt(s, emitter, &mut ctx, data);
    }

    // -- function epilogue: restore and return --
    emitter.label(&epilogue_label);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16));  // restore frame ptr & return addr
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
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
            StmtKind::Foreach { value_var, body, array, key_var, .. } => {
                if let Some(k) = key_var {
                    if !ctx.variables.contains_key(k) {
                        ctx.alloc_var(k, PhpType::Int);
                    }
                }
                if !ctx.variables.contains_key(value_var) {
                    let elem_ty = match infer_local_type(array, sig) {
                        PhpType::Array(t) => *t,
                        _ => PhpType::Int,
                    };
                    ctx.alloc_var(value_var, elem_ty);
                }
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_local_vars(body, ctx, sig);
                }
                if let Some(body) = default {
                    collect_local_vars(body, ctx, sig);
                }
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
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
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
        ExprKind::Negate(inner) => {
            let inner_ty = infer_local_type(inner, sig);
            if inner_ty == PhpType::Float { PhpType::Float } else { PhpType::Int }
        }
        ExprKind::Not(_) => PhpType::Bool,
        ExprKind::Ternary { then_expr, .. } => infer_local_type(then_expr, sig),
        ExprKind::BinaryOp { left, op, right } => {
            use crate::parser::ast::BinOp;
            match op {
                BinOp::Concat => PhpType::Str,
                BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt
                | BinOp::LtEq | BinOp::GtEq | BinOp::StrictEq
                | BinOp::StrictNotEq | BinOp::And | BinOp::Or => PhpType::Bool,
                BinOp::Div | BinOp::Pow => PhpType::Float,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Mod => {
                    let lt = infer_local_type(left, sig);
                    let rt = infer_local_type(right, sig);
                    if lt == PhpType::Float || rt == PhpType::Float {
                        PhpType::Float
                    } else {
                        PhpType::Int
                    }
                }
            }
        }
        ExprKind::FunctionCall { name, args } => {
            match name.as_str() {
                "floatval" | "floor" | "ceil" | "round" | "sqrt" | "pow" => PhpType::Float,
                "abs" => {
                    if !args.is_empty() {
                        let t = infer_local_type(&args[0], sig);
                        if t == PhpType::Float { PhpType::Float } else { PhpType::Int }
                    } else {
                        PhpType::Int
                    }
                }
                "min" | "max" => {
                    if args.len() >= 2 {
                        let t0 = infer_local_type(&args[0], sig);
                        let t1 = infer_local_type(&args[1], sig);
                        if t0 == PhpType::Float || t1 == PhpType::Float {
                            PhpType::Float
                        } else {
                            PhpType::Int
                        }
                    } else {
                        PhpType::Int
                    }
                }
                _ => PhpType::Int,
            }
        }
        ExprKind::Cast { target, .. } => {
            use crate::parser::ast::CastType;
            match target {
                CastType::Int => PhpType::Int,
                CastType::Float => PhpType::Float,
                CastType::String => PhpType::Str,
                CastType::Bool => PhpType::Bool,
                CastType::Array => PhpType::Array(Box::new(PhpType::Int)),
            }
        }
        ExprKind::Closure { .. } => PhpType::Callable,
        ExprKind::ClosureCall { .. } => PhpType::Int,
        _ => PhpType::Int,
    }
}
