//! Purpose:
//! Handles function resolution signature details for call checking.
//! Materializes signatures or specialized metadata used by argument validation and return inference.
//!
//! Called from:
//! - `crate::types::checker::functions::resolution`
//!
//! Key details:
//! - Specialized and builtin signatures must expose the caller-visible parameter contract expected by call-argument planning.
//! - Callable-array target provenance is local-frame state. A body check starts with empty target
//!   maps and restores the caller's maps before call-site validation resumes.


use crate::errors::CompileError;
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::super::{Checker, FnDecl};

impl Checker {
    /// Resolves a function's signature given its declaration, parameter types, and body.
    ///
    /// Builds a `TypeEnv` from the provided parameter types, then type-checks the body
    /// while collecting return type information. Handles callable parameters by saving and
    /// restoring their metadata around the body check. Validates declared return types
    /// against inferred returns, and applies PHP's generator rules (functions containing
    /// `yield` implicitly return `Generator`). Stores the final signature in `self.functions`.
    ///
    /// Returns the resolved return type, or a `CompileError` if the body fails to type-check
    /// or return types are incompatible with any declared annotation.
    ///
    /// A recursive self-call that widens the parameters (`recursive_param_widenings`) re-runs the
    /// body with the widened types until they stop changing. A round that failed is retried the
    /// same way, since its error may only exist under the narrower types.
    pub(crate) fn resolve_function_signature(
        &mut self,
        name: &str,
        decl: &FnDecl,
        param_types: Vec<(String, PhpType)>,
    ) -> Result<PhpType, CompileError> {
        self.completed_function_signatures.remove(name);
        let result = self.resolve_function_signature_rounds(name, decl, param_types.clone());
        // A call-site-specialized parameter type is an optimization, not PHP semantics: the
        // parameter is untyped, so php reads `$o->secret` at run time and raises the catchable
        // `Error` for an inaccessible name only when that call executes. Specializing the
        // parameter to the one class a call passes turned that into a compile error -- or, when
        // the failing round ran inside a loop's storage pre-pass (which drops its errors), into a
        // silently poisoned signature whose placeholder `int` return type read a string as `0`.
        // So a visibility refusal retries with the specialized parameters read as `mixed`.
        match result {
            Err(error) if Self::is_member_visibility_refusal(&error) => {
                let mut widened = param_types.clone();
                let mut changed = false;
                for (idx, (_, param_ty)) in widened.iter_mut().enumerate() {
                    let undeclared = decl.param_types.get(idx).map_or(true, |ann| ann.is_none());
                    let by_ref = decl.ref_params.get(idx).copied().unwrap_or(false);
                    if undeclared
                        && !by_ref
                        && matches!(param_ty, PhpType::Object(_) | PhpType::Union(_))
                    {
                        *param_ty = PhpType::Mixed;
                        changed = true;
                    }
                }
                if !changed {
                    return Err(error);
                }
                self.resolve_function_signature_rounds(name, decl, widened)
                    .or(Err(error))
            }
            other => other,
        }
    }

    /// Returns whether every diagnostic is php's runtime visibility refusal for a property.
    fn is_member_visibility_refusal(error: &CompileError) -> bool {
        error.flatten().iter().all(|error| {
            error.message.starts_with("Cannot access ") && error.message.contains(" property")
        })
    }

    /// The body-check rounds of `resolve_function_signature`, without the visibility retry.
    fn resolve_function_signature_rounds(
        &mut self,
        name: &str,
        decl: &FnDecl,
        param_types: Vec<(String, PhpType)>,
    ) -> Result<PhpType, CompileError> {
        // Types only widen, so a handful of rounds reaches the fixpoint; the bound only guards
        // against a widening that oscillates.
        const MAX_RECURSIVE_WIDENING_ROUNDS: usize = 8;
        let mut param_types = param_types;
        for _ in 0..MAX_RECURSIVE_WIDENING_ROUNDS {
            self.recursive_param_widenings.remove(name);
            let result = self.resolve_function_signature_once(name, decl, param_types.clone());
            match self.recursive_param_widenings.remove(name) {
                Some(widened) if widened != param_types => param_types = widened,
                _ => return result,
            }
        }
        self.resolve_function_signature_once(name, decl, param_types)
    }

