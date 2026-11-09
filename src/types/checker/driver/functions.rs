//! Purpose:
//! Implements the checker driver functions phase.
//! Owns one ordered step in building checker state and validating the program before optimization/codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Phase order controls diagnostics, available declarations, required libraries, and function-local environments.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, Program, StmtKind, TypeExpr};
use crate::source::SourceMode;
use crate::types::FunctionSig;

use super::super::{Checker, FnDecl};

#[cfg(test)]
#[path = "function_variant_tests.rs"]
mod tests;

impl Checker {
    /// Collects top-level function declarations from the program, deduplicating by PHP case-insensitive
    /// symbol key. Emits `DuplicateFunction` for repeats and `CannotRedeclareBuiltin` when a user
    /// function shadows a built-in. Stores `FnDecl` records in `fn_decls` and variant groups in
    /// `function_variant_groups`.
    pub(super) fn collect_function_decls(
        &mut self,
        program: &Program,
        errors: &mut Vec<CompileError>,
    ) {
        let mut seen_functions = HashSet::default();
        for stmt in program {
            if let StmtKind::FunctionVariantGroup { name, variants } = &stmt.kind {
                if !seen_functions.insert(php_symbol_key(name)) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate function declaration: {}", name),
                    ));
                    continue;
                }
                let builtin = crate::strict_php::with_source_mode(stmt.source_mode, || {
                    crate::types::checker::builtins::canonical_builtin_function_name(name)
                        .filter(|builtin| {
                            crate::types::checker::builtins::catalog::builtin_is_available_for_target(
                                builtin,
                                self.target,
                            )
                        })
                });
                if let Some(builtin) = builtin.filter(|_| stmt.source_mode != SourceMode::Internal) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Cannot redeclare built-in function: {}", builtin),
                    ));
                    continue;
                }
                self.function_variant_groups
                    .insert(name.clone(), variants.clone());
                continue;
            }
            if let StmtKind::FunctionDecl {
                name,
                type_params,
                params,
                param_attributes,
                variadic,
                variadic_by_ref,
                variadic_type,
                return_type,
                by_ref_return,
                body,
                ..
            } = &stmt.kind
            {
                if !seen_functions.insert(php_symbol_key(name)) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate function declaration: {}", name),
                    ));
                    continue;
                }
                let builtin = crate::strict_php::with_source_mode(stmt.source_mode, || {
                    crate::types::checker::builtins::canonical_builtin_function_name(name)
                        .filter(|builtin| {
                            crate::types::checker::builtins::catalog::builtin_is_available_for_target(
                                builtin,
                                self.target,
                            )
                        })
                });
                if let Some(builtin) = builtin.filter(|_| stmt.source_mode != SourceMode::Internal) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Cannot redeclare built-in function: {}", builtin),
                    ));
                    continue;
                }
                let param_names: Vec<String> =
                    params.iter().map(|(n, _, _, _)| n.clone()).collect();
                let param_type_anns: Vec<Option<TypeExpr>> =
                    params.iter().map(|(_, t, _, _)| t.clone()).collect();
                // A free function has no lexical class even when called from a method.
                // Normalize before publishing any provisional or specialized signature so
                // every consumer preserves lazy relative-scope errors and rejects new static.
                let defaults: Result<Vec<Option<Expr>>, CompileError> = params.iter()
                    .map(|(_, _, default, _)| {
                        default.as_ref().map(
                            super::super::schema::normalize_free_function_default_in_scope,
                        ).transpose()
                    })
                    .collect();
                let defaults = match defaults {
                    Ok(defaults) => defaults,
                    Err(error) => {
                        errors.push(error);
                        continue;
                    }
                };
                let mut ref_flags: Vec<bool> = params.iter().map(|(_, _, _, r)| *r).collect();
                if variadic.is_some() {
                    ref_flags.push(*variadic_by_ref);
                }
                self.fn_decls.insert(
                    name.clone(),
                    FnDecl {
                        type_params: type_params.clone(),
                        params: param_names,
                        param_types: param_type_anns,
                        param_attributes: param_attributes.clone(),
                        defaults,
                        ref_params: ref_flags,
                        variadic: variadic.clone(),
                        variadic_by_ref: *variadic_by_ref,
                        variadic_type: variadic_type.clone(),
                        return_type: return_type.clone(),
                        by_ref_return: *by_ref_return,
                        span: stmt.span,
                        body: body.clone(),
                        attributes: stmt.attributes.clone(),
                    },
                );
            }
        }
        self.collect_generic_function_variant_templates(errors);
    }

    /// Returns true if `name` resolves to any declared function: user declaration, variant group, or
    /// extern. Resolution is case-insensitive via PHP symbol key matching.
    pub(crate) fn has_function_decl_folded(&self, name: &str) -> bool {
        if self.fn_decls.contains_key(name)
            || self.function_variant_groups.contains_key(name)
            || self.extern_functions.contains_key(name)
        {
            return true;
        }
        // Asked per function reference, and most references name a builtin that no table holds:
        // scanning the three tables each time was a top checker cost on a Symfony build. The
        // tables only grow, so their folded names are refreshed whenever a size changed.
        let shape = (self.fn_decls.len(), self.function_variant_groups.len(), self.extern_functions.len());
        let mut folded = self.folded_function_names.borrow_mut();
        if folded.0 != shape || (folded.1.is_empty() && shape != (0, 0, 0)) {
            folded.1 = self
                .fn_decls
                .keys()
                .chain(self.function_variant_groups.keys())
                .chain(self.extern_functions.keys())
                .map(|existing| existing.to_ascii_lowercase())
                .collect();
            folded.0 = shape;
        }
        folded.1.contains(&name.to_ascii_lowercase())
    }

    /// Returns the canonical (case-matching) name of a user function identified by `name` by
    /// case-insensitive lookup in `functions`, `function_variant_groups`, and `fn_decls`. Returns
    /// `None` if no matching function exists.
    pub(crate) fn canonical_function_name_folded(&self, name: &str) -> Option<String> {
        // Asked per call expression, mostly with builtin names no table holds: three linear
        // scans each time were a top checker cost on a Symfony build. The folded index answers
        // the same thing (first table first), rebuilt when a table grew.
        let shape = (self.functions.len(), self.function_variant_groups.len(), self.fn_decls.len());
        let mut cache = self.canonical_function_names.borrow_mut();
        if cache.as_ref().is_none_or(|(built_for, _)| *built_for != shape) {
            let fold = |keys: &mut dyn Iterator<Item = &String>| {
                let mut index = crate::fast_hash::FastMap::default();
                for key in keys {
                    index.entry(key.to_ascii_lowercase()).or_insert_with(|| key.clone());
                }
                index
            };
            *cache = Some((
                shape,
                [
                    fold(&mut self.functions.keys()),
                    fold(&mut self.function_variant_groups.keys()),
                    fold(&mut self.fn_decls.keys()),
                ],
            ));
        }
        let (_, tables) = cache.as_ref()?;
        let folded = name.to_ascii_lowercase();
        tables.iter().find_map(|table| table.get(&folded).cloned())
    }

    /// Returns the canonical (case-matching) name of an extern function identified by `name` by
    /// case-insensitive lookup in `extern_functions`. Returns `None` if no matching extern exists.
    pub(crate) fn canonical_extern_function_name_folded(&self, name: &str) -> Option<String> {
        folded_map_key(&self.extern_functions, name)
    }

    /// Re-resolves every user function whose return type is inferred, with the parameter types it
    /// already has, and answers whether any return type changed. Diagnostics are not collected:
    /// the first resolution reported them, and the final top-level pass reports what remains.
    pub(super) fn reinfer_undeclared_function_returns(&mut self) -> bool {
        let candidates: Vec<(String, Vec<(String, crate::types::PhpType)>, crate::types::PhpType)> = self
            .functions
            .iter()
            // A `mixed` return is already the widest answer; re-resolving could only narrow it,
            // which is what undid `widen_dynamic_only_passthrough_returns`.
            .filter(|(name, sig)| {
                !sig.declared_return
                    && sig.return_type.codegen_repr() != crate::types::PhpType::Mixed
                    && self.fn_decls.contains_key(*name)
            })
            .map(|(name, sig)| (name.clone(), sig.params.clone(), sig.return_type.clone()))
            .collect();
        let mut changed = false;
        for (name, params, before) in candidates {
            let Some(decl) = self.fn_decls.get(&name).cloned() else {
                continue;
            };
            if self.resolve_function_signature(&name, &decl, params).is_err() {
                continue;
            }
            if self.functions.get(&name).is_some_and(|sig| sig.return_type != before) {
                changed = true;
            }
        }
        changed
    }

    /// Rechecks only functions carrying a scalar-foreach diagnostic after return types settle.
    /// Declared returns (notably void) skip return reinference, but their loop subjects can also
    /// depend on a method's provisional return. Rewalk the body so locals derived from that
    /// return are refreshed too; merely reinferring the subject in its old environment is stale.
    pub(super) fn recheck_foreach_warning_functions(&mut self, errors: &mut Vec<CompileError>) {
        let mut names: Vec<String> = self
            .foreach_warnings
            .values()
            .filter_map(|(function, _)| function.clone())
            .collect();
        names.sort();
        names.dedup();
        for name in names {
            let Some(decl) = self.fn_decls.get(&name).cloned() else {
                continue;
            };
            let Some((params, return_type)) = self
                .functions
                .get(&name)
                .map(|sig| (sig.params.clone(), sig.return_type.clone()))
            else {
                continue;
            };
            if let Err(error) = self.resolve_function_signature(&name, &decl, params) {
                errors.extend(error.flatten());
            }
            // A diagnostic rewalk uses the direct-call parameter types. They cannot narrow a
            // Mixed return already widened for dynamic callers that may supply other values.
            if return_type == crate::types::PhpType::Mixed {
                if let Some(signature) = self.functions.get_mut(&name) {
                    signature.return_type = return_type;
                }
            }
        }
    }

    /// Resolves type signatures for user functions not already resolved during the initial pass,
    /// then resolves variant groups. Appends errors to `errors`.
    pub(super) fn resolve_unchecked_functions(&mut self, errors: &mut Vec<CompileError>) {
        let unchecked: Vec<String> = self
            .fn_decls
            .iter()
            // A TEMPLATE has no signature of its own: its parameter types name type parameters
            // that only a call site can bind, so resolving it here would check the body against
            // `T` and report `Unknown type: T`. Its instantiations are ordinary entries in
            // `fn_decls` and are resolved by this same loop.
            .filter(|(name, decl)| {
                decl.type_params.is_empty() && !self.functions.contains_key(*name)
            })
            .map(|(name, _)| name.clone())
            .collect();
        for name in unchecked {
            if let Err(error) = self.resolve_uncalled_function(&name) {
                errors.extend(error.flatten());
            }
        }
        self.resolve_function_variant_groups(errors);
    }

    /// Resolves the signature of a declared function no call site has reached, reading its
    /// undeclared parameters as `mixed` (see `widen_unrefined_params_of_an_uncalled_function`).
    ///
    /// Shared by the end-of-pass resolution and by `function_exists('name')`, which resolves the
    /// function it names without being a call.
    pub(crate) fn resolve_uncalled_function(&mut self, name: &str) -> Result<(), CompileError> {
        let Some(decl) = self.fn_decls.get(name).cloned() else {
            return Ok(());
        };
        let mut param_types = self.initial_function_param_types(name, &decl)?;
        Self::widen_unrefined_params_of_an_uncalled_function(&decl, &mut param_types);
        self.resolve_function_signature(name, &decl, param_types)
            .map(|_| ())
    }

    /// Resolves a function-variant group no call site has reached, with the same `mixed` reading
    /// of undeclared parameters as [`Self::resolve_uncalled_function`].
    pub(crate) fn resolve_uncalled_function_variant_group(
        &mut self,
        name: &str,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        self.resolve_function_variant_group_signature(name, span, true)
    }

    /// Makes an UNCALLED function's undeclared parameters `mixed` instead of the `Int` seed.
    ///
    /// `initial_function_param_types` seeds an undeclared parameter with `PhpType::Int` as a
    /// gradual starting point that the FIRST call site then discards and replaces with the real
    /// argument type. This path is the one where that call site never comes — the function is
    /// resolved precisely because nothing calls it — so the seed is not a starting point, it is
    /// the final answer, and it is a fabrication: PHP reads an undeclared parameter as `mixed`.
    ///
    /// Left alone it does not stay contained either, because the body is still checked and every
    /// call it makes carries the fabricated `int` outward. Twig's deprecated
    /// `twig_array_filter(Environment $env, $array, $arrow)` is called by nothing, and passing
    /// its two `int` locals to `CoreExtension::filter()` locked that method's `$array` and
    /// `$arrow` to `int` for the whole build — so `$arrow($v, $k)` inside it read as "Cannot call
    /// $arrow — not a callable (got Int)" and `new \IteratorIterator($array)` as "expects
    /// Object(Traversable), got Int", on code PHP runs.
    ///
    /// The methods equivalent already behaves this way: `method_body_param_type` answers `mixed`
    /// for an undeclared parameter that no call site has specialized.
    fn widen_unrefined_params_of_an_uncalled_function(
        decl: &crate::types::checker::FnDecl,
        param_types: &mut [(String, crate::types::PhpType)],
    ) {
        for (idx, (_, param_ty)) in param_types.iter_mut().enumerate() {
            let declared = decl.param_types.get(idx).and_then(|ty| ty.as_ref()).is_some();
            let has_default = decl.defaults.get(idx).and_then(|d| d.as_ref()).is_some();
            let by_ref = decl.ref_params.get(idx).copied().unwrap_or(false);
            // Only the seed itself is replaced: a declared `int`, a default, or a by-reference
            // parameter each carry a real fact and keep it.
            if !declared && !has_default && !by_ref && *param_ty == crate::types::PhpType::Int {
                *param_ty = crate::types::PhpType::Mixed;
            }
        }
    }

    /// Reports whether a real call site ever invoked this declaration with an argument list.
    ///
    /// An include-variant declaration is registered under a GENERATED symbol
    /// (`__elephc_include_variant_{hash}_{local}`) while the call site names the function the way
    /// the source wrote it, so the two never match directly. The mapping is read back out of
    /// `function_variant_groups`, whose key IS the public PHP name and whose values are exactly
    /// the symbols the resolver generated for it — not parsed back out of the symbol's spelling,
    /// which a user function named like a generated variant could imitate.
    ///
    /// A call to `append_item` reaches whichever variant of `append_item` is live, so any variant
    /// of a called name counts as called. That is the conservative direction: the only effect is
    /// to leave a return type alone. A variant reachable solely through a dynamic callable
    /// therefore keeps the placeholder, exactly as it did before the widening existed.
    fn function_was_called_directly(&self, name: &str) -> bool {
        if self.functions_called_directly.contains(name) {
            return true;
        }
        self.function_variant_groups
            .iter()
            .any(|(public_name, variants)| {
                variants.iter().any(|variant| variant == name)
                    && self.functions_called_directly.contains(public_name)
            })
    }

    /// Records `mixed` for every function that returns one of its own untyped parameters and was
    /// never called with an argument list.
    ///
    /// An untyped parameter starts as the `Int` PLACEHOLDER, which a real call site replaces with
    /// the argument's type. A function no call site ever passed anything to keeps it — so a return
    /// inferred from that parameter records `Int` for a value that is really whatever the caller
    /// passes. Reached through `call_user_func`, `array_map` or a first-class callable, the
    /// runtime-callable invoker then coerces through the recorded type and a returned string
    /// arrives as `int(0)`, silently (issue #576).
    ///
    /// The gate is `functions_called_directly`, not membership in `functions`. Those differ for
    /// exactly the shape that matters: `array_map(h(...), […])` RESOLVES `h` while checking the
    /// callable expression, inserting a placeholder-based signature, without ever calling it.
    ///
    /// Gated at all ON PURPOSE. Widening every pass-through body instead feeds back:
    /// `function grow($arr) { … ; return $arr; }` called as `$arr = grow($arr);` would record
    /// `mixed`, the caller's local would become `mixed`, the next call would re-specialize the
    /// parameter to `mixed`, and `array_push($arr, …)` inside the body would stop type-checking.
    pub(super) fn widen_dynamic_only_passthrough_returns(&mut self) {
        let candidates: Vec<String> = self
            .fn_decls
            .keys()
            .filter(|name| !self.function_was_called_directly(name))
            .cloned()
            .collect();
        for name in candidates {
            let Some(decl) = self.fn_decls.get(&name).cloned() else {
                continue;
            };
            let Some(signature) = self.functions.get(&name) else {
                continue;
            };
            if !crate::types::dynamic_params::return_exposes_dynamic_param(
                &decl.body,
                signature,
                &name,
                &self.callable_param_sigs,
            ) {
                continue;
            }
            if let Some(signature) = self.functions.get_mut(&name) {
                signature.return_type = crate::types::PhpType::Mixed;
            }
        }
    }

    /// Iterates all variant groups that are not yet in `functions` and calls
    /// `ensure_function_variant_group_signature` to compute and insert their unified signatures.
    /// Appends errors to `errors`.
    fn resolve_function_variant_groups(&mut self, errors: &mut Vec<CompileError>) {
        let names: Vec<String> = self.function_variant_groups.keys().cloned().collect();
        for name in names {
            // A generic group has no runtime signature until a call binds its parameters.
            if self.fn_decls.get(&name).is_some_and(|decl| !decl.type_params.is_empty()) {
                continue;
            }
            // Nothing called this group, so its variants are resolved the way an uncalled plain
            // function is: see `widen_unrefined_params_of_an_uncalled_function`.
            if let Err(error) =
                self.resolve_uncalled_function_variant_group(&name, crate::span::Span::dummy())
            {
                errors.extend(error.flatten());
            }
        }
    }

    /// Ensures a unified signature exists for a variant group named `name`. If no unified signature
    /// is cached yet, computes a provisional signature from the first variant and inserts it into
    /// `functions`, after checking declared contracts before any recursive body uses it.
    /// Then resolves each individual variant's signature and verifies all variants
    /// share an identical signature. On mismatch, returns an error; on success, inserts the unified
    /// signature and returns `Ok`.
    pub(crate) fn ensure_function_variant_group_signature(
        &mut self,
        name: &str,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        self.resolve_function_variant_group_signature(name, span, false)
    }

    /// Resolves a variant group's unified signature; `uncalled` widens every undeclared
    /// parameter's `int` inference seed to `mixed`, because no call site will ever refine it.
    ///
    /// A conditionally declared function (`if (!function_exists('f')) { function f($x) {…} }`)
    /// becomes a variant group, and the group reached this point with the raw seed: the plain
    /// function path already widened it, this one did not. So Laravel's never-called `lazy($class,
    /// …)` exported `int` for `$class` into `typeFromParameter($class)`, and from there into a
    /// trait method declared `Closure $closure` — a refused build on code php runs.
    fn resolve_function_variant_group_signature(
        &mut self,
        name: &str,
        span: crate::span::Span,
        uncalled: bool,
    ) -> Result<(), CompileError> {
        if let Some(error) = self.failed_variant_contracts.get(name) {
            return Err(error.clone());
        }
        if self.completed_function_signatures.contains(name)
            || (self.functions.contains_key(name)
                && self.function_variant_groups.get(name).is_some_and(|variants| {
                    variants.iter().any(|variant| self.resolving_functions.contains(variant))
                }))
        {
            return Ok(());
        }
        let variants = self
            .function_variant_groups
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;
        // The group's calling convention is its WIDEST variant's: a caller fills every optional
        // parameter any variant declares, and a narrower variant ignores the extra argument.
        //
        // `max_by_key` keeps the LAST of equal keys, so the walk is reversed: on a tie the group's
        // first declared variant stays the reference and the later one is blamed for a conflict.
        let first_variant = variants
            .iter()
            .rev()
            .max_by_key(|variant| {
                self.fn_decls.get(*variant).map_or(0, |decl| decl.params.len())
            })
            .ok_or_else(|| CompileError::new(span, &format!("Function '{}' has no variants", name)))?
            .clone();

        if let Some(provisional) = self.provisional_variant_group_sig(&first_variant, uncalled)? {
            let declared = self.declared_variant_signature(&first_variant)?;
            for variant in variants.iter().filter(|variant| **variant != first_variant) {
                if let Some(candidate) = self.declared_variant_signature(variant)? {
                    let compatible = declared
                        .as_ref()
                        .and_then(|declared| widest_compatible_variant(declared, &candidate))
                        .is_some();
                    if !compatible {
                        let conflict_span = self.fn_decls.get(variant)
                            .map_or(span, |decl| decl.span);
                        let error = CompileError::new(
                            conflict_span,
                            &format!("Function variants for '{}' must have identical signatures", name),
                        );
                        self.failed_variant_contracts.insert(name.to_string(), error.clone());
                        return Err(error);
                    }
                }
            }
            self.functions.insert(name.to_string(), provisional);
        }

        for variant in &variants {
            if self.functions.contains_key(variant) {
                continue;
            }
            let decl = self.fn_decls.get(variant).cloned().ok_or_else(|| {
                CompileError::new(
                    span,
                    &format!(
                        "Compiler error: function variant '{}' for '{}' has no declaration",
                        variant, name
                    ),
                )
            })?;
            let mut param_types = self.initial_function_param_types(variant, &decl)?;
            if uncalled {
                Self::widen_unrefined_params_of_an_uncalled_function(&decl, &mut param_types);
            }
            self.resolve_function_signature(variant, &decl, param_types)?;
        }

        // In-flight and failed bodies both leave placeholders. An in-flight group is
        // revisited after unchecked function resolution, rather than cached as complete.
        // Failed bodies must not invent a signature disagreement from those placeholders.
        if variants.iter().any(|variant| !self.completed_function_signatures.contains(variant)) {
            return Ok(());
        }

        let mut sigs = variants.iter().map(|variant| {
            self.functions.get(variant).cloned().ok_or_else(|| {
                CompileError::new(
                    span,
                    &format!(
                        "Compiler error: function variant '{}' for '{}' has no signature",
                        variant, name
                    ),
                )
            })
        });
        let mut first = sigs
            .next()
            .transpose()?
            .ok_or_else(|| CompileError::new(span, &format!("Function '{}' has no variants", name)))?;
        for (index, sig) in sigs.enumerate() {
            let sig = sig?;
            if let Some(widest) = widest_compatible_variant(&first, &sig) {
                first = widest;
            } else {
                let conflict_span = self.fn_decls.get(&variants[index + 1])
                    .map_or(span, |decl| decl.span);
                return Err(CompileError::new(
                    conflict_span,
                    &format!(
                        "Function variants for '{}' must have identical signatures",
                        name
                    ),
                ));
            }
        }
        self.functions.insert(name.to_string(), first);
        self.completed_function_signatures.insert(name.to_string());
        Ok(())
    }

    /// Builds a provisional `FunctionSig` for the first variant in a group using its declaration and
    /// initial param types. Used as a placeholder when a unified variant-group signature is needed
    /// before individual variants are fully resolved. Returns `Ok(None)` if no declaration exists
    /// for the variant. `uncalled` widens the undeclared parameters' seeds exactly as the variants
    /// themselves are widened, so the placeholder never advertises a type they will not have.
    ///
    ///
    /// A recursive call inside an include-loaded variant names the group, so this placeholder is
    /// what types that self-call while the variant's body is walked. It is built by the same
    /// `provisional_function_sig` the free-function path uses, so the declared return hint (or
    /// `Generator` for a body with `yield`) seeds it instead of a blanket `Int` (issue #635).
    fn provisional_variant_group_sig(
        &mut self,
        first_variant: &str,
        uncalled: bool,
    ) -> Result<Option<FunctionSig>, CompileError> {
        let Some(decl) = self.fn_decls.get(first_variant).cloned() else {
            return Ok(None);
        };
        let mut param_types = self.initial_function_param_types(first_variant, &decl)?;
        if uncalled {
            Self::widen_unrefined_params_of_an_uncalled_function(&decl, &mut param_types);
        }
        Ok(Some(self.provisional_function_sig(&decl, param_types)))
    }

    /// Resolves actual annotations independently of body-driven Generator or Int placeholders.
    fn declared_variant_signature(&mut self, variant: &str) -> Result<Option<FunctionSig>, CompileError> {
        let Some(decl) = self.fn_decls.get(variant).cloned() else { return Ok(None); };
        let params = self.initial_function_param_types(variant, &decl)?;
        let mut sig = self.provisional_function_sig(&decl, params);
        sig.return_type = match &decl.return_type {
            Some(annotation) => self.resolve_declared_return_type_hint(annotation, decl.span, &format!("Function '{}'", variant))?,
            None => crate::types::PhpType::Mixed,
        };
        for (index, (_, ty)) in sig.params.iter_mut().enumerate() {
            *ty = match sig.param_type_exprs.get(index).and_then(Option::as_ref) {
                Some(annotation) => {
                    let resolved = self.resolve_declared_param_type_hint(annotation, decl.span, &format!("Function '{}' parameter", variant))?;
                    if index == decl.params.len() { crate::types::PhpType::Array(Box::new(resolved)) } else { resolved }
                }
                None => crate::types::PhpType::Mixed,
            };
        }
        Ok(Some(sig))
    }

    /// Emits one copy of each group or variant-declaration diagnosis in original error order.
    pub(super) fn deduplicate_variant_signature_errors(&self, errors: &mut Vec<CompileError>) {
        let messages: HashSet<_> = self.function_variant_groups.keys().map(|name| {
            format!("Function variants for '{}' must have identical signatures", name)
        }).collect();
        let declaration_spans: HashSet<_> = self.function_variant_groups.values()
            .flatten().filter_map(|variant| self.fn_decls.get(variant))
            .map(|decl| decl.span).collect();
        let mut seen = HashSet::default();
        errors.retain(|error| {
            (!messages.contains(&error.message) && !declaration_spans.contains(&error.span))
                || seen.insert((error.span, error.file.clone(), error.message.clone()))
        });
    }
}

