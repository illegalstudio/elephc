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
        let resolving_variant_group = self
            .function_variant_groups
            .get(name)
            .is_some_and(|variants| {
                variants
                    .iter()
                    .any(|variant| self.resolving_functions.contains(variant))
            });
        if resolving_variant_group {
            return Ok(false);
        }
        if self.resolving_functions.contains(name) {
            // A recursive self-call cannot re-specialize the declaration it is inside, but the
            // types it passes still belong to the parameters: record them for the re-run, and
            // widen the provisional signature so this call and later ones are checked against it.
            if self.fn_decls.contains_key(name) {
                if let Some(widened) =
                    self.respecialized_param_types_for_call(name, &stored_sig, args, caller_env)?
                {
                    if widened != stored_sig.params {
                        if let Some(sig) = self.functions.get_mut(name) {
                            sig.params = widened.clone();
                        }
                        self.recursive_param_widenings.insert(name.to_string(), widened);
                    }
                }
            }
            return Ok(false);
        }

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
            if self.resolving_functions.contains(variant) {
                continue;
            }
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
                let conflict_span = self.fn_decls.get(variant)
                    .map_or(crate::span::Span::dummy(), |decl| decl.span);
                return Err(CompileError::new(
                    conflict_span,
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
        let mut pending_variadic = None;
        let stored_regular_count = self
            .functions
            .get(name)
            .map(crate::types::call_args::regular_param_count)
            .unwrap_or(0);
        let tail_spread_keeps_keys =
            self.variadic_tail_spread_keeps_keys(args, stored_regular_count, caller_env);
        // An untyped parameter's first specialization replaces the `Int` placeholder; every later
        // one joins with what earlier calls taught, as the variadic tail below does. Adopting only
        // the first left `call_user_func(any(...), true)` after `call_user_func(any(...), 1.5)`
        // binding `true` through a `float` parameter: `float(1)` where php returns `bool(true)`.
        let mut first_seen = Vec::new();
        if let Some(stored_sig) = self.functions.get(name) {
            let regular_param_count = crate::types::call_args::regular_param_count(stored_sig);
            for idx in 0..regular_param_count {
                first_seen.push(
                    !self
                        .param_specialization_seen
                        .contains(&(name.to_string(), idx)),
                );
            }
        }
        let mut taught = Vec::new();
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
                    && !stored_sig.ref_params.get(seen_idx).copied().unwrap_or(false)
                {
                    let current = &stored_sig.params[seen_idx].1;
                    // The joins mirror the direct-call respecialization's: `never` teaches
                    // nothing, and `null` only where it has a tagged representation.
                    let joinable = !matches!(actual_ty, PhpType::Never)
                        && (!matches!(actual_ty, PhpType::Void)
                            || crate::codegen::sentinels::null_repr_is_tagged());
                    if first_seen.get(seen_idx).copied().unwrap_or(false) {
                        if *current == PhpType::Int && *actual_ty != PhpType::Int {
                            stored_sig.params[seen_idx].1 = actual_ty.clone();
                        }
                    } else if joinable && current != actual_ty {
                        stored_sig.params[seen_idx].1 = Self::union_param_type(current, actual_ty);
                    }
                    taught.push(seen_idx);
                }
                seen_idx += 1;
            }
            if stored_sig.variadic.is_some()
                && seen_idx >= regular_param_count
                && args.len() > regular_param_count
                && !variadic_param_is_by_ref(stored_sig)
            {
                let regular_names: Vec<String> = stored_sig.params[..regular_param_count]
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect();
                if Self::has_unknown_named_variadic_arg(args, &regular_names) || tail_spread_keeps_keys {
                    if let Some((_, variadic_ty)) = stored_sig.params.last_mut() {
                        *variadic_ty = PhpType::Iterable;
                    }
                } else {
                    // A spread in the tail contributes its ELEMENTS' type, not the array's.
                    let tail_types = args
                        .iter()
                        .zip(actual_arg_types.iter())
                        .skip(regular_param_count)
                        .map(|(arg, ty)| match (&arg.kind, ty.codegen_repr()) {
                            (ExprKind::Spread(_), PhpType::Array(elem)) => *elem,
                            (ExprKind::Spread(_), PhpType::AssocArray { value, .. }) => *value,
                            (ExprKind::Spread(_), _) => PhpType::Mixed,
                            _ => ty.clone(),
                        })
                        .collect::<Vec<_>>();
                    // Element types join without coercion, as regular parameters do: folding
                    // with the RETURN rule (`string` absorbs `int`) made `w(1); w("s")` hand the
                    // body "1" for the int.
                    let mut elem_ty = tail_types[0].clone();
                    for actual_ty in tail_types.iter().skip(1) {
                        elem_ty = Self::union_param_type(&elem_ty, actual_ty);
                    }
                    elem_ty = Self::variadic_container_elem_ty(elem_ty);
                    pending_variadic =
                        Some((stored_sig.params.len().saturating_sub(1), elem_ty));
                }
            }
        }
        if let Some((variadic_index, elem_ty)) = pending_variadic {
            let key = (name.to_string(), variadic_index);
            let adopt_fallback = !self.param_specialization_seen.contains(&key);
            if let Some((_, PhpType::Array(existing_elem_ty))) =
                self.functions.get_mut(name).and_then(|sig| sig.params.last_mut())
            {
                **existing_elem_ty = if **existing_elem_ty == PhpType::Int && adopt_fallback {
                    elem_ty
                } else {
                    Self::union_param_type(existing_elem_ty.as_ref(), &elem_ty)
                };
            }
            if adopt_fallback {
                self.param_specialization_seen.insert(key);
            }
        }
        for idx in taught {
            self.param_specialization_seen.insert((name.to_string(), idx));
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
            if let ExprKind::Spread(inner) = &arg.kind {
                // A spread landing in the variadic tail contributes its elements' type.
                if seen_idx >= regular_param_count {
                    let source_elem = match self.infer_type(inner, caller_env)?.codegen_repr() {
                        PhpType::Array(elem) => *elem,
                        PhpType::AssocArray { value, .. } => *value,
                        _ => PhpType::Mixed,
                    };
                    changed |= self.widen_variadic_element(name, stored_sig, &mut param_types, source_elem);
                } else {
                    // A spread over regular parameters binds an unknown number of them (an int
                    // key by position, a string key by name), so each remaining undeclared
                    // parameter receives some element of the source. Leaving them alone kept the
                    // first call's specialization: `g(...$ints)` then `g(...$mixed)` compiled the
                    // second call against `int` parameters and read a string as `0`.
                    let source_elem = match self.infer_type(inner, caller_env)?.codegen_repr() {
                        PhpType::Array(elem) => *elem,
                        PhpType::AssocArray { value, .. } => *value,
                        _ => PhpType::Mixed,
                    };
                    for index in seen_idx..regular_param_count {
                        let declared = stored_sig.declared_params.get(index).copied().unwrap_or(false);
                        let by_ref = stored_sig.ref_params.get(index).copied().unwrap_or(false);
                        if declared
                            || by_ref
                            || matches!(source_elem, PhpType::Never | PhpType::Void)
                        {
                            continue;
                        }
                        let key = (name.to_string(), index);
                        let seen = self.param_specialization_seen.contains(&key);
                        let widened = if param_types[index].1 == PhpType::Int && !seen {
                            self.param_specialization_seen.insert(key);
                            source_elem.clone()
                        } else {
                            Self::union_param_type(&param_types[index].1, &source_elem)
                        };
                        if param_types[index].1 != widened {
                            param_types[index].1 = widened;
                            changed = true;
                        }
                    }
                }
                continue;
            }
            if seen_idx < regular_param_count && actual_ty == PhpType::Callable {
                if let Some((param_name, _)) = param_types.get(seen_idx) {
                    if let Some(sig) = self.resolve_expr_callable_sig(arg, caller_env)? {
                        let key = (name.to_string(), param_name.clone());
                        changed |= self.record_callable_param_sig(key, arg.span, sig);
                    }
                }
            }
            if seen_idx < regular_param_count && is_callable_array_type(&actual_ty) {
                if let Some((param_name, _)) = param_types.get(seen_idx) {
                    if let Some(sig) = self.resolve_expr_callable_array_sig(arg, caller_env)? {
                        let key = (name.to_string(), param_name.clone());
                        changed |= self.record_callable_param_sig(key, arg.span, sig);
                    }
                }
            }
            let undeclared_ref_param = seen_idx < regular_param_count
                && stored_sig.ref_params.get(seen_idx).copied().unwrap_or(false)
                && !stored_sig
                    .declared_params
                    .get(seen_idx)
                    .copied()
                    .unwrap_or(false);
            if undeclared_ref_param {
                if param_types[seen_idx].1 != PhpType::Mixed {
                    param_types[seen_idx].1 = PhpType::Mixed;
                    changed = true;
                }
                seen_idx += 1;
                continue;
            }
            // A DECLARED `array` hint is generic: it resolves to `Array(Mixed)` and is then
            // specialized to the first call site's concrete element type. That narrowing has to be
            // joined over ALL call sites, or a later `Array(Mixed)` argument is passed to a body
            // compiled for raw scalar slots and read back as a POINTER. The `declared_params` gate
            // below used to exclude it from the very widening machinery that exists to prevent
            // exactly this, because `array` is, technically, declared.
            //
            // `array` is the only PHP hint that resolves to an `Array`/`AssocArray` type, so a
            // declared parameter currently carrying one can only have come from a generic hint.
            let declared = stored_sig
                .declared_params
                .get(seen_idx)
                .copied()
                .unwrap_or(false);
            // A hash argument decides the STORAGE of a bare `array` hint, declared or not. The hint
            // resolves to `Array(Mixed)` -- the indexed repr -- and a hash bound to it is restamped
            // rather than converted, so every consumer reading the static type misreads the
            // container (`json_encode` prints raw slot words, `in_array` faults). The join of an
            // indexed contract with a hash argument is the HASH, as in
            // `types::array_storage::join_array_storage_conversion`: a hash represents any PHP
            // array, a packed vector cannot. Only the SHAPE is decided here -- the key and value
            // stay `Mixed`, so this never narrows an element type the way specialization does.
            if seen_idx < regular_param_count
                && declared
                && Self::is_generic_array_hint(&param_types[seen_idx].1)
                && matches!(actual_ty, PhpType::AssocArray { .. })
            {
                param_types[seen_idx].1 = PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                };
                changed = true;
            }
            // A callable argument widens like any other. Excluding it left a parameter first
            // called with a string at `string` when a later call passed a closure, so the body
            // read the closure's descriptor as a string: `gettype()` answered "string", and a
            // `preg_replace_callback()` forwarding it fataled with `Call to undefined function`.
            // The reverse order already came out `mixed`, through the first-call adoption above.
            if seen_idx < regular_param_count
                && !declared
                && !matches!(actual_ty, PhpType::Never)
                && (!matches!(actual_ty, PhpType::Void)
                    || crate::codegen::sentinels::null_repr_is_tagged())
            {
                let key = (name.to_string(), seen_idx);
                let seen = self.param_specialization_seen.contains(&key);
                if Self::is_generic_array_hint(&param_types[seen_idx].1)
                    && !seen
                    && is_specializable_array_argument(&actual_ty)
                {
                    // Discard the generic `array` hint exactly once, mirroring the `Int` fallback
                    // below: adopt the first call's concrete array type so an all-`array<int>`
                    // parameter is not immediately polluted back to `array<mixed>` by unioning with
                    // its own declaration. Exact PHP `array` arguments are a packed/hash union, not
                    // `Array(Mixed)`; adopting that union keeps by-reference `array` parameters
                    // array-typed instead of widening to `Mixed`. The seen set marks the discard,
                    // so every LATER call widens instead of re-adopting.
                    self.param_specialization_seen.insert(key);
                    let specialized = Self::specialize_generic_array_param_hint(
                        &param_types[seen_idx].1,
                        &actual_ty,
                    );
                    if param_types[seen_idx].1 != specialized {
                        param_types[seen_idx].1 = specialized;
                        changed = true;
                    }
                } else if param_types[seen_idx].1 == PhpType::Int && !seen {
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
            // The variadic tail's element type joins over every call exactly as a regular
            // parameter does. It did not: the element type was fixed by the calls seen before the
            // body was first resolved, so `v(); v("s")` kept the `int` fallback and `v(1); v("s")`
            // kept `int`, and the body read each string's pointer as an integer.
            if seen_idx >= regular_param_count
                && !matches!(arg.kind, ExprKind::NamedArg { .. })
                && !matches!(actual_ty, PhpType::Never)
            {
                let actual_elem = Self::variadic_container_elem_ty(actual_ty.clone());
                changed |= self.widen_variadic_element(name, stored_sig, &mut param_types, actual_elem);
            }
            seen_idx += 1;
        }

        if !variadic_param_is_by_ref(stored_sig)
            && function_variadic_tail_needs_iterable(
            args,
            stored_sig,
            regular_param_count,
            caller_env,
        )
        {
            if let Some(variadic_name) = stored_sig.variadic.as_deref() {
                if let Some(variadic_index) = param_types
                    .iter()
                    .position(|(param_name, _)| param_name == variadic_name)
                {
                    if param_types[variadic_index].1 != PhpType::Iterable {
                        param_types[variadic_index].1 = PhpType::Iterable;
                        changed = true;
                    }
                }
            }
        }

        Ok(changed.then_some(param_types))
    }
}

