use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

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
            if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                if let PhpType::Array(ref elem_ty) = arr_ty {
                    let dummy_arg = match elem_ty.as_ref() {
                        PhpType::Str => Expr::new(ExprKind::StringLiteral(String::new()), span),
                        PhpType::Float => Expr::new(ExprKind::FloatLiteral(0.0), span),
                        PhpType::Bool => Expr::new(ExprKind::BoolLiteral(false), span),
                        _ => Expr::new(ExprKind::IntLiteral(0), span),
                    };
                    let dummy_args = vec![dummy_arg];
                    let _ = checker.check_function_call(cb_name, &dummy_args, span, env);
                }
            }
            match arr_ty {
                PhpType::Array(elem_ty) => Ok(Some(PhpType::Array(elem_ty))),
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
            if let ExprKind::StringLiteral(cb_name) = &args[1].kind {
                let dummy_args = vec![Expr::new(ExprKind::IntLiteral(0), span)];
                let _ = checker.check_function_call(cb_name, &dummy_args, span, env);
            }
            match arr_ty {
                PhpType::Array(elem_ty) => Ok(Some(PhpType::Array(elem_ty))),
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
            if let ExprKind::StringLiteral(cb_name) = &args[1].kind {
                let dummy_args = vec![
                    Expr::new(ExprKind::IntLiteral(0), span),
                    Expr::new(ExprKind::IntLiteral(0), span),
                ];
                let _ = checker.check_function_call(cb_name, &dummy_args, span, env);
            }
            Ok(Some(PhpType::Int))
        }
        "array_walk" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "array_walk() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if let ExprKind::StringLiteral(cb_name) = &args[1].kind {
                let dummy_args = vec![Expr::new(ExprKind::IntLiteral(0), span)];
                let _ = checker.check_function_call(cb_name, &dummy_args, span, env);
            }
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
            if let ExprKind::StringLiteral(cb_name) = &args[1].kind {
                let dummy_args = vec![
                    Expr::new(ExprKind::IntLiteral(0), span),
                    Expr::new(ExprKind::IntLiteral(0), span),
                ];
                let _ = checker.check_function_call(cb_name, &dummy_args, span, env);
            }
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
            if let ExprKind::FirstClassCallable(target) = &args[0].kind {
                let elems = match &args[1].kind {
                    ExprKind::ArrayLiteral(elems) => elems.as_slice(),
                    _ => &[],
                };
                let sig = checker.specialize_first_class_callable_target(target, elems, span, env)?;
                if sig.ref_params.iter().any(|is_ref| *is_ref) {
                    return Err(CompileError::new(
                        span,
                        "call_user_func_array() does not support pass-by-reference callback parameters yet",
                    ));
                }
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
                    if sig.ref_params.iter().any(|is_ref| *is_ref) {
                        return Err(CompileError::new(
                            span,
                            "call_user_func_array() does not support pass-by-reference callback parameters yet",
                        ));
                    }
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
                    if sig.ref_params.iter().any(|is_ref| *is_ref) {
                        return Err(CompileError::new(
                            span,
                            "call_user_func_array() does not support pass-by-reference callback parameters yet",
                        ));
                    }
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
                    if decl.ref_params.iter().any(|is_ref| *is_ref) {
                        return Err(CompileError::new(
                            span,
                            "call_user_func_array() does not support pass-by-reference callback parameters yet",
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
                if sig.ref_params.iter().any(|is_ref| *is_ref) {
                    return Err(CompileError::new(
                        span,
                        "call_user_func_array() does not support pass-by-reference callback parameters yet",
                    ));
                }
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
            Ok(Some(PhpType::Int))
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
            Ok(Some(PhpType::Int))
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
                }
            }
            Ok(Some(PhpType::Bool))
        }
        _ => Ok(None),
    }
}
