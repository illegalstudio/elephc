//! Purpose:
//! Handles function resolution specialization details for call checking.
//! Materializes signatures or specialized metadata used by argument validation and return inference.
//!
//! Called from:
//! - `crate::types::checker::functions::resolution`
//!
//! Key details:
//! - Specialized and builtin signatures must expose the caller-visible parameter contract expected by call-argument planning.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    pub(crate) fn respecialize_resolved_function_callable_params_if_needed(
        &mut self,
        name: &str,
        args: &[Expr],
        caller_env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let Some(decl) = self.fn_decls.get(name).cloned() else {
            return Ok(false);
        };
        let Some(stored_sig) = self.functions.get(name).cloned() else {
            return Ok(false);
        };

        let regular_param_count = crate::types::call_args::regular_param_count(&stored_sig);
        let mut param_types = stored_sig.params.clone();
        let mut changed = false;
        let mut seen_idx = 0usize;

        for arg in args {
            let actual_ty = self.infer_type(arg, caller_env)?;
            if matches!(arg.kind, ExprKind::Spread(_)) {
                continue;
            }
            if seen_idx < regular_param_count && actual_ty == PhpType::Callable {
                if let Some((param_name, _)) = param_types.get(seen_idx) {
                    if let Some(sig) = self.resolve_expr_callable_sig(arg, caller_env)? {
                        let key = (name.to_string(), param_name.clone());
                        if self.callable_param_sigs.get(&key) != Some(&sig) {
                            self.callable_param_sigs.insert(key, sig);
                            changed = true;
                        }
                    }
                }
            }
            // Untyped callable params can be resolved as the `Int` fallback
            // before method/property flow has stabilized; recheck them once
            // a later pass sees the callable argument.
            if seen_idx < regular_param_count
                && !stored_sig
                    .declared_params
                    .get(seen_idx)
                    .copied()
                    .unwrap_or(false)
                && param_types[seen_idx].1 == PhpType::Int
                && actual_ty == PhpType::Callable
            {
                param_types[seen_idx].1 = actual_ty;
                changed = true;
            }
            seen_idx += 1;
        }

        if changed {
            self.resolve_function_signature(name, &decl, param_types)?;
        }

        Ok(changed)
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
            let regular_param_count = crate::types::call_args::regular_param_count(stored_sig);
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
                let regular_names: Vec<String> = stored_sig.params[..regular_param_count]
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect();
                if Self::has_unknown_named_variadic_arg(args, &regular_names) {
                    if let Some((_, variadic_ty)) = stored_sig.params.last_mut() {
                        *variadic_ty = PhpType::Iterable;
                    }
                } else {
                    let mut elem_ty = actual_arg_types[regular_param_count].clone();
                    for actual_ty in actual_arg_types.iter().skip(regular_param_count + 1) {
                        elem_ty = Self::wider_type(&elem_ty, actual_ty);
                    }
                    elem_ty = Self::variadic_container_elem_ty(elem_ty);
                    if let Some((_, PhpType::Array(existing_elem_ty))) = stored_sig.params.last_mut() {
                        **existing_elem_ty = Self::wider_type(existing_elem_ty.as_ref(), &elem_ty);
                    }
                }
            }
        }
        Ok(())
    }
}
