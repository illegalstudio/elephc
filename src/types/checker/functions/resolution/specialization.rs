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
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::super::{Checker, FnDecl};

impl Checker {
    /// Re-specializes a previously resolved function's signature when call-site type
    /// information allows more precise parameter types than the original declaration.
    ///
    /// For resolved (builtin or user-defined) functions, this performs call-site
    /// specialization: it infers actual argument types and updates the stored signature
    /// if the function was declared with inferred parameter types. For functions with
    /// multiple variants, all variants are updated to have identical signatures.
    ///
    /// Returns `Ok(true)` if specialization occurred, `Ok(false)` if the function
    /// was not found or no specialization was needed.
    pub(crate) fn respecialize_resolved_function_params_if_needed(
        &mut self,
        name: &str,
        args: &[Expr],
        caller_env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let Some(stored_sig) = self.functions.get(name).cloned() else {
            return Ok(false);
        };

        let Some(param_types) =
            self.respecialized_param_types_for_call(name, &stored_sig, args, caller_env)?
        else {
            return Ok(false);
        };

        if let Some(decl) = self.fn_decls.get(name).cloned() {
            self.resolve_function_signature(name, &decl, param_types)?;
            return Ok(true);
        }

        let Some(variants) = self.function_variant_groups.get(name).cloned() else {
            return Ok(false);
        };

        for variant in &variants {
            let decl = self.fn_decls.get(variant).cloned().ok_or_else(|| {
                CompileError::new(
                    crate::span::Span::dummy(),
                    &format!(
                        "Compiler error: function variant '{}' for '{}' has no declaration",
                        variant, name
                    ),
                )
            })?;
            let variant_param_types = param_types_for_decl(&decl, &param_types);
            self.resolve_function_signature(variant, &decl, variant_param_types)?;
        }

        let first_variant = variants.first().ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Function '{}' has no variants", name),
            )
        })?;
        let first_sig = self.functions.get(first_variant).cloned().ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Compiler error: function variant '{}' for '{}' has no signature",
                    first_variant, name
                ),
            )
        })?;
        for variant in variants.iter().skip(1) {
            let sig = self.functions.get(variant).cloned().ok_or_else(|| {
                CompileError::new(
                    crate::span::Span::dummy(),
                    &format!(
                        "Compiler error: function variant '{}' for '{}' has no signature",
                        variant, name
                    ),
                )
            })?;
            if sig != first_sig {
                return Err(CompileError::new(
                    crate::span::Span::dummy(),
                    &format!(
                        "Function variants for '{}' must have identical signatures",
                        name
                    ),
                ));
            }
        }
        self.functions.insert(name.to_string(), first_sig);
        Ok(true)
    }

    /// Specializes an untyped user-defined function's signature from the actual argument
    /// types at a specific call site.
    ///
    /// For functions that were declared without explicit parameter type annotations,
    /// this infers concrete types from the call-site arguments and updates the stored
    /// signature in place. Handles both regular parameters and variadic parameters,
    /// including the special case of unknown-named variadic arguments which widen
    /// the variadic element type to `Iterable`.
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

    /// Infers concrete parameter types from actual argument types at a call site and
    /// returns them if they differ from the stored signature.
    ///
    /// This is the core specialization logic used by `respecialize_resolved_function_params_if_needed`.
    /// For each argument position with an inferred `Callable` type, it records the callable's
    /// signature against the parameter name for later use. For undeclared parameters with `Int`
    /// as a fallback type, it replaces the fallback with the actual argument type when the
    /// actual type is not itself `Int`, `Bool`, or `Void`.
    ///
    /// Returns `Some(param_types)` if any changes were made, `None` otherwise.
    fn respecialized_param_types_for_call(
        &mut self,
        name: &str,
        stored_sig: &FunctionSig,
        args: &[Expr],
        caller_env: &TypeEnv,
    ) -> Result<Option<Vec<(String, PhpType)>>, CompileError> {
        let regular_param_count = crate::types::call_args::regular_param_count(stored_sig);
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
            if seen_idx < regular_param_count && is_callable_array_type(&actual_ty) {
                if let Some((param_name, _)) = param_types.get(seen_idx) {
                    if let Some(sig) = self.resolve_expr_callable_array_sig(arg, caller_env)? {
                        let key = (name.to_string(), param_name.clone());
                        if self.callable_param_sigs.get(&key) != Some(&sig) {
                            self.callable_param_sigs.insert(key, sig);
                            changed = true;
                        }
                    }
                }
            }
            if seen_idx < regular_param_count
                && !stored_sig
                    .declared_params
                    .get(seen_idx)
                    .copied()
                    .unwrap_or(false)
                && !matches!(
                    actual_ty,
                    PhpType::Void | PhpType::Never | PhpType::Callable
                )
            {
                let key = (name.to_string(), seen_idx);
                let seen = self.param_specialization_seen.contains(&key);
                if param_types[seen_idx].1 == PhpType::Int && !seen {
                    // Discard the `Int` fallback exactly once: adopt the type of the
                    // first call so an all-`Str` (etc.) parameter is not polluted by
                    // unioning the fallback. The seen set marks the discard so a real
                    // later int call still widens instead of re-adopting.
                    self.param_specialization_seen.insert(key);
                    if param_types[seen_idx].1 != actual_ty {
                        param_types[seen_idx].1 = actual_ty.clone();
                        changed = true;
                    }
                } else {
                    // Widen to the union so heterogeneous call sites become `Mixed`
                    // rather than a single (wrong) type. A no-op for an already-`Mixed`
                    // or patched parameter (so e.g. `Generator::send`'s value stays
                    // `Mixed`).
                    let widened = Self::union_param_type(&param_types[seen_idx].1, &actual_ty);
                    if param_types[seen_idx].1 != widened {
                        param_types[seen_idx].1 = widened;
                        changed = true;
                    }
                }
            }
            seen_idx += 1;
        }

        Ok(changed.then_some(param_types))
    }
}

/// Returns true when a call argument is an array whose elements are callable descriptors.
fn is_callable_array_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}

/// Extracts parameter types from a generic `param_types` list, mapping them to the
/// parameter names declared in `decl`.
///
/// Positions are matched by index: positional parameters map directly, and if `decl`
/// has a variadic parameter, the final entry in `param_types` maps to it. Parameters
/// in `decl` that have no corresponding entry in `param_types` are omitted.
fn param_types_for_decl(
    decl: &FnDecl,
    param_types: &[(String, PhpType)],
) -> Vec<(String, PhpType)> {
    param_types
        .iter()
        .enumerate()
        .filter_map(|(idx, (_, ty))| {
            if let Some(name) = decl.params.get(idx) {
                Some((name.clone(), ty.clone()))
            } else if idx == decl.params.len() {
                decl.variadic
                    .as_ref()
                    .map(|name| (name.clone(), ty.clone()))
            } else {
                None
            }
        })
        .collect()
}
