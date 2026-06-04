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
    /// For functions declared without explicit parameter type annotations, this widens each
    /// undeclared regular parameter via `record_observed_param_type` (accumulating the union of
    /// the types observed for that parameter across call sites) and updates the stored signature
    /// in place. Variadic parameters are handled separately, including the special case of
    /// unknown-named variadic arguments which widen the variadic element type to `Iterable`.
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
        let Some((regular_param_count, declared_params, has_variadic)) =
            self.functions.get(name).map(|sig| {
                (
                    crate::types::call_args::regular_param_count(sig),
                    sig.declared_params.clone(),
                    sig.variadic.is_some(),
                )
            })
        else {
            return Ok(());
        };

        // Phase 1: accumulate observed call-site types per undeclared regular parameter. This
        // borrows `&mut self` for the accumulator, so it must not hold a borrow of the signature.
        let mut desired_param_types: Vec<(usize, PhpType)> = Vec::new();
        let mut seen_idx = 0usize;
        for (arg, actual_ty) in args.iter().zip(actual_arg_types.iter()) {
            if matches!(arg.kind, ExprKind::Spread(_)) {
                continue;
            }
            if seen_idx < regular_param_count
                && !declared_params.get(seen_idx).copied().unwrap_or(false)
            {
                if let Some(desired) = self.record_observed_param_type(name, seen_idx, actual_ty) {
                    desired_param_types.push((seen_idx, desired));
                }
            }
            seen_idx += 1;
        }

        // Phase 2: write the widened parameter types and update the variadic element type.
        if let Some(stored_sig) = self.functions.get_mut(name) {
            for (idx, ty) in desired_param_types {
                if idx < stored_sig.params.len() {
                    stored_sig.params[idx].1 = ty;
                }
            }
            if has_variadic && seen_idx > regular_param_count {
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

    /// Records an observed call-site argument type for an undeclared (untyped) parameter and
    /// returns the parameter's specialized type, or `None` to leave the parameter unchanged.
    ///
    /// The accumulator unions every observed argument type for `(name, idx)` across call sites.
    /// `Int`, `Bool`, and `Void` arguments do not, on their own, specialize a parameter — matching
    /// the historical behavior where they keep the `Int` fallback (`void` is never recorded, since
    /// it models `null`/no-value sources). So while the accumulated union is empty or contains only
    /// `Int`/`Bool`, this returns `None`. Once a stronger type is observed (e.g. `string`), the full
    /// accumulated union — including any previously seen `Int`/`Bool`, e.g. `int|string` — is
    /// returned so codegen boxes the parameter as a `Mixed` runtime value and each argument keeps
    /// its own runtime type instead of being coerced to the last-seen type.
    pub(crate) fn record_observed_param_type(
        &mut self,
        name: &str,
        idx: usize,
        actual_ty: &PhpType,
    ) -> Option<PhpType> {
        if matches!(actual_ty, PhpType::Void) {
            return None;
        }
        let key = (name.to_string(), idx);
        let accumulated = match self.fn_param_observed_types.get(&key) {
            Some(prev) => self.normalize_union_type(vec![prev.clone(), actual_ty.clone()]),
            None => actual_ty.clone(),
        };
        self.fn_param_observed_types.insert(key, accumulated.clone());
        if is_int_or_bool_only(&accumulated) {
            None
        } else {
            Some(accumulated)
        }
    }

    /// Specializes parameter types from the actual argument types at a call site and
    /// returns them if they differ from the stored signature.
    ///
    /// This is the core specialization logic used by `respecialize_resolved_function_params_if_needed`.
    /// For each argument position with an inferred `Callable` type, it records the callable's
    /// signature against the parameter name for later use. For undeclared parameters it widens the
    /// type via `record_observed_param_type`, accumulating the union of every type observed for that
    /// parameter across call sites (see that method for the int-fallback and `void` handling).
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
            {
                if let Some(desired) =
                    self.record_observed_param_type(name, seen_idx, &actual_ty)
                {
                    if param_types[seen_idx].1 != desired {
                        param_types[seen_idx].1 = desired;
                        changed = true;
                    }
                }
            }
            seen_idx += 1;
        }

        Ok(changed.then_some(param_types))
    }
}

/// Returns true when an accumulated parameter type contains only `Int`/`Bool` members.
///
/// Untyped parameters fall back to `Int`, and historically calls that only ever pass integers or
/// booleans keep whatever type the rest of the checker inferred (booleans must stay `bool` so a
/// strict `=== true`/`=== false` guard is not folded away). Such observations are still recorded
/// in the accumulator — so they widen to a union once a stronger type (e.g. `string`) appears —
/// but on their own they do not drive call-site specialization.
fn is_int_or_bool_only(ty: &PhpType) -> bool {
    match ty {
        PhpType::Union(members) => members
            .iter()
            .all(|member| matches!(member, PhpType::Int | PhpType::Bool)),
        other => matches!(other, PhpType::Int | PhpType::Bool),
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
