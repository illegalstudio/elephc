use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::{Checker, FnDecl};

impl Checker {
    pub fn check_function_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        if let Some(sig) = self.functions.get(name).cloned() {
            let effective_sig = Self::callable_sig_for_declared_params(&sig, &sig.declared_params);
            let normalized_args = self.normalize_named_call_args(
                &effective_sig,
                args,
                span,
                &format!("Function '{}'", name),
            )?;
            let args = normalized_args.as_slice();
            let effective_arg_count = args
                .iter()
                .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
                .count();
            let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));
            let required = effective_sig.defaults.iter().filter(|d| d.is_none()).count();
            if effective_sig.ref_params.iter().any(|is_ref| *is_ref) && has_spread {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' cannot be invoked with spread arguments when it has pass-by-reference parameters",
                        name
                    ),
                ));
            }
            if !has_spread {
                if effective_sig.variadic.is_some() {
                    if effective_arg_count < required {
                        return Err(CompileError::new(
                            span,
                            &format!(
                                "Function '{}' expects at least {} arguments, got {}",
                                name, required, effective_arg_count
                            ),
                        ));
                    }
                } else if effective_arg_count < required
                    || effective_arg_count > effective_sig.params.len()
                {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Function '{}' expects {} arguments, got {}",
                            name,
                            Self::format_fixed_or_range_arity(required, effective_sig.params.len()),
                            effective_arg_count
                        ),
                    ));
                }
            }
            let regular_param_count = if effective_sig.variadic.is_some() {
                effective_sig.params.len().saturating_sub(1)
            } else {
                effective_sig.params.len()
            };
            let variadic_elem_ty = effective_sig.variadic.as_ref().and_then(|_| {
                effective_sig.params.last().and_then(|(_, ty)| match ty {
                    PhpType::Array(elem) => Some((**elem).clone()),
                    _ => None,
                })
            });
            let mut param_idx = 0usize;
            for arg in args {
                let actual_ty = self.infer_type(arg, caller_env)?;
                if matches!(arg.kind, ExprKind::Spread(_)) {
                    continue;
                }
                if param_idx < regular_param_count {
                    if effective_sig
                        .ref_params
                        .get(param_idx)
                        .copied()
                        .unwrap_or(false)
                        && !matches!(arg.kind, ExprKind::Variable(_))
                    {
                        let param_name = effective_sig
                            .params
                            .get(param_idx)
                            .map(|(name, _)| name.as_str())
                            .unwrap_or("arg");
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "Function '{}' parameter ${} must be passed a variable",
                                name, param_name
                            ),
                        ));
                    }
                    if let Some((param_name, expected_ty)) = effective_sig.params.get(param_idx) {
                        if effective_sig
                            .declared_params
                            .get(param_idx)
                            .copied()
                            .unwrap_or(false)
                            && effective_sig
                                .ref_params
                                .get(param_idx)
                                .copied()
                                .unwrap_or(false)
                        {
                            self.require_boxed_by_ref_storage(
                                expected_ty,
                                &actual_ty,
                                arg.span,
                                &format!("Function '{}' parameter ${}", name, param_name),
                            )?;
                        }
                        self.require_compatible_arg_type(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &format!("Function '{}' parameter ${}", name, param_name),
                        )?;
                    }
                } else if let (Some(vname), Some(expected_ty)) =
                    (effective_sig.variadic.as_ref(), variadic_elem_ty.as_ref())
                {
                    self.require_compatible_arg_type(
                        expected_ty,
                        &actual_ty,
                        arg.span,
                        &format!("Function '{}' variadic parameter ${}", name, vname),
                    )?;
                }
                param_idx += 1;
            }
            return Ok(sig.return_type);
        }

        let decl = self
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;
        let normalization_sig = FunctionSig {
            params: decl
                .params
                .iter()
                .enumerate()
                .map(|(idx, param_name)| {
                    let ty = decl
                        .param_types
                        .get(idx)
                        .and_then(|type_ann| type_ann.as_ref())
                        .and_then(|type_ann| self.resolve_type_expr(type_ann, decl.span).ok())
                        .unwrap_or(PhpType::Int);
                    (param_name.clone(), ty)
                })
                .chain(
                    decl.variadic
                        .iter()
                        .cloned()
                        .map(|name| (name, PhpType::Array(Box::new(PhpType::Int)))),
                )
                .collect(),
            defaults: decl.defaults.clone(),
            return_type: PhpType::Int,
            ref_params: decl.ref_params.clone(),
            declared_params: decl.param_types.iter().map(|type_ann| type_ann.is_some()).collect(),
            variadic: decl.variadic.clone(),
        };
        let normalized_args = self.normalize_named_call_args(
            &normalization_sig,
            args,
            span,
            &format!("Function '{}'", name),
        )?;
        let args = normalized_args.as_slice();
        let effective_arg_count = args
            .iter()
            .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
            .count();
        let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));
        let required = decl.defaults.iter().filter(|d| d.is_none()).count();
        if decl.ref_params.iter().any(|is_ref| *is_ref) && has_spread {
            return Err(CompileError::new(
                span,
                &format!(
                    "Function '{}' cannot be invoked with spread arguments when it has pass-by-reference parameters",
                    name
                ),
            ));
        }
        if !has_spread {
            if decl.variadic.is_some() {
                if effective_arg_count < required {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Function '{}' expects at least {} arguments, got {}",
                            name, required, effective_arg_count
                        ),
                    ));
                }
            } else if effective_arg_count < required || effective_arg_count > decl.params.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name,
                        Self::format_fixed_or_range_arity(required, decl.params.len()),
                        effective_arg_count
                    ),
                ));
            }
        }

        let mut param_types = Vec::new();
        let mut arg_idx = 0;
        for arg in args {
            let ty = self.infer_type(arg, caller_env)?;
            if let ExprKind::Spread(_) = &arg.kind {
                for i in arg_idx..decl.params.len() {
                    if let Some(type_ann) = decl.param_types.get(i).and_then(|t| t.as_ref()) {
                        let declared_ty = self.resolve_declared_param_type_hint(
                            type_ann,
                            decl.span,
                            &format!("Function '{}' parameter ${}", name, decl.params[i]),
                        )?;
                        self.require_compatible_arg_type(
                            &declared_ty,
                            &ty,
                            arg.span,
                            &format!("Function '{}' parameter ${}", name, decl.params[i]),
                        )?;
                        param_types.push((decl.params[i].clone(), declared_ty));
                    } else {
                        param_types.push((decl.params[i].clone(), ty.clone()));
                    }
                }
                arg_idx = decl.params.len();
            } else if arg_idx < decl.params.len() {
                if decl.ref_params.get(arg_idx).copied().unwrap_or(false)
                    && !matches!(arg.kind, ExprKind::Variable(_))
                {
                    let param_name = decl
                        .params
                        .get(arg_idx)
                        .map(String::as_str)
                        .unwrap_or("arg");
                    return Err(CompileError::new(
                        arg.span,
                        &format!(
                            "Function '{}' parameter ${} must be passed a variable",
                            name, param_name
                        ),
                    ));
                }
                if let Some(type_ann) = decl.param_types.get(arg_idx).and_then(|t| t.as_ref()) {
                    let param_name = decl
                        .params
                        .get(arg_idx)
                        .map(String::as_str)
                        .unwrap_or("arg");
                    let declared_ty = self.resolve_declared_param_type_hint(
                        type_ann,
                        decl.span,
                        &format!("Function '{}' parameter ${}", name, param_name),
                    )?;
                    if decl.ref_params.get(arg_idx).copied().unwrap_or(false) {
                        self.require_boxed_by_ref_storage(
                            &declared_ty,
                            &ty,
                            arg.span,
                            &format!("Function '{}' parameter ${}", name, param_name),
                        )?;
                    }
                    self.require_compatible_arg_type(
                        &declared_ty,
                        &ty,
                        arg.span,
                        &format!("Function '{}' parameter ${}", name, param_name),
                    )?;
                    param_types.push((decl.params[arg_idx].clone(), declared_ty));
                    arg_idx += 1;
                    continue;
                }
                param_types.push((decl.params[arg_idx].clone(), ty));
                arg_idx += 1;
            } else {
                arg_idx += 1;
            }
        }

        for i in arg_idx..decl.params.len() {
            if let Some(default_expr) = &decl.defaults[i] {
                if let Some(type_ann) = decl.param_types.get(i).and_then(|t| t.as_ref()) {
                    let declared_ty = self.resolve_declared_param_type_hint(
                        type_ann,
                        decl.span,
                        &format!("Function '{}' parameter ${}", name, decl.params[i]),
                    )?;
                    let default_ty = self.infer_type(default_expr, caller_env)?;
                    self.require_compatible_arg_type(
                        &declared_ty,
                        &default_ty,
                        default_expr.span,
                        &format!("Function '{}' parameter ${}", name, decl.params[i]),
                    )?;
                    param_types.push((decl.params[i].clone(), declared_ty));
                } else {
                    let ty = self.infer_type(default_expr, caller_env)?;
                    param_types.push((decl.params[i].clone(), ty));
                }
            }
        }

        if let Some(ref vp) = decl.variadic {
            let variadic_elem_ty = if args.len() > decl.params.len() {
                self.infer_type(&args[decl.params.len()], caller_env)
                    .unwrap_or(PhpType::Int)
            } else {
                PhpType::Int
            };
            param_types.push((vp.clone(), PhpType::Array(Box::new(variadic_elem_ty))));
        }

        self.resolve_function_signature(name, &decl, param_types)
    }

    pub(crate) fn specialize_untyped_function_params(
        &mut self,
        name: &str,
        args: &[Expr],
        caller_env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let actual_arg_types = args
            .iter()
            .map(|arg| self.infer_type(arg, caller_env))
            .collect::<Result<Vec<_>, CompileError>>()?;
        if let Some(stored_sig) = self.functions.get_mut(name) {
            let regular_param_count = if stored_sig.variadic.is_some() {
                stored_sig.params.len().saturating_sub(1)
            } else {
                stored_sig.params.len()
            };
            let mut seen_idx = 0usize;
            for (arg, actual_ty) in args.iter().zip(actual_arg_types.iter()) {
                if matches!(arg.kind, ExprKind::Spread(_)) {
                    continue;
                }
                if seen_idx < regular_param_count
                    && !stored_sig
                        .declared_params
                        .get(seen_idx)
                        .copied()
                        .unwrap_or(false)
                    && stored_sig.params[seen_idx].1 == PhpType::Int
                    && *actual_ty != PhpType::Int
                {
                    stored_sig.params[seen_idx].1 = actual_ty.clone();
                }
                seen_idx += 1;
            }
            if stored_sig.variadic.is_some() && seen_idx > regular_param_count {
                let mut elem_ty = actual_arg_types[regular_param_count].clone();
                for actual_ty in actual_arg_types.iter().skip(regular_param_count + 1) {
                    elem_ty = Self::wider_type(&elem_ty, actual_ty);
                }
                if let Some((_, PhpType::Array(existing_elem_ty))) = stored_sig.params.last_mut() {
                    **existing_elem_ty = Self::wider_type(existing_elem_ty.as_ref(), &elem_ty);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn resolve_function_signature(
        &mut self,
        name: &str,
        decl: &FnDecl,
        param_types: Vec<(String, PhpType)>,
    ) -> Result<PhpType, CompileError> {
        let mut local_env: TypeEnv = HashMap::new();
        for (pname, pty) in &param_types {
            local_env.insert(pname.clone(), pty.clone());
        }

        let provisional_sig = FunctionSig {
            params: param_types.clone(),
            defaults: decl.defaults.clone(),
            return_type: PhpType::Int,
            ref_params: decl.ref_params.clone(),
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| false))
                .collect(),
            variadic: decl.variadic.clone(),
        };
        self.functions.insert(name.to_string(), provisional_sig);

        let mut return_type = PhpType::Void;
        let mut all_return_types: Vec<PhpType> = Vec::new();
        let mut errors = Vec::new();
        let ref_param_names: Vec<String> = decl
            .params
            .iter()
            .zip(decl.ref_params.iter())
            .filter(|(_, is_ref)| **is_ref)
            .map(|(name, _)| name.clone())
            .collect();
        self.with_local_storage_context(ref_param_names, |checker| {
            for stmt in &decl.body {
                if let Err(error) = checker.check_stmt(stmt, &mut local_env) {
                    errors.extend(error.flatten());
                }
                if let Some(rt) = checker.find_return_type(stmt, &local_env) {
                    all_return_types.push(rt);
                }
            }
            Ok(())
        })?;
        if !errors.is_empty() {
            return Err(CompileError::from_many(errors));
        }

        if let Some(type_ann) = decl.return_type.as_ref() {
            let declared_ret = self.resolve_declared_return_type_hint(
                type_ann,
                decl.span,
                &format!("Function '{}'", name),
            )?;
            if all_return_types.is_empty() {
                self.require_compatible_arg_type(
                    &declared_ret,
                    &PhpType::Void,
                    decl.span,
                    &format!("Function '{}' return type", name),
                )?;
            } else {
                for rt in &all_return_types {
                    self.require_compatible_arg_type(
                        &declared_ret,
                        rt,
                        decl.span,
                        &format!("Function '{}' return type", name),
                    )?;
                }
            }
            return_type = declared_ret;
        } else if !all_return_types.is_empty() {
            return_type = all_return_types[0].clone();
            for rt in &all_return_types[1..] {
                return_type = Self::wider_type(&return_type, rt);
            }
        }

        let sig = FunctionSig {
            params: param_types,
            defaults: decl.defaults.clone(),
            return_type: return_type.clone(),
            ref_params: decl.ref_params.clone(),
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| false))
                .collect(),
            variadic: decl.variadic.clone(),
        };
        self.functions.insert(name.to_string(), sig);

        Ok(return_type)
    }
}