/// Performs a case-insensitive PHP symbol key lookup on `map` and returns the canonical (case-
/// matching) key if one exists. Used to translate case-insensitive names to their actual declared
/// spelling.
fn folded_map_key<T>(map: &HashMap<String, T>, name: &str) -> Option<String> {
    // A spelling that is itself a key is the canonical one: a case-insensitive symbol table
    // never holds two spellings of one name, so the scan below could only find this same key.
    if map.contains_key(name) {
        return Some(name.to_string());
    }
    map.keys()
        .find(|existing| existing.eq_ignore_ascii_case(name))
        .cloned()
}

#[cfg(test)]
mod folded_lookup_tests {
    use super::*;

    /// The folded-name index answers case-insensitively and picks up a name declared after an
    /// earlier question (the tables grow while the checker runs).
    #[test]
    fn folded_function_lookup_sees_later_declarations() {
        let mut checker = Checker::new(crate::codegen_support::platform::Target::parse("macos-aarch64").unwrap());
        assert!(!checker.has_function_decl_folded("Late\\Fn"));
        checker.function_variant_groups.insert("Late\\Fn".to_string(), Vec::new());
        assert!(checker.has_function_decl_folded("late\\fn"));
        assert!(checker.has_function_decl_folded("LATE\\FN"));
        assert!(!checker.has_function_decl_folded("other"));
    }
}

