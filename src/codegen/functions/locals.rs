//! Purpose:
//! Collects all local and hidden frame slots required by function bodies before frame sizing.
//! Finds temporaries for named arguments, try handlers, closures, fibers, and statement lowering.
//!
//! Called from:
//! - `crate::codegen::functions` before prologue emission
//!
//! Key details:
//! - Any lowering path that introduces storage must be represented here before stack offsets are assigned.

use crate::codegen::context::{Context, HeapOwnership};
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind, InstanceOfTarget, StmtKind};
use crate::types::{
    merge_array_key_types, normalized_array_key_type, static_array_key_forces_hash_storage,
    FunctionSig, PhpType,
};
use super::types::{codegen_declared_type, codegen_static_type, infer_local_type};

/// Collects all local and hidden frame slots required by a function body before frame sizing.
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
                let needs_mixed_numeric_slot = runtime_numeric_result_may_widen(value, sig, ctx);
                if !ctx.variables.contains_key(name) {
                    let static_ty = infer_local_type(value, sig, Some(ctx));
                    let slot_ty = if needs_mixed_numeric_slot {
                        PhpType::Mixed
                    } else {
                        static_ty.codegen_repr()
                    };
                    ctx.alloc_var_with_static_type(name, slot_ty, static_ty);
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
                value_by_ref,
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
                            PhpType::Iterable
                            | PhpType::Object(_)
                            | PhpType::Mixed
                            | PhpType::Union(_) => PhpType::Mixed,
                            _ => PhpType::Int,
                        };
                        ctx.alloc_var(k, key_ty.codegen_repr());
                    }
                }
                if !ctx.variables.contains_key(value_var) {
                    let elem_ty = match &arr_ty {
                        PhpType::Array(t) => *t.clone(),
                        PhpType::AssocArray { value, .. } => *value.clone(),
                        PhpType::Iterable
                        | PhpType::Object(_)
                        | PhpType::Mixed
                        | PhpType::Union(_) => PhpType::Mixed,
                        _ => PhpType::Int,
                    };
                    if *value_by_ref {
                        ctx.alloc_var_with_static_type(
                            value_var,
                            elem_ty.codegen_repr(),
                            elem_ty.clone(),
                        );
                        ctx.update_var_type_static_and_ownership(
                            value_var,
                            elem_ty.codegen_repr(),
                            elem_ty.clone(),
                            HeapOwnership::borrowed_alias_for_type(&elem_ty),
                        );
                    } else {
                        ctx.alloc_var_with_static_type(value_var, elem_ty.codegen_repr(), elem_ty);
                    }
                }
                if *value_by_ref && !ctx.ref_params.contains(value_var) {
                    let flag_key =
                        Context::foreach_local_ref_cell_flag_key(value_var, stmt.span);
                    ctx.ensure_local_ref_cell_flag(flag_key, value_var);
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
            StmtKind::ArrayAssign {
                array,
                index,
                value,
            } => {
                collect_assignment_expr_vars(index, ctx, sig);
                collect_assignment_expr_vars(value, ctx, sig);
                refine_local_array_type_for_keyed_write(array, index, value, ctx, sig);
            }
            StmtKind::NestedArrayAssign { target, value } => {
                collect_assignment_expr_vars(target, ctx, sig);
                collect_assignment_expr_vars(value, ctx, sig);
            }
            StmtKind::PropertyArrayAssign { index, value, .. }
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

/// Refines the type of a local array variable when assigned with a keyed write.
/// When a numeric index is used on an array with `Never` element type, promotes
/// the array to an associative array. Does nothing if the variable is not tracked
/// or is not an array type.
fn refine_local_array_type_for_keyed_write(
    array: &str,
    index: &Expr,
    value: &Expr,
    ctx: &mut Context,
    sig: &FunctionSig,
) {
    let Some(existing_ty) = ctx.variables.get(array).map(|var| var.static_ty.clone()) else {
        return;
    };
    let PhpType::Array(existing_elem_ty) = existing_ty else {
        return;
    };

    let index_ty = infer_local_type(index, sig, Some(ctx));
    let normalized_key_ty = normalized_array_key_type(index, index_ty);
    let force_hash_for_empty_array = matches!(existing_elem_ty.as_ref(), PhpType::Never)
        && static_array_key_forces_hash_storage(index);
    if matches!(normalized_key_ty, PhpType::Int)
        && !force_hash_for_empty_array
    {
        return;
    }

    let value_ty = infer_local_type(value, sig, Some(ctx));
    let assoc_key_ty = if matches!(existing_elem_ty.as_ref(), PhpType::Never) {
        normalized_key_ty
    } else {
        merge_array_key_types(PhpType::Int, normalized_key_ty)
    };
    let assoc_value_ty = if matches!(existing_elem_ty.as_ref(), PhpType::Never) {
        value_ty
    } else if existing_elem_ty.as_ref() == &value_ty {
        *existing_elem_ty
    } else {
        PhpType::Mixed
    };
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(assoc_key_ty),
        value: Box::new(assoc_value_ty),
    };
    ctx.update_var_type_static_and_ownership(
        array,
        assoc_ty.clone(),
        assoc_ty.clone(),
        HeapOwnership::for_type(&assoc_ty),
    );
}

