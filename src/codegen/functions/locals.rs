use crate::codegen::context::Context;
use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, StmtKind};
use crate::types::{FunctionSig, PhpType};

use super::types::{codegen_declared_type, codegen_static_type, infer_local_type};

pub fn collect_local_vars(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &mut Context,
    sig: &FunctionSig,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Synthetic(stmts) => {
                collect_local_vars(stmts, ctx, sig);
            }
            StmtKind::IncludeOnceGuard { body, .. } => {
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::IncludeOnceMark { .. } => {}
            StmtKind::Assign { name, value } => {
                collect_assignment_expr_vars(value, ctx, sig);
                if !ctx.variables.contains_key(name) {
                    let static_ty = infer_local_type(value, sig, Some(ctx));
                    ctx.alloc_var_with_static_type(name, static_ty.codegen_repr(), static_ty);
                }
            }
            StmtKind::TypedAssign {
                type_expr,
                name,
                value,
            } => {
                collect_assignment_expr_vars(value, ctx, sig);
                if !ctx.variables.contains_key(name) {
                    let static_ty = codegen_static_type(type_expr, ctx);
                    let ty = codegen_declared_type(type_expr, ctx).codegen_repr();
                    ctx.alloc_var_with_static_type(name, ty, static_ty);
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
                collect_assignment_expr_vars(init, ctx, sig);
                if !ctx.variables.contains_key(name) {
                    let static_ty = infer_local_type(init, sig, Some(ctx));
                    ctx.alloc_var_with_static_type(name, static_ty.codegen_repr(), static_ty);
                }
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_assignment_expr_vars(condition, ctx, sig);
                collect_local_vars(then_body, ctx, sig);
                for (condition, body) in elseif_clauses {
                    collect_assignment_expr_vars(condition, ctx, sig);
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
                        let key_ty = match &arr_ty {
                            PhpType::AssocArray { key, .. } => *key.clone(),
                            PhpType::Iterable | PhpType::Object(_) => PhpType::Mixed,
                            _ => PhpType::Int,
                        };
                        ctx.alloc_var(k, key_ty.codegen_repr());
                    }
                }
                if !ctx.variables.contains_key(value_var) {
                    let elem_ty = match &arr_ty {
                        PhpType::Array(t) => *t.clone(),
                        PhpType::AssocArray { value, .. } => *value.clone(),
                        PhpType::Iterable | PhpType::Object(_) => PhpType::Mixed,
                        _ => PhpType::Int,
                    };
                    ctx.alloc_var_with_static_type(value_var, elem_ty.codegen_repr(), elem_ty);
                }
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (patterns, body) in cases {
                    for pattern in patterns {
                        collect_assignment_expr_vars(pattern, ctx, sig);
                    }
                    collect_local_vars(body, ctx, sig);
                }
                if let Some(body) = default {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::ConstDecl { value, .. } => {
                collect_assignment_expr_vars(value, ctx, sig);
            }
            StmtKind::ListUnpack { vars, value, .. } => {
                collect_assignment_expr_vars(value, ctx, sig);
                let elem_ty = match infer_local_type(value, sig, Some(ctx)) {
                    PhpType::Array(t) => *t,
                    _ => PhpType::Int,
                };
                for var in vars {
                    if !ctx.variables.contains_key(var) {
                        ctx.alloc_var_with_static_type(var, elem_ty.codegen_repr(), elem_ty.clone());
                    }
                }
            }
            StmtKind::PropertyAssign { value, .. } => {
                collect_assignment_expr_vars(value, ctx, sig);
                if let ExprKind::Variable(_) = &value.kind {
                } else {
                }
            }
            StmtKind::DoWhile { body, condition } | StmtKind::While { body, condition } => {
                collect_assignment_expr_vars(condition, ctx, sig);
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
                ..
            } => {
                if let Some(s) = init {
                    collect_local_vars(&[*s.clone()], ctx, sig);
                }
                if let Some(condition) = condition {
                    collect_assignment_expr_vars(condition, ctx, sig);
                }
                if let Some(s) = update {
                    collect_local_vars(&[*s.clone()], ctx, sig);
                }
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::Return(Some(expr))
            | StmtKind::Include { path: expr, .. } => {
                collect_assignment_expr_vars(expr, ctx, sig);
            }
            StmtKind::ArrayAssign { index, value, .. }
            | StmtKind::PropertyArrayAssign { index, value, .. }
            | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                collect_assignment_expr_vars(index, ctx, sig);
                collect_assignment_expr_vars(value, ctx, sig);
            }
            StmtKind::ArrayPush { value, .. }
            | StmtKind::StaticPropertyAssign { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. }
            | StmtKind::PropertyArrayPush { value, .. } => {
                collect_assignment_expr_vars(value, ctx, sig);
            }
            _ => {}
        }
    }
}

fn collect_assignment_expr_vars(expr: &Expr, ctx: &mut Context, sig: &FunctionSig) {
    match &expr.kind {
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => {
            collect_local_vars(prelude, ctx, sig);
            collect_assignment_expr_vars(value, ctx, sig);
            if let Some(temp_name) = conditional_value_temp {
                if !ctx.variables.contains_key(temp_name) {
                    let static_ty = infer_conditional_assignment_temp_type(value, sig, ctx);
                    ctx.alloc_var_with_static_type(
                        temp_name,
                        static_ty.codegen_repr(),
                        static_ty,
                    );
                }
            }
            if let ExprKind::Variable(name) = &target.kind {
                if !ctx.variables.contains_key(name) {
                    let static_ty = infer_local_type(value, sig, Some(ctx));
                    ctx.alloc_var_with_static_type(name, static_ty.codegen_repr(), static_ty);
                }
            } else {
                collect_assignment_expr_vars(target, ctx, sig);
            }
            if let Some(result_target) = result_target {
                collect_assignment_expr_vars(result_target, ctx, sig);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_assignment_expr_vars(left, ctx, sig);
            collect_assignment_expr_vars(right, ctx, sig);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_assignment_expr_vars(value, ctx, sig);
            collect_instanceof_target_vars(target, ctx, sig);
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Spread(value)
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. } => collect_assignment_expr_vars(value, ctx, sig),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_assignment_expr_vars(value, ctx, sig);
            collect_assignment_expr_vars(default, ctx, sig);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_assignment_expr_vars(condition, ctx, sig);
            collect_assignment_expr_vars(then_expr, ctx, sig);
            collect_assignment_expr_vars(else_expr, ctx, sig);
        }
        ExprKind::FunctionCall { name, args } => {
            collect_named_builtin_or_extern_call_temps(name.as_str(), expr.span, args, ctx, sig);
            for arg in args {
                collect_assignment_expr_vars(arg, ctx, sig);
            }
        }
        ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                collect_assignment_expr_vars(arg, ctx, sig);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_assignment_expr_vars(callee, ctx, sig);
            for arg in args {
                collect_assignment_expr_vars(arg, ctx, sig);
            }
        }
        ExprKind::ArrayLiteral(elems) => {
            for elem in elems {
                collect_assignment_expr_vars(elem, ctx, sig);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                collect_assignment_expr_vars(key, ctx, sig);
                collect_assignment_expr_vars(value, ctx, sig);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_assignment_expr_vars(subject, ctx, sig);
            for (patterns, result) in arms {
                for pattern in patterns {
                    collect_assignment_expr_vars(pattern, ctx, sig);
                }
                collect_assignment_expr_vars(result, ctx, sig);
            }
            if let Some(default) = default {
                collect_assignment_expr_vars(default, ctx, sig);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_assignment_expr_vars(array, ctx, sig);
            collect_assignment_expr_vars(index, ctx, sig);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_assignment_expr_vars(object, ctx, sig);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_assignment_expr_vars(object, ctx, sig);
            for arg in args {
                collect_assignment_expr_vars(arg, ctx, sig);
            }
        }
        ExprKind::BufferNew { len, .. } => collect_assignment_expr_vars(len, ctx, sig),
        ExprKind::Closure {
            params,
            captures: _,
            ..
        } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    collect_assignment_expr_vars(default, ctx, sig);
                }
            }
        }
        _ => {}
    }
}