impl Checker {
    /// Joins one argument's type into an undeclared by-value variadic's element type.
    ///
    /// The element type joins over every call exactly as a regular parameter does. It did not:
    /// it was fixed by the calls seen before the body was first resolved, so `v(); v("s")` kept
    /// the `int` fallback and `v(1); v("s")` kept `int`, and the body read each string's pointer
    /// as an integer. The `int` fallback is discarded once, as for regular parameters.
    fn widen_variadic_element(
        &mut self,
        name: &str,
        stored_sig: &FunctionSig,
        param_types: &mut [(String, PhpType)],
        actual_elem: PhpType,
    ) -> bool {
        if stored_sig.variadic.is_none() || variadic_param_is_by_ref(stored_sig) {
            return false;
        }
        let variadic_index = param_types.len().saturating_sub(1);
        if stored_sig
            .declared_params
            .get(variadic_index)
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        let Some((_, PhpType::Array(elem))) = param_types.get(variadic_index) else {
            return false;
        };
        let elem = (**elem).clone();
        let key = (name.to_string(), variadic_index);
        let next = if elem == PhpType::Int && !self.param_specialization_seen.contains(&key) {
            self.param_specialization_seen.insert(key);
            actual_elem
        } else {
            Self::union_param_type(&elem, &actual_elem)
        };
        if next == elem {
            return false;
        }
        param_types[variadic_index].1 = PhpType::Array(Box::new(next));
        true
    }
}