/// Recursively collects variables referenced within an assignment expression tree.
/// Handles named argument temps, conditional assignment temps, pipe temps, and closure
/// capture receiver temps. Visits all sub-expressions to ensure all referenced locals
/// are allocated before frame sizing.
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
        ExprKind::Pipe { value, callable } => {
            collect_assignment_expr_vars(value, ctx, sig);
            collect_assignment_expr_vars(callable, ctx, sig);
            let temp_name = crate::codegen::expr::calls::pipe_value_temp_name(expr.span);
            if !ctx.variables.contains_key(&temp_name) {
                let static_ty = infer_local_type(value, sig, Some(ctx));
                ctx.alloc_var_with_static_type(&temp_name, static_ty.codegen_repr(), static_ty);
            }
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
        ExprKind::NewObject { class_name, args } => {
            let call_span = args.first().map(|arg| arg.span).unwrap_or(expr.span);
            collect_named_constructor_call_temps(class_name.as_str(), call_span, args, ctx, sig);
            for arg in args {
                collect_assignment_expr_vars(arg, ctx, sig);
            }
        }
        ExprKind::ClosureCall { args, .. }
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
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_assignment_expr_vars(object, ctx, sig);
            collect_assignment_expr_vars(property, ctx, sig);
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
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => {
            collect_assignment_expr_vars(object, ctx, sig);
            if !matches!(&object.kind, ExprKind::Variable(_) | ExprKind::This) {
                let temp_name =
                    crate::codegen::expr::calls::first_class_method_receiver_temp_name(object.span);
                if !ctx.variables.contains_key(&temp_name) {
                    let static_ty = infer_local_type(object, sig, Some(ctx));
                    ctx.alloc_var_with_static_type(
                        &temp_name,
                        static_ty.codegen_repr(),
                        static_ty,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Infers the static type for a conditional assignment temporary variable.
/// Returns the type of the `default` branch for null coalesce, otherwise the value type.
/// This determines the slot type for the hidden temp that holds the result.
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

/// Returns true if a numeric expression may widen beyond i32 at runtime.
/// Used to decide whether an assignment target needs a Mixed slot instead of Int.
/// Only Add/Sub/Mul on Int-typed results can widen; other ops and float ops are safe.
fn runtime_numeric_result_may_widen(value: &Expr, sig: &FunctionSig, ctx: &Context) -> bool {
    matches!(
        value.kind,
        ExprKind::BinaryOp {
            op: BinOp::Add | BinOp::Sub | BinOp::Mul,
            ..
        }
    ) && infer_local_type(value, sig, Some(ctx)) == PhpType::Int
}

/// Allocates temporary variables for named arguments in builtin or extern calls.
/// Only allocates when the call has named args; skips externs without signatures or
/// calls using only positional arguments. The temps hold positional prefix arrays
/// and individual named argument values that must be materialized before the call.
fn collect_named_builtin_or_extern_call_temps(
    name: &str,
    call_span: crate::span::Span,
    args: &[Expr],
    ctx: &mut Context,
    current_sig: &FunctionSig,
) {
    let call_sig = if ctx.extern_functions.contains_key(name) {
        ctx.functions.get(name).cloned()
    } else {
        crate::types::builtin_call_sig(name)
    };
    let Some(call_sig) = call_sig else {
        return;
    };
    collect_named_call_temps_for_sig(&call_sig, call_span, args, ctx, current_sig);
}

/// Collects named constructor call temps for the surrounding analysis or metadata result.
fn collect_named_constructor_call_temps(
    class_name: &str,
    call_span: crate::span::Span,
    args: &[Expr],
    ctx: &mut Context,
    current_sig: &FunctionSig,
) {
    let Some(call_sig) = ctx
        .classes
        .get(class_name)
        .and_then(|class_info| class_info.methods.get("__construct"))
        .cloned()
    else {
        return;
    };
    collect_named_call_temps_for_sig(&call_sig, call_span, args, ctx, current_sig);
}

/// Collects named call temps for sig for the surrounding analysis or metadata result.
fn collect_named_call_temps_for_sig(
    call_sig: &FunctionSig,
    call_span: crate::span::Span,
    args: &[Expr],
    ctx: &mut Context,
    current_sig: &FunctionSig,
) {
    let assoc_spread_sources = assoc_spread_sources_for_locals(args, current_sig, ctx);
    let Ok(plan) = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        call_sig,
        args,
        call_span,
        crate::types::call_args::regular_param_count(call_sig),
        false,
        false,
        &assoc_spread_sources,
    ) else {
        return;
    };
    if !plan.has_named_args() {
        return;
    }

    if plan.has_spread_args() {
        let first_named_pos = plan.first_named_pos.unwrap_or(plan.source_args.len());
        let prefix_expr = plan
            .positional_prefix_expr(call_span)
            .unwrap_or_else(|| Expr::new(ExprKind::ArrayLiteral(Vec::new()), call_span));
        let prefix_name =
            crate::codegen::expr::calls::args::named_call_prefix_temp_name(call_span);
        if !ctx.variables.contains_key(&prefix_name) {
            let static_ty = infer_local_type(&prefix_expr, current_sig, Some(ctx));
            ctx.alloc_var_with_static_type(&prefix_name, static_ty.codegen_repr(), static_ty);
        }
        for source in &plan.source_values {
            if source.source_index() >= first_named_pos {
                collect_planned_call_value_temp(
                    call_sig,
                    call_span,
                    source.source_index(),
                    source.param_idx(),
                    source.expr(),
                    ctx,
                    current_sig,
                );
            }
        }
    } else {
        for source in &plan.source_values {
            collect_planned_call_value_temp(
                call_sig,
                call_span,
                source.source_index(),
                source.param_idx(),
                source.expr(),
                ctx,
                current_sig,
            );
        }
    }
}

/// Allocates a temporary for a single planned call argument value when needed.
/// A temp is allocated unless the argument is a ref param or a side-effect-free literal
/// (which can be reused inline without a frame slot). Uses the call signature to determine
/// whether the parameter is by-reference.
fn collect_planned_call_value_temp(
    call_sig: &FunctionSig,
    call_span: crate::span::Span,
    arg_idx: usize,
    param_idx: Option<usize>,
    value: &Expr,
    ctx: &mut Context,
    current_sig: &FunctionSig,
) {
    let is_ref = param_idx
        .and_then(|param_idx| call_sig.ref_params.get(param_idx))
        .copied()
        .unwrap_or(false);
    if !is_ref && !is_side_effect_free_literal(value) {
        collect_call_arg_temp(call_span, arg_idx, value, ctx, current_sig);
    }
}

/// Determines which call arguments are associative spread sources for local collection purposes.
/// Maps each argument to true if it is a spread of an AssocArray type, false otherwise.
/// Used to plan named argument temps in calls with mixed positional/spread arguments.
fn assoc_spread_sources_for_locals(
    args: &[Expr],
    current_sig: &FunctionSig,
    ctx: &Context,
) -> Vec<bool> {
    crate::types::call_args::expand_static_assoc_spread_args(args)
        .iter()
        .map(|arg| match &arg.kind {
            ExprKind::Spread(inner) => matches!(
                infer_local_type(inner, current_sig, Some(ctx)),
                PhpType::AssocArray { .. }
            ),
            _ => false,
        })
        .collect()
}

/// Returns true if an expression is a literal with no observable side effects.
/// Covers string, int, float, bool, and null literals. These can be used inline
/// without allocating a temporary frame slot since they are cheap to recreate.
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

/// Allocates a temporary variable for a single named call argument.
/// The temp holds the argument value at the call site so it can be passed as a
/// named parameter. Only allocated if not already present in the variable table.
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

/// Collects variables referenced in an instanceof target expression.
/// Only the Expr variant contains variables; class-name literals require no locals.
/// Delegates to `collect_assignment_expr_vars` for the expression case.
fn collect_instanceof_target_vars(
    target: &InstanceOfTarget,
    ctx: &mut Context,
    sig: &FunctionSig,
) {
    if let InstanceOfTarget::Expr(expr) = target {
        collect_assignment_expr_vars(expr, ctx, sig);
    }
}

/// Resolves a PHP catch type name to its codegen form.
/// Handles `self` and `parent` keywords by substituting the current class name;
/// other names are returned as-is. Used when allocating the catch exception variable.
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