fn infer_conditional_assignment_temp_type(
    value: &Expr,
    sig: &FunctionSig,
    ctx: &Context,
) -> PhpType {
    match &value.kind {
        ExprKind::NullCoalesce { default, .. } => infer_local_type(default, sig, Some(ctx)),
        _ => infer_local_type(value, sig, Some(ctx)),
    }
}

fn collect_named_builtin_or_extern_call_temps(
    name: &str,
    call_span: crate::span::Span,
    args: &[Expr],
    ctx: &mut Context,
    current_sig: &FunctionSig,
) {
    let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
    let args = expanded_args.as_slice();

    if !crate::codegen::expr::calls::args::has_named_args(args) {
        return;
    }
    let Some(call_sig) = crate::types::builtin_call_sig(name)
        .or_else(|| ctx.extern_functions.contains_key(name).then(|| ctx.functions.get(name).cloned()).flatten())
    else {
        return;
    };
    let regular_param_count =
        crate::codegen::expr::calls::args::regular_param_count(Some(&call_sig), args.len());

    if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        let first_named_pos = args
            .iter()
            .position(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
            .unwrap_or(args.len());
        let prefix_args = args[..first_named_pos].to_vec();
        let prefix_span = prefix_args
            .first()
            .map(|arg| arg.span)
            .unwrap_or(call_span);
        let prefix_expr = if let [arg] = prefix_args.as_slice() {
            if let ExprKind::Spread(inner) = &arg.kind {
                (**inner).clone()
            } else {
                Expr::new(ExprKind::ArrayLiteral(prefix_args), prefix_span)
            }
        } else {
            Expr::new(ExprKind::ArrayLiteral(prefix_args), prefix_span)
        };
        let prefix_name =
            crate::codegen::expr::calls::args::named_call_prefix_temp_name(call_span);
        if !ctx.variables.contains_key(&prefix_name) {
            let static_ty = infer_local_type(&prefix_expr, current_sig, Some(ctx));
            ctx.alloc_var_with_static_type(&prefix_name, static_ty.codegen_repr(), static_ty);
        }
        for (idx, arg) in args.iter().enumerate().skip(first_named_pos) {
            if let ExprKind::NamedArg { name, value } = &arg.kind {
                collect_named_call_value_temp(
                    &call_sig,
                    regular_param_count,
                    call_span,
                    idx,
                    name,
                    value,
                    ctx,
                    current_sig,
                );
            }
        }
    } else {
        let mut positional_idx = 0usize;
        for (idx, arg) in args.iter().enumerate() {
            match &arg.kind {
                ExprKind::NamedArg { name, value } => {
                    collect_named_call_value_temp(
                        &call_sig,
                        regular_param_count,
                        call_span,
                        idx,
                        name,
                        value,
                        ctx,
                        current_sig,
                    );
                }
                _ => {
                    let is_ref = call_sig
                        .ref_params
                        .get(positional_idx)
                        .copied()
                        .unwrap_or(false);
                    if !is_ref && !is_side_effect_free_literal(arg) {
                        collect_call_arg_temp(call_span, idx, arg, ctx, current_sig);
                    }
                    positional_idx += 1;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_named_call_value_temp(
    call_sig: &FunctionSig,
    regular_param_count: usize,
    call_span: crate::span::Span,
    arg_idx: usize,
    name: &str,
    value: &Expr,
    ctx: &mut Context,
    current_sig: &FunctionSig,
) {
    let is_ref = call_sig
        .params
        .iter()
        .take(regular_param_count)
        .position(|(param_name, _)| param_name == name)
        .and_then(|param_idx| call_sig.ref_params.get(param_idx))
        .copied()
        .unwrap_or(false);
    if !is_ref && !is_side_effect_free_literal(value) {
        collect_call_arg_temp(call_span, arg_idx, value, ctx, current_sig);
    }
}

fn is_side_effect_free_literal(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
    )
}

fn collect_call_arg_temp(
    call_span: crate::span::Span,
    arg_idx: usize,
    value: &Expr,
    ctx: &mut Context,
    current_sig: &FunctionSig,
) {
    let temp_name = crate::codegen::expr::calls::args::named_call_arg_temp_name(call_span, arg_idx);
    if !ctx.variables.contains_key(&temp_name) {
        let static_ty = infer_local_type(value, current_sig, Some(ctx));
        ctx.alloc_var_with_static_type(&temp_name, static_ty.codegen_repr(), static_ty);
    }
}

fn collect_instanceof_target_vars(
    target: &InstanceOfTarget,
    ctx: &mut Context,
    sig: &FunctionSig,
) {
    if let InstanceOfTarget::Expr(expr) = target {
        collect_assignment_expr_vars(expr, ctx, sig);
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