    /// One body check of `resolve_function_signature` with fixed parameter types.
    fn resolve_function_signature_once(
        &mut self,
        name: &str,
        decl: &FnDecl,
        param_types: Vec<(String, PhpType)>,
    ) -> Result<PhpType, CompileError> {
        let mut local_env: TypeEnv = TypeEnv::default();
        for (pname, pty) in &param_types {
            // A by-reference parameter this body widens is bound `mixed` for the whole body,
            // which is the representation its cell really has once `scan_widened_ref_params` has
            // decided so. Binding it at the DECLARED type instead would make the very store that
            // caused the decision the hard `cannot reassign` error the decision exists to remove.
            // Only the ENVIRONMENT is widened — `param_types` still builds the signature below,
            // so the call boundary keeps validating arguments against the declared type.
            local_env.insert(
                pname.clone(),
                self.widened_ref_param_env_type(name, pname, pty),
            );
        }
        // Seed the request superglobals so a function body can read/write
        // `$_SERVER`/`$_GET`/`$_POST` without a `global` declaration. `or_insert`
        // never clobbers a parameter that happens to share a superglobal name.
        for name in crate::superglobals::SUPERGLOBALS {
            local_env
                .entry((*name).to_string())
                .or_insert_with(crate::superglobals::superglobal_type);
        }
        Self::seed_vivified_array_locals(&mut local_env, &decl.body);
        let function_key = name.to_string();
        let callable_param_names: Vec<String> = param_types
            .iter()
            .filter(|(_, pty)| {
                pty == &PhpType::Callable || is_callable_array_return_type(pty)
            })
            .map(|(pname, _)| pname.clone())
            .collect();
        let declared_callable_param_names: Vec<String> = param_types
            .iter()
            .enumerate()
            .filter(|(idx, (_, pty))| {
                pty == &PhpType::Callable
                    && decl.param_types.get(*idx).is_some_and(|type_ann| type_ann.is_some())
            })
            .map(|(_, (pname, _))| pname.clone())
            .collect();
        let saved_callable_param_names = self.callable_param_names.clone();
        // Target records name storage in one physical frame. A same-named parameter or local in
        // this function must neither inherit the caller's target nor let a loop/try join clear
        // the caller's record while the body is re-specialized. Callable parameter signatures
        // have their dedicated channel below; callable-array locals rebuild their records from
        // assignments inside this body.
        let saved_callable_array_targets = std::mem::take(&mut self.callable_array_targets);
        let saved_callable_array_target_versions =
            std::mem::take(&mut self.callable_array_target_versions);
        for pname in &declared_callable_param_names {
            self.callable_param_names.insert(pname.clone());
        }
        let saved_callable_metadata: Vec<_> = callable_param_names
            .iter()
            .map(|pname| {
                (
                    pname.clone(),
                    self.callable_sigs.get(pname).cloned(),
                    self.closure_return_types.get(pname).cloned(),
                )
            })
            .collect();
        // The callable TARGETS recorded for a variable (`$cb = [$obj, 'm']`, `$cb = f(...)`) are
        // keyed by name alone. Every parameter is a fresh binding, so a caller's `$callback`
        // must not describe this body's `$callback`: it did, and the preg prelude's own
        // `preg_replace_callback($regex, $callback, ...)` re-checked the caller's
        // `[$first, 'replace']` inside the prelude, where `$first` is undefined.
        let hidden_callable_targets: Vec<_> = param_types
            .iter()
            .map(|(pname, _)| {
                (
                    pname.clone(),
                    self.callable_array_targets.remove(pname),
                    self.first_class_callable_targets.remove(pname),
                )
            })
            .collect();
        for pname in &callable_param_names {
            if let Some(sig) = self
                .callable_param_sigs
                .get(&(function_key.clone(), pname.clone()))
                .cloned()
            {
                self.closure_return_types
                    .insert(pname.clone(), sig.return_type.clone());
                self.callable_sigs.insert(pname.clone(), sig);
            } else if let Some(sig) = param_types
                .iter()
                .position(|(name, _)| name == pname)
                .and_then(|index| decl.param_types.get(index))
                .and_then(|type_ann| type_ann.as_ref())
                .and_then(|type_ann| self.declared_callable_signature(type_ann, decl.span))
            {
                // No call site told us what this callable is, but the DECLARATION did.
                self.closure_return_types
                    .insert(pname.clone(), sig.return_type.clone());
                self.callable_sigs.insert(pname.clone(), sig);
            } else {
                self.closure_return_types.remove(pname);
                self.callable_sigs.remove(pname);
            }
        }

        let provisional_sig = self.provisional_function_sig(decl, param_types.clone());
        self.functions.insert(name.to_string(), provisional_sig);

        let mut return_type = PhpType::Void;
        let mut all_return_infos = Vec::new();
        let mut callable_return_sigs = Vec::new();
        let mut callable_array_return_sigs = Vec::new();
        let mut errors = Vec::new();
        let ref_param_names: Vec<String> = decl
            .params
            .iter()
            .zip(decl.ref_params.iter())
            .filter(|(_, is_ref)| **is_ref)
            .map(|(name, _)| name.clone())
            .collect();
        // Every parameter is bound unconditionally on entry, so all of them are recorded at
        // binding depth 0 (a missing entry means "seeded, not bound here", which is not
        // kill/retype eligible).
        let param_names: Vec<String> = decl
            .params
            .iter()
            .cloned()
            .chain(decl.variadic.iter().cloned())
            .collect();
        let prev_by_ref_return = self.current_by_ref_return;
        self.current_by_ref_return = decl.by_ref_return;
        self.clear_loop_storage_scope(&function_key);
        let previous_loop_storage_scope =
            std::mem::replace(&mut self.current_loop_storage_scope, function_key.clone());
        self.resolving_functions.insert(function_key.clone());
        // Track the innermost function body being walked (nested resolution of
        // a called function saves and restores the enclosing name), so friend
        // channels can identify compiler-owned procedural aliases.
        let previous_function = self.current_function.replace(function_key.clone());
        // The storage this frame already holds on entry: the parameters, with the types this
        // call site specialized them to. The superglobals `local_env` also carries are not the
        // frame's own storage and are excluded from marking outright, so they are left out.
        let pre_bound_own_storage: std::collections::HashMap<String, PhpType> =
            param_types.iter().cloned().collect();
        let body_check_result = self.with_local_storage_context(
            ref_param_names,
            param_names,
            pre_bound_own_storage,
            &decl.body,
            |checker| {
                checker.boxed_parameter_locals.extend(decl.params.iter().enumerate()
                    .filter(|(index, _)| decl.param_types.get(*index).is_none_or(Option::is_none))
                    .map(|(_, name)| name.clone()));
                for stmt in &decl.body {
                    if let Err(error) = checker.check_stmt(stmt, &mut local_env) {
                        // Naming the enclosing function is what lets `pipeline` recover the FILE:
                        // after autoload expansion every spliced file shares one line-number
                        // space, so `line:col` alone identifies nothing.
                        errors.extend(
                            error.within_declaration(function_key.as_str()).flatten(),
                        );
                    }
                    checker.collect_return_infos(stmt, &local_env, &mut all_return_infos);
                    checker.collect_return_callable_sigs(
                        stmt,
                        &local_env,
                        &mut callable_return_sigs,
                    );
                    checker.collect_return_callable_array_sigs(
                        stmt,
                        &local_env,
                        &mut callable_array_return_sigs,
                    );
                }
                Ok(())
            },
        );
        self.current_function = previous_function;
        self.resolving_functions.remove(&function_key);
        self.current_loop_storage_scope = previous_loop_storage_scope;
        self.current_by_ref_return = prev_by_ref_return;
        self.callable_param_names = saved_callable_param_names;
        for (pname, array_target, first_class_target) in hidden_callable_targets {
            match array_target {
                Some(target) => {
                    self.callable_array_targets.insert(pname.clone(), target);
                }
                None => {
                    self.callable_array_targets.remove(&pname);
                }
            }
            match first_class_target {
                Some(target) => {
                    self.first_class_callable_targets.insert(pname, target);
                }
                None => {
                    self.first_class_callable_targets.remove(&pname);
                }
            }
        }
        self.callable_array_targets = saved_callable_array_targets;
        self.callable_array_target_versions = saved_callable_array_target_versions;
        body_check_result?;
        for pname in &callable_param_names {
            if let Some(sig) = self.callable_sigs.get(pname).cloned() {
                self.callable_param_sigs
                    .insert((function_key.clone(), pname.clone()), sig);
            }
        }
        for (pname, saved_sig, saved_return) in saved_callable_metadata {
            if let Some(sig) = saved_sig {
                self.callable_sigs.insert(pname.clone(), sig);
            } else {
                self.callable_sigs.remove(&pname);
            }
            if let Some(return_ty) = saved_return {
                self.closure_return_types.insert(pname, return_ty);
            } else {
                self.closure_return_types.remove(&pname);
            }
        }
        if !errors.is_empty() {
            return Err(CompileError::from_many(errors));
        }

        let contains_yield = super::super::super::yield_validation::body_contains_yield(&decl.body);
        if contains_yield {
            let generator_ty = PhpType::Object("Generator".to_string());
            if let Some(type_ann) = decl.return_type.as_ref() {
                let declared_ret = self.resolve_declared_return_type_hint(
                    type_ann,
                    decl.span,
                    &format!("Function '{}'", name),
                )?;
                if !self.generator_return_type_accepts(&declared_ret) {
                    self.require_compatible_return_hint(
                        type_ann,
                        &declared_ret,
                        &generator_ty,
                        true,
                        true,
                        decl.span,
                        &format!("Function '{}' return type", name),
                    )?;
                }
            }
            return_type = generator_ty;
        } else if let Some(type_ann) = decl.return_type.as_ref() {
            let declared_ret = self.resolve_declared_return_type_hint(
                type_ann,
                decl.span,
                &format!("Function '{}'", name),
            )?;
            if matches!(declared_ret, PhpType::Never) && Self::body_contains_return(&decl.body) {
                return Err(CompileError::new(
                    decl.span,
                    &format!("Function '{}' declared never must not return", name),
                ));
            }
            self.require_return_hint_coverage(
                type_ann,
                &declared_ret,
                &decl.body,
                decl.span,
                &format!("Function '{}'", name),
            )?;
            if !all_return_infos.is_empty() {
                for return_info in &all_return_infos {
                    self.require_compatible_return_hint(
                        type_ann,
                        &declared_ret,
                        &return_info.ty,
                        return_info.has_value,
                        return_info.strict_types,
                        decl.span,
                        &format!("Function '{}' return type", name),
                    )?;
                }
            }
            return_type = if Self::is_generic_array_hint(&declared_ret) {
                Self::generic_array_return_contract(&all_return_infos)
                .unwrap_or(declared_ret)
            } else {
                declared_ret
            };
        } else if !all_return_infos.is_empty() {
            return_type = all_return_infos[0].ty.clone();
            for return_info in &all_return_infos[1..] {
                return_type = Self::wider_type(&return_type, &return_info.ty);
            }
            if !decl.by_ref_return {
                return_type = self.widen_return_for_fall_off(&decl.body, return_type);
            }
        }

        let sig = FunctionSig {
            params: param_types,
            param_type_exprs: decl
                .param_types
                .iter()
                .cloned()
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.clone()))
                .collect(),
            param_attributes: decl.param_attributes.clone(),
            defaults: decl.defaults.clone(),
            return_type: return_type.clone(),
            declared_return: decl.return_type.is_some(),
            by_ref_return: decl.by_ref_return,
            ref_params: decl.ref_params.clone(),
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.is_some()))
                .collect(),
            variadic: decl.variadic.clone(),
            deprecation: crate::types::checker::schema::validation::extract_deprecation(
                &decl.attributes,
            ),
            // The syntactic fact, taken from the SOURCE body before any pass has run. `decl.body`
            // here is the declaration as parsed, so an unreachable `yield` still counts -- which
            // is PHP's rule, and the thing lowering must not try to re-derive later.
            is_generator: contains_yield,
        };
        self.functions.insert(name.to_string(), sig);
        if return_type == PhpType::Callable {
            if let Some(callable_sig) = matching_callable_sig(&callable_return_sigs) {
                self.callable_return_sigs
                    .insert(name.to_string(), callable_sig);
            } else {
                self.callable_return_sigs.remove(name);
            }
        } else {
            self.callable_return_sigs.remove(name);
        }
        if is_callable_array_return_type(&return_type) {
            if let Some(callable_sig) = matching_callable_sig(&callable_array_return_sigs) {
                self.callable_array_return_sigs
                    .insert(name.to_string(), callable_sig);
            } else {
                self.callable_array_return_sigs.remove(name);
            }
        } else {
            self.callable_array_return_sigs.remove(name);
        }

        self.completed_function_signatures.insert(name.to_string());
        Ok(return_type)
    }

    /// Builds the *provisional* signature published before a free function's body is walked.
    ///
    /// Shared by the two places that publish one: `resolve_function_signature` (under the
    /// function's own name) and the variant-group path in
    /// `crate::types::checker::driver::functions` (under the group's PHP-visible name, which is
    /// what a recursive call inside an include-loaded variant spells). Keeping one builder keeps
    /// one provisional policy: a variant group used to publish its own copy with a hardcoded
    /// `Int` return, so a recursive include-loaded `: string` function was rejected with
    /// `return type expects Str, got Int` although the same function compiled when declared in
    /// the main file (issue #635).
    pub(in crate::types::checker) fn provisional_function_sig(
        &self,
        decl: &FnDecl,
        param_types: Vec<(String, PhpType)>,
    ) -> FunctionSig {
        FunctionSig {
            params: param_types,
            param_type_exprs: decl
                .param_types
                .iter()
                .cloned()
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.clone()))
                .collect(),
            param_attributes: decl.param_attributes.clone(),
            defaults: decl.defaults.clone(),
            return_type: self.provisional_return_type(decl),
            declared_return: decl.return_type.is_some(),
            by_ref_return: decl.by_ref_return,
            ref_params: decl.ref_params.clone(),
            deprecation: None,
            is_generator: false,
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.is_some()))
                .collect(),
            variadic: decl.variadic.clone(),
        }
    }

    /// Picks the return type for the *provisional* signature published before a free
    /// function's body is walked.
    ///
    /// The provisional signature exists so that a call to the function from inside its own
    /// body — direct recursion, or a mutual-recursion cycle re-entering a function already
    /// on the resolution stack — can be typed at all. Whatever this returns is what such a
    /// self-call's expression type will be, since `resolving_functions` suppresses
    /// re-specialization for in-flight functions (see `specialization.rs`).
    ///
    /// Resolution order mirrors the authoritative pass that runs after the body walk:
    /// - a body containing `yield` always produces a `Generator`, whatever the annotation says;
    /// - otherwise an explicit return hint is trusted verbatim;
    /// - an unhinted function keeps the historical `Int` placeholder. That placeholder is not
    ///   observable for a function with a base case, because the final unhinted return type is
    ///   the `wider_type` merge of every `return` and the base case's real type absorbs it. It
    ///   only survives for a function whose *only* return is the recursive call, which is
    ///   unconditional infinite recursion and cannot produce a value anyway.
    ///
    /// A hint that fails to resolve (unknown class, misplaced `never`) falls back to the
    /// placeholder rather than raising here: the authoritative pass resolves the same hint
    /// again and reports that error with its original span and ordering.
    fn provisional_return_type(&self, decl: &FnDecl) -> PhpType {
        if super::super::super::yield_validation::body_contains_yield(&decl.body) {
            return PhpType::Object("Generator".to_string());
        }
        let Some(type_ann) = decl.return_type.as_ref() else {
            return PhpType::Int;
        };
        self.resolve_declared_return_type_hint(type_ann, decl.span, "")
            .unwrap_or(PhpType::Int)
    }

    /// Returns true when a declared generator return annotation accepts
    /// the actual `Generator` object returned when the body contains `yield`.
    ///
    /// Shared with the method pass (`crate::types::checker::method_pass`) so a
    /// generator method's hint is validated by exactly the same rule as a
    /// generator function's.
    pub(crate) fn generator_return_type_accepts(&self, declared_ret: &PhpType) -> bool {
        if matches!(declared_ret, PhpType::Object(name) if name == "Traversable") {
            return true;
        }
        self.type_accepts(declared_ret, &PhpType::Object("Generator".to_string()))
    }
}


/// Returns true when a function return type is a homogeneous array of callables.
fn is_callable_array_return_type(return_type: &PhpType) -> bool {
    match return_type {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}

/// Computes the callable signature metadata for matching callable.
fn matching_callable_sig(return_sigs: &[FunctionSig]) -> Option<FunctionSig> {
    let first = return_sigs.first()?.clone();
    if return_sigs.iter().all(|sig| sig == &first) {
        Some(callable_return_codegen_sig(first))
    } else {
        None
    }
}

/// Computes the callable signature metadata for callable return codegen.
///
/// An undeclared parameter of the returned closure stays `mixed`, which is what the closure is
/// compiled with (`__eir_closure_*(x: Heap(Mixed))`) and what a call through the descriptor boxes
/// every argument to. It used to be rewritten to `int`, a contract the closure never had:
/// `$f = make(); $f("str")` was refused "callable $f parameter $x expects Int, got Str", and so was
/// Laravel's `array_reduce($pipes, $this->carry(), $destination)` with its closure carry.
fn callable_return_codegen_sig(sig: FunctionSig) -> FunctionSig {
    sig
}
