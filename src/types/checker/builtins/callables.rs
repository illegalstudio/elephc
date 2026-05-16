//! Purpose:
//! Type-checks the callables PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

fn validate_call_user_func_array_ref_args(
    sig: &crate::types::FunctionSig,
    arg_array: &Expr,
    span: crate::span::Span,
) -> Result<(), CompileError> {
    if !sig.ref_params.iter().any(|is_ref| *is_ref) {
        return Ok(());
    }
    if matches!(arg_array.kind, ExprKind::ArrayLiteral(_)) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        "call_user_func_array() requires a literal argument array when the callback has pass-by-reference parameters",
    ))
}

fn dummy_arg_for_array_scalar_elem(arr_ty: &PhpType, span: crate::span::Span) -> Expr {
    let elem_ty = match arr_ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref(),
        PhpType::AssocArray { value, .. } => value.as_ref(),
        _ => &PhpType::Int,
    };
    match elem_ty {
        PhpType::Str => Expr::new(ExprKind::StringLiteral(String::new()), span),
        PhpType::Float => Expr::new(ExprKind::FloatLiteral(0.0), span),
        PhpType::Bool => Expr::new(ExprKind::BoolLiteral(false), span),
        _ => Expr::new(ExprKind::IntLiteral(0), span),
    }
}

fn check_callback_builtin_call(
    checker: &mut Checker,
    callback: &Expr,
    callback_args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
    label: &str,
) -> Result<PhpType, CompileError> {
    if checker.expr_call_complex_callee_needs_runtime_capture(callback) {
        return Err(CompileError::new(
            callback.span,
            &format!(
                "{} does not support complex expressions that select captured callables at runtime",
                label
            ),
        ));
    }

    if let ExprKind::FirstClassCallable(target) = &callback.kind {
        let sig = checker.specialize_first_class_callable_target(target, callback_args, span, env)?;
        return checker.check_known_callable_call(&sig, callback_args, span, env, label);
    }

    if let ExprKind::Variable(var_name) = &callback.kind {
        if let Some(target) = checker.first_class_callable_targets.get(var_name).cloned() {
            let sig =
                checker.specialize_first_class_callable_target(&target, callback_args, span, env)?;
            checker.callable_sigs.insert(var_name.clone(), sig.clone());
            checker
                .closure_return_types
                .insert(var_name.clone(), sig.return_type.clone());
            return checker.check_known_callable_call(&sig, callback_args, span, env, label);
        }
    }

    if let ExprKind::StringLiteral(cb_name) = &callback.kind {
        if let Some(sig) = checker.functions.get(cb_name.as_str()).cloned() {
            return checker.check_known_callable_call(&sig, callback_args, span, env, label);
        }
        if let Some(decl) = checker.fn_decls.get(cb_name.as_str()).cloned() {
            let effective_arg_count = callback_args.len();
            let required = decl.defaults.iter().filter(|default| default.is_none()).count();
            if decl.variadic.is_some() {
                if effective_arg_count < required {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Function '{}' expects at least {} arguments, got {}",
                            cb_name, required, effective_arg_count
                        ),
                    ));
                }
            } else if effective_arg_count < required || effective_arg_count > decl.params.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        cb_name,
                        Checker::format_fixed_or_range_arity(required, decl.params.len()),
                        effective_arg_count
                    ),
                ));
            }
            // Keep function-variant discovery, but do not treat scalar dummy args
            // as authoritative parameter types for callbacks over refcounted arrays.
            let _ = checker.check_function_call(cb_name, callback_args, span, env);
            return Ok(PhpType::Int);
        }
        return checker.check_function_call(cb_name, callback_args, span, env);
    }

    if let Some(sig) = checker.resolve_expr_callable_sig(callback, env)? {
        return checker.check_known_callable_call(&sig, callback_args, span, env, label);
    }

    Err(CompileError::new(
        callback.span,
        &format!("{} must have a statically known callable signature", label),
    ))
}

pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "array_map" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "array_map() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[1], env)?;
            match arr_ty {
                PhpType::Array(elem_ty) => {
                    let arr_ty = PhpType::Array(elem_ty.clone());
                    let dummy_args = vec![dummy_arg_for_array_scalar_elem(&arr_ty, span)];
                    check_callback_builtin_call(
                        checker,
                        &args[0],
                        &dummy_args,
                        span,
                        env,
                        "array_map() callback",
                    )?;
                    Ok(Some(PhpType::Array(elem_ty)))
                }
                _ => Err(CompileError::new(
                    span,
                    "array_map() second argument must be array",
                )),
            }
        }
        "array_filter" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "array_filter() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            match arr_ty {
                PhpType::Array(elem_ty) => {
                    let arr_ty = PhpType::Array(elem_ty.clone());
                    let dummy_args = vec![dummy_arg_for_array_scalar_elem(&arr_ty, span)];
                    check_callback_builtin_call(
                        checker,
                        &args[1],
                        &dummy_args,
                        span,
                        env,
                        "array_filter() callback",
                    )?;
                    Ok(Some(PhpType::Array(elem_ty)))
                }
                _ => Err(CompileError::new(
                    span,
                    "array_filter() first argument must be array",
                )),
            }
        }
        "array_reduce" => {
            if args.len() != 3 {
                return Err(CompileError::new(
                    span,
                    "array_reduce() takes exactly 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            let dummy_args = vec![
                Expr::new(ExprKind::IntLiteral(0), span),
                dummy_arg_for_array_scalar_elem(&arr_ty, span),
            ];
            check_callback_builtin_call(
                checker,
                &args[1],
                &dummy_args,
                span,
                env,
                "array_reduce() callback",
            )?;
            Ok(Some(PhpType::Int))
        }
        "array_walk" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "array_walk() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            let dummy_args = vec![dummy_arg_for_array_scalar_elem(&arr_ty, span)];
            check_callback_builtin_call(
                checker,
                &args[1],
                &dummy_args,
                span,
                env,
                "array_walk() callback",
            )?;
            Ok(Some(PhpType::Void))
        }
        "usort" | "uksort" | "uasort" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            let cmp_arg = if name == "uksort" {
                Expr::new(ExprKind::IntLiteral(0), span)
            } else {
                dummy_arg_for_array_scalar_elem(&arr_ty, span)
            };
            let dummy_args = vec![cmp_arg.clone(), cmp_arg];
            check_callback_builtin_call(
                checker,
                &args[1],
                &dummy_args,
                span,
                env,
                &format!("{}() callback", name),
            )?;
            Ok(Some(PhpType::Void))
        }
        "call_user_func_array" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "call_user_func_array() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if checker.expr_call_complex_callee_needs_runtime_capture(&args[0]) {
                return Err(CompileError::new(
                    args[0].span,
                    "call_user_func_array() callback does not support complex expressions that select captured callables at runtime",
                ));
            }
            if let ExprKind::FirstClassCallable(target) = &args[0].kind {
                let elems = match &args[1].kind {
                    ExprKind::ArrayLiteral(elems) => elems.as_slice(),
                    _ => &[],
                };
                let sig = checker.specialize_first_class_callable_target(target, elems, span, env)?;
                validate_call_user_func_array_ref_args(&sig, &args[1], span)?;
                if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                    let ret_ty = checker.check_known_callable_call(
                        &sig,
                        elems,
                        span,
                        env,
                        "call_user_func_array() callback",
                    )?;
                    return Ok(Some(ret_ty));
                }
                return Ok(Some(sig.return_type));
            }
            if let ExprKind::Variable(var_name) = &args[0].kind {
                if let Some(target) = checker.first_class_callable_targets.get(var_name).cloned() {
                    let elems = match &args[1].kind {
                        ExprKind::ArrayLiteral(elems) => elems.as_slice(),
                        _ => &[],
                    };
                    let sig =
                        checker.specialize_first_class_callable_target(&target, elems, span, env)?;
                    checker.callable_sigs.insert(var_name.clone(), sig.clone());
                    checker
                        .closure_return_types
                        .insert(var_name.clone(), sig.return_type.clone());
                    validate_call_user_func_array_ref_args(&sig, &args[1], span)?;
                    if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                        let ret_ty = checker.check_known_callable_call(
                            &sig,
                            elems,
                            span,
                            env,
                            "call_user_func_array() callback",
                        )?;
                        return Ok(Some(ret_ty));
                    }
                    return Ok(Some(sig.return_type));
                }
            }
            if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                if let Some(sig) = checker.functions.get(cb_name.as_str()).cloned() {
                    validate_call_user_func_array_ref_args(&sig, &args[1], span)?;
                    if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                        let ret_ty = checker.check_known_callable_call(
                            &sig,
                            elems,
                            span,
                            env,
                            "call_user_func_array() callback",
                        )?;
                        return Ok(Some(ret_ty));
                    }
                    return Ok(Some(sig.return_type.clone()));
                }
                if let Some(decl) = checker.fn_decls.get(cb_name.as_str()).cloned() {
                    if decl.ref_params.iter().any(|is_ref| *is_ref)
                        && !matches!(args[1].kind, ExprKind::ArrayLiteral(_))
                    {
                        return Err(CompileError::new(
                            span,
                            "call_user_func_array() requires a literal argument array when the callback has pass-by-reference parameters",
                        ));
                    }
                }
                if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                    let ret_ty = checker.check_function_call(cb_name, elems, span, env)?;
                    return Ok(Some(ret_ty));
                }
                if let Some(decl) = checker.fn_decls.get(cb_name.as_str()).cloned() {
                    let dummy_args: Vec<Expr> = decl
                        .params
                        .iter()
                        .map(|_| Expr::new(ExprKind::IntLiteral(0), span))
                        .collect();
                    let ret_ty = checker.check_function_call(cb_name, &dummy_args, span, env)?;
                    return Ok(Some(ret_ty));
                }
            }
            if let Some(sig) = checker.resolve_expr_callable_sig(&args[0], env)? {
                validate_call_user_func_array_ref_args(&sig, &args[1], span)?;
                if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                    let ret_ty = checker.check_known_callable_call(
                        &sig,
                        elems,
                        span,
                        env,
                        "call_user_func_array() callback",
                    )?;
                    return Ok(Some(ret_ty));
                }
                return Ok(Some(sig.return_type.clone()));
            }
            Err(CompileError::new(
                args[0].span,
                "call_user_func_array() callback must have a statically known callable signature",
            ))
        }
        "call_user_func" => {
            if args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "call_user_func() takes at least 1 argument",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if checker.expr_call_complex_callee_needs_runtime_capture(&args[0]) {
                return Err(CompileError::new(
                    args[0].span,
                    "call_user_func() callback does not support complex expressions that select captured callables at runtime",
                ));
            }
            if let ExprKind::FirstClassCallable(target) = &args[0].kind {
                let sig =
                    checker.specialize_first_class_callable_target(target, &args[1..], span, env)?;
                let ret_ty = checker.check_known_callable_call(
                    &sig,
                    &args[1..],
                    span,
                    env,
                    "call_user_func() callback",
                )?;
                return Ok(Some(ret_ty));
            }
            if let ExprKind::Variable(var_name) = &args[0].kind {
                if let Some(target) = checker.first_class_callable_targets.get(var_name).cloned() {
                    let sig = checker.specialize_first_class_callable_target(
                        &target,
                        &args[1..],
                        span,
                        env,
                    )?;
                    checker.callable_sigs.insert(var_name.clone(), sig.clone());
                    checker
                        .closure_return_types
                        .insert(var_name.clone(), sig.return_type.clone());
                    let ret_ty = checker.check_known_callable_call(
                        &sig,
                        &args[1..],
                        span,
                        env,
                        "call_user_func() callback",
                    )?;
                    return Ok(Some(ret_ty));
                }
            }
            if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                if let Some(sig) = checker.functions.get(cb_name.as_str()).cloned() {
                    let ret_ty = checker.check_known_callable_call(
                        &sig,
                        &args[1..],
                        span,
                        env,
                        "call_user_func() callback",
                    )?;
                    return Ok(Some(ret_ty));
                }
                let cb_args = args[1..].to_vec();
                let ret_ty = checker.check_function_call(cb_name, &cb_args, span, env)?;
                return Ok(Some(ret_ty));
            }
            if let Some(sig) = checker.resolve_expr_callable_sig(&args[0], env)? {
                let ret_ty = checker.check_known_callable_call(
                    &sig,
                    &args[1..],
                    span,
                    env,
                    "call_user_func() callback",
                )?;
                return Ok(Some(ret_ty));
            }
            Err(CompileError::new(
                args[0].span,
                "call_user_func() callback must have a statically known callable signature",
            ))
        }
        "class_alias" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "class_alias() takes 2 or 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            return Err(CompileError::new(
                span,
                "class_alias() is only supported as a top-level statement with literal class names",
            ));
        }
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 1 or 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if !matches!(args[0].kind, ExprKind::StringLiteral(_)) {
                return Err(CompileError::new(
                    span,
                    &format!("{}() first argument must be a string literal in AOT mode", name),
                ));
            }
            if let Some(autoload_arg) = args.get(1) {
                if !matches!(
                    autoload_arg.kind,
                    ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_)
                ) {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "{}() autoload argument must be a literal bool or int in AOT mode",
                            name
                        ),
                    ));
                }
            }
            Ok(Some(PhpType::Bool))
        }
        "get_class" => {
            if args.len() > 1 {
                return Err(CompileError::new(
                    span,
                    "get_class() takes at most 1 argument",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "get_parent_class" => {
            if args.len() > 1 {
                return Err(CompileError::new(
                    span,
                    "get_parent_class() takes at most 1 argument",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "is_a" | "is_subclass_of" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 2 or 3 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes no arguments", name),
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "function_exists" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "function_exists() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                if checker.fn_decls.contains_key(cb_name.as_str())
                    && !checker.functions.contains_key(cb_name.as_str())
                {
                    if let Some(decl) = checker.fn_decls.get(cb_name.as_str()).cloned() {
                        let dummy_args: Vec<Expr> = decl
                            .params
                            .iter()
                            .map(|_| Expr::new(ExprKind::IntLiteral(0), span))
                            .collect();
                        let _ = checker.check_function_call(cb_name, &dummy_args, span, env);
                    }
                } else if checker.function_variant_groups.contains_key(cb_name.as_str())
                    && !checker.functions.contains_key(cb_name.as_str())
                {
                    let _ = checker.ensure_function_variant_group_signature(cb_name, span);
                }
            }
            Ok(Some(PhpType::Bool))
        }
        _ => Ok(None),
    }
}
