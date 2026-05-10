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
