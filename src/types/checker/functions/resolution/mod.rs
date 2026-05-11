//! Purpose:
//! Resolves function call targets and callable signatures for the checker.
//! Combines declared functions, builtin signatures, specialization, and callable metadata into one lookup path.
//!
//! Called from:
//! - `crate::types::checker::functions`
//!
//! Key details:
//! - Resolution must match name-resolver canonicalization and PHP builtin fallback rules.

mod signature;
mod specialization;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    fn variadic_container_elem_ty(elem_ty: PhpType) -> PhpType {
        if matches!(elem_ty, PhpType::Iterable) {
            PhpType::Mixed
        } else {
            elem_ty
        }
    }

    fn has_unknown_named_variadic_arg(args: &[Expr], regular_params: &[String]) -> bool {
        args.iter().any(|arg| {
            matches!(
                &arg.kind,
                ExprKind::NamedArg { name, .. } if !regular_params.iter().any(|param| param == name)
            )
        })
    }

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
            return self.check_normalized_resolved_function_call(
                name,
                &sig,
                &effective_sig,
                &normalized_args,
                span,
                caller_env,
            );
        }

        if self.function_variant_groups.contains_key(name) {
            self.ensure_function_variant_group_signature(name, span)?;
            return self.check_function_call(name, args, span, caller_env);
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
            declared_return: decl.return_type.is_some(),
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
                    let specialized_ty = Self::specialize_generic_array_hint(&declared_ty, &ty);
                    param_types.push((decl.params[arg_idx].clone(), specialized_ty));
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
            if Self::has_unknown_named_variadic_arg(args, &decl.params) {
                param_types.push((vp.clone(), PhpType::Iterable));
            } else {
                let variadic_elem_ty = if args.len() > decl.params.len() {
                    self.infer_type(&args[decl.params.len()], caller_env)
                        .unwrap_or(PhpType::Int)
                } else {
                    PhpType::Int
                };
                let variadic_elem_ty = Self::variadic_container_elem_ty(variadic_elem_ty);
                param_types.push((vp.clone(), PhpType::Array(Box::new(variadic_elem_ty))));
            }
        }

        self.resolve_function_signature(name, &decl, param_types)
    }

    pub(crate) fn check_function_call_pre_normalized(
        &mut self,
        name: &str,
        normalized_args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let sig = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;
        let effective_sig = Self::callable_sig_for_declared_params(&sig, &sig.declared_params);
        self.check_normalized_resolved_function_call(
            name,
            &sig,
            &effective_sig,
            normalized_args,
            span,
            caller_env,
        )
    }

    fn check_normalized_resolved_function_call(
        &mut self,
        name: &str,
        sig: &FunctionSig,
        effective_sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
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
        Ok(sig.return_type.clone())
    }
}
