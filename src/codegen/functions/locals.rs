use crate::codegen::context::Context;
use crate::parser::ast::{ExprKind, StmtKind};
use crate::types::{FunctionSig, PhpType};

use super::types::{codegen_declared_type, infer_local_type};

pub fn collect_local_vars(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &mut Context,
    sig: &FunctionSig,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                if !ctx.variables.contains_key(name) {
                    let ty = infer_local_type(value, sig, Some(ctx)).codegen_repr();
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::TypedAssign {
                type_expr, name, ..
            } => {
                if !ctx.variables.contains_key(name) {
                    let ty = codegen_declared_type(type_expr, ctx).codegen_repr();
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::Global { vars } => {
                for name in vars {
                    if !ctx.variables.contains_key(name) {
                        ctx.alloc_var(name, PhpType::Int);
                    }
                }
            }
            StmtKind::StaticVar { name, init } => {
                if !ctx.variables.contains_key(name) {
                    let ty = infer_local_type(init, sig, Some(ctx)).codegen_repr();
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_local_vars(then_body, ctx, sig);
                for (_, body) in elseif_clauses {
                    collect_local_vars(body, ctx, sig);
                }
                if let Some(body) = else_body {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_local_vars(try_body, ctx, sig);
                for catch_clause in catches {
                    let catch_type_name = resolve_codegen_catch_type_name(
                        ctx,
                        catch_clause
                            .exception_types
                            .first()
                            .map(|name| name.as_str())
                            .unwrap_or("Throwable"),
                    );
                    if let Some(variable) = &catch_clause.variable {
                        if !ctx.variables.contains_key(variable) {
                            ctx.alloc_var(variable, PhpType::Object(catch_type_name));
                        }
                    }
                    collect_local_vars(&catch_clause.body, ctx, sig);
                }
                if let Some(body) = finally_body {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::Foreach {
                value_var,
                body,
                array,
                key_var,
                ..
            } => {
                let arr_ty = infer_local_type(array, sig, Some(ctx));
                if let Some(k) = key_var {
                    if !ctx.variables.contains_key(k) {
                        let key_ty = if matches!(&arr_ty, PhpType::AssocArray { .. }) {
                            PhpType::Str
                        } else {
                            PhpType::Int
                        };
                        ctx.alloc_var(k, key_ty.codegen_repr());
                    }
                }
                if !ctx.variables.contains_key(value_var) {
                    let elem_ty = match &arr_ty {
                        PhpType::Array(t) => *t.clone(),
                        PhpType::AssocArray { value, .. } => *value.clone(),
                        _ => PhpType::Int,
                    };
                    ctx.alloc_var(value_var, elem_ty.codegen_repr());
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
            StmtKind::ConstDecl { .. } => {}
            StmtKind::ListUnpack { vars, value, .. } => {
                let elem_ty = match infer_local_type(value, sig, Some(ctx)) {
                    PhpType::Array(t) => *t,
                    _ => PhpType::Int,
                };
                for var in vars {
                    if !ctx.variables.contains_key(var) {
                        ctx.alloc_var(var, elem_ty.codegen_repr());
                    }
                }
            }
            StmtKind::ArrayAssign { .. }
            | StmtKind::ArrayPush { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. } => {}
            StmtKind::PropertyAssign { value, .. } => {
                if let ExprKind::Variable(_) = &value.kind {
                } else {
                }
            }
            StmtKind::PropertyArrayPush { .. } | StmtKind::PropertyArrayAssign { .. } => {}
            StmtKind::DoWhile { body, .. } | StmtKind::While { body, .. } => {
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::For {
                init, update, body, ..
            } => {
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

fn resolve_codegen_catch_type_name(ctx: &Context, raw_name: &str) -> String {
    match raw_name {
        "self" => ctx
            .current_class
            .clone()
            .unwrap_or_else(|| raw_name.to_string()),
        "parent" => ctx
            .current_class
            .as_ref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.clone())
            .unwrap_or_else(|| raw_name.to_string()),
        _ => raw_name.to_string(),
    }
}