/// Returns whether a call argument can specialize a generic `array` parameter.
///
/// Concrete packed and associative arrays are adopted as-is. Exact PHP `array` is a two-member
/// packed/hash union whose codegen representation is Mixed; treating that union as an array
/// shape lets `specialize_generic_array_param_hint` keep the array contract instead of
/// `union_param_type` widening it to Mixed.
fn is_specializable_array_argument(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) || ty.is_php_array()
}

/// Returns true when a call argument is an array whose elements are callable descriptors.
fn is_callable_array_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}

/// Returns whether a stored function signature has a by-reference variadic slot.
fn variadic_param_is_by_ref(sig: &FunctionSig) -> bool {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return false;
    };
    sig.params
        .iter()
        .position(|(name, _)| name == variadic_name)
        .and_then(|index| sig.ref_params.get(index))
        .copied()
        .unwrap_or(false)
}

/// Returns whether a function's variadic container must preserve string keys at runtime.
fn function_variadic_tail_needs_iterable(
    args: &[Expr],
    sig: &FunctionSig,
    regular_param_count: usize,
    env: &TypeEnv,
) -> bool {
    if sig.variadic.is_none() {
        return false;
    }
    if args.iter().any(|arg| {
        matches!(
            &arg.kind,
            ExprKind::Spread(inner) if function_spread_source_keeps_runtime_keys(inner, env)
        )
    }) {
        return true;
    }
    args.iter().any(|arg| {
        matches!(
            &arg.kind,
            ExprKind::NamedArg { name, .. }
                if !sig
                    .params
                    .iter()
                    .take(regular_param_count)
                    .any(|(param_name, _)| param_name == name)
        )
    })
}

/// Returns whether a spread source can carry string keys into a variadic function tail.
fn function_spread_source_keeps_runtime_keys(expr: &Expr, env: &TypeEnv) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => matches!(
            env.get(name),
            Some(PhpType::AssocArray { .. } | PhpType::Iterable)
        ),
        ExprKind::ArrayLiteralAssoc(_) => true,
        _ => matches!(
            crate::types::checker::infer_expr_type_syntactic(expr),
            PhpType::AssocArray { .. } | PhpType::Iterable
        ),
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