/// The signature a variant group can call both `a` and `b` through, or `None` when there is none.
///
/// Identical signatures trivially share one. Beyond that, two variants may differ only by
/// OPTIONAL parameters the wider one appends: Symfony's polyfill declares php 8.5's
/// `grapheme_stripos(..., ?string $locale = '')` beside the 8.4 shape, each behind its own
/// `function_exists()` guard in a different bootstrap file, exactly one of which binds. The caller
/// passes the wider list and the narrower body never reads the surplus argument. Any other
/// difference in how arguments or the result are stored, a passing mode, a default, or a
/// variadic stays a refusal.
fn widest_compatible_variant(
    a: &crate::types::FunctionSig,
    b: &crate::types::FunctionSig,
) -> Option<crate::types::FunctionSig> {
    if a == b {
        return Some(a.clone());
    }
    let (narrow, wide) = if a.params.len() <= b.params.len() { (a, b) } else { (b, a) };
    let n = narrow.params.len();
    // Compared by STORAGE, not by declared php type: the call goes through one convention, so
    // what must agree is how each argument and the result are passed. The 8.0 polyfill's
    // untyped `$haystack` and the 8.4 one's `?string $haystack` are both a boxed mixed cell.
    // The defaults must be the same expressions, because the CALLER fills them in.
    let same_storage = |left: &crate::types::PhpType, right: &crate::types::PhpType| {
        left.codegen_repr() == right.codegen_repr()
    };
    let same_shape = narrow.variadic.is_none()
        && wide.variadic.is_none()
        && same_storage(&narrow.return_type, &wide.return_type)
        && narrow.by_ref_return == wide.by_ref_return
        && narrow.is_generator == wide.is_generator
        && narrow
            .params
            .iter()
            .zip(&wide.params[..n])
            .all(|((_, left), (_, right))| same_storage(left, right))
        && narrow.defaults[..] == wide.defaults[..n]
        && narrow.ref_params[..] == wide.ref_params[..n];
    let surplus_optional = wide.defaults[n..].iter().all(Option::is_some)
        && !wide.ref_params[n..].iter().any(|by_ref| *by_ref);
    (same_shape && surplus_optional).then(|| wide.clone())
}
