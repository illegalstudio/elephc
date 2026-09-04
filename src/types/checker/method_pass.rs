//! Purpose:
//! Runs method-body validation once class and interface schemas are available.
//! Checks instance/static context, declared returns, visibility-sensitive access, and inherited method contracts.
//!
//! Called from:
//! - `crate::types::checker::driver::functions`
//!
//! Key details:
//! - Method checking depends on flattened class metadata and must preserve `self`, `parent`, and `$this` context.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::ClassMethod;
use crate::types::{traits::FlattenedClass, FunctionSig, PhpType, TypeEnv};

use super::Checker;

impl Checker {
    /// Runs method-body validation in passes until class type information stabilizes.
    ///
    /// Each pass type-checks every non-abstract method body, collecting return types and
    /// errors. If a pass changes `self.classes` (e.g., via inferred return types), another
    /// pass runs. Iteration stops when types stabilize or `2 * class_count + 1` passes
    /// are exhausted.
    ///
    /// For non-static methods, `$this` is inserted into the per-method `TypeEnv` as an
    /// `Object` of the declaring class. Parameters are resolved against declared type hints
    /// or inferred from the class signature; variadic parameters use `PhpType::Array(Int)`
    /// as a fallback.
    ///
    /// Sets `self.current_class`, `self.current_method`, and `self.current_method_is_static`
    /// during body checking to enable context-sensitive diagnostics.
    pub(super) fn type_check_methods_until_stable(
        &mut self,
        flattened_classes: &[FlattenedClass],
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let mut method_passes_remaining = (flattened_classes.len().max(1) * 2) + 1;
        loop {
            let classes_before_pass = self.classes.clone();
            let mut pass_errors = Vec::new();

            for class in flattened_classes {
                for method in &class.methods {
                    if method.is_abstract {
                        continue;
                    }
                    let method_key = php_symbol_key(&method.name);
                    let mut method_env = Self::seed_method_env();
                    if !method.is_static {
                        method_env.insert("this".to_string(), PhpType::Object(class.name.clone()));
                    }
                    let sig_params = if method.is_static {
                        self.classes
                            .get(&class.name)
                            .and_then(|c| c.static_methods.get(&method_key))
                            .map(|s| s.params.clone())
                    } else {
                        self.classes
                            .get(&class.name)
                            .and_then(|c| c.methods.get(&method_key))
                            .map(|s| s.params.clone())
                    };
                    for (i, (pname, type_ann, _, _)) in method.params.iter().enumerate() {
                        let ty = if let Some(type_ann) = type_ann {
                            let declared = self.resolve_declared_param_type_hint(
                                type_ann,
                                method.span,
                                &format!("Method parameter ${}", pname),
                            )?;
                            // A generic `array` hint is sharpened to the call-site array shape
                            // recorded on the stored signature, mirroring how free-function
                            // `array` parameters are specialized (issue #406). Without this a
                            // method `array` parameter stays an integer-indexed list and rejects
                            // string-key access / mis-encodes associative arrays.
                            if Self::is_generic_array_hint(&declared) {
                                sig_params
                                    .as_ref()
                                    .and_then(|p| p.get(i))
                                    .map(|(_, t)| t.clone())
                                    .filter(|t| {
                                        matches!(
                                            t,
                                            PhpType::Array(_) | PhpType::AssocArray { .. }
                                        )
                                    })
                                    .map(|t| {
                                        Self::specialize_generic_array_param_hint(&declared, &t)
                                    })
                                    .unwrap_or(declared)
                            } else {
                                declared
                            }
                        } else {
                            sig_params
                                .as_ref()
                                .and_then(|p| p.get(i))
                                .map(|(_, t)| t.clone())
                                .unwrap_or(PhpType::Int)
                        };
                        // PHP's __unserialize($data) always receives the associative
                        // array produced by __serialize(); a bare `array` hint resolves
                        // to an indexed Array(Mixed) that rejects $data['key']. Type the
                        // first parameter as a string/int-keyed assoc array so the body
                        // can read string keys, matching the bare hash the unserialize
                        // runtime passes in (kept in sync with build_method_sig). Scoped
                        // to user methods (real span); synthetic SPL bodies keep `array`.
                        let ty = if method_key == "__unserialize" && i == 0 && method.span.line != 0 {
                            PhpType::AssocArray {
                                key: Box::new(PhpType::Mixed),
                                value: Box::new(PhpType::Mixed),
                            }
                        } else {
                            ty
                        };
                        method_env.insert(pname.clone(), ty);
                    }
                    if let Some(variadic_name) = &method.variadic {
                        let fallback_ty = if method.variadic_by_ref {
                            PhpType::Array(Box::new(PhpType::Mixed))
                        } else {
                            PhpType::Array(Box::new(PhpType::Int))
                        };
                        let ty = sig_params
                            .as_ref()
                            .and_then(|p| p.get(method.params.len()))
                            .map(|(_, t)| t.clone())
                            .unwrap_or(fallback_ty);
                        method_env.insert(variadic_name.clone(), ty);
                    }
                    if method_key == "__construct" {
                        self.patch_constructor_method_env(class, method, &mut method_env);
                    }
                    self.patch_stream_contract_method_env(class, method, &mut method_env);

                    self.current_class = Some(class.name.clone());
                    self.current_method = Some(method_key.clone());
                    self.current_method_is_static = method.is_static;
                    self.current_by_ref_return = method.by_ref_return;
                    let loop_storage_scope = format!("{}::{}", class.name, method.name);
                    self.loop_storage_types
                        .retain(|(scope, _), _| scope != &loop_storage_scope);
                    let previous_loop_storage_scope = std::mem::replace(
                        &mut self.current_loop_storage_scope,
                        loop_storage_scope,
                    );
                    let method_ref_params: Vec<String> = method
                        .params
                        .iter()
                        .filter(|(_, _, _, is_ref)| *is_ref)
                        .map(|(name, _, _, _)| name.clone())
                        .collect();
                    // Every parameter is bound unconditionally on entry, so all of them are
                    // recorded at binding depth 0 (a missing entry means "seeded, not bound
                    // here", which is not kill/retype eligible).
                    let method_param_names: Vec<String> = method
                        .params
                        .iter()
                        .map(|(name, _, _, _)| name.clone())
                        .chain(method.variadic.iter().cloned())
                        .collect();
                    // A parameter with a declared type hint is a contract: never kill/retype
                    // eligible inside the body, in either mode.
                    let method_typed_params: Vec<String> = method
                        .params
                        .iter()
                        .filter(|(_, type_ann, _, _)| type_ann.is_some())
                        .map(|(name, _, _, _)| name.clone())
                        .chain(
                            method
                                .variadic
                                .iter()
                                .filter(|_| method.variadic_type.is_some())
                                .cloned(),
                        )
                        .collect();
                    let mut method_errors = Vec::new();
                    // The storage this frame already holds on entry: the parameters. `$this`,
                    // the superglobals and the seeded globals `method_env` also carries are not
                    // this frame's own storage, and none of them is markable anyway.
                    let pre_bound_own_storage: std::collections::HashMap<String, PhpType> =
                        method_param_names
                            .iter()
                            .filter_map(|name| {
                                method_env.get(name).map(|ty| (name.clone(), ty.clone()))
                            })
                            .collect();
                    self.with_local_storage_context(
                        method_ref_params,
                        method_param_names,
                        method_typed_params,
                        pre_bound_own_storage,
                        &method.body,
                        |checker| {
                            for s in &method.body {
                                if let Err(error) = checker.check_stmt(s, &mut method_env) {
                                    method_errors.extend(error.flatten());
                                }
                            }
                            Ok(())
                        },
                    )?;
                    let method_has_errors = !method_errors.is_empty();
                    pass_errors.extend(method_errors);

                    if !method_has_errors {
                        self.update_method_return_type(class, method, &method_env, &mut pass_errors);
                        self.update_method_by_ref_param_types(class, method, &method_env);
                    }
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                    self.current_by_ref_return = false;
                    self.current_loop_storage_scope = previous_loop_storage_scope;
                }
            }

            let stabilized = self.classes == classes_before_pass;
            let out_of_passes = method_passes_remaining == 0;
            if stabilized || out_of_passes {
                errors.extend(pass_errors);
                break;
            }

            method_passes_remaining -= 1;
        }
        Ok(())
    }

    /// Builds the PHP-local base environment shared by all method bodies.
    ///
    /// Methods can read request superglobals without a `global` declaration, but
    /// ordinary top-level locals belong to file scope and must not leak into a
    /// method. Explicit `global` statements resolve through `self.top_level_env`.
    fn seed_method_env() -> TypeEnv {
        crate::superglobals::SUPERGLOBALS
            .iter()
            .map(|name| ((*name).to_string(), crate::superglobals::superglobal_type()))
            .collect()
    }

    /// Patches an untyped stream-wrapper contract parameter with the type PHP documents for it.
    ///
    /// A wrapper's methods are reached through a runtime vtable with raw fixed-ABI arguments, so
    /// `normalize_method_map_for_eir` deliberately leaves their untyped parameters alone rather
    /// than widening them to boxed Mixed, which would desynchronize the dispatcher from the body.
    /// The consequence was that they kept the `Int` an untyped parameter is seeded with, and an
    /// ordinary `stream_write($data) { return strlen($data); }` — the signature the manual shows —
    /// failed to compile with `strlen() argument must be string`.
    ///
    /// The dispatcher's argument types are not unknown, only undeclared: PHP specifies them. So
    /// they are seeded here instead of widened, which leaves the fixed ABI exactly as it was and
    /// lets the body use its own arguments. Only parameters WITHOUT a type hint are touched, and
    /// only in a class that really is a wrapper — a lone `stream_write()` on an unrelated class
    /// keeps its inference, because a method name is not a contract.
    fn patch_stream_contract_method_env(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
        method_env: &mut TypeEnv,
    ) {
        let Some(ci) = self.classes.get(&class.name).cloned() else {
            return;
        };
        if !declares_stream_wrapper_marker(&ci) && !declares_stream_filter_marker(&ci) {
            return;
        }
        let key = crate::names::php_symbol_key(&method.name);
        let Some(contract) = stream_wrapper_contract_param_types(&key) else {
            return;
        };
        for (i, (pname, type_ann, _, _)) in method.params.iter().enumerate() {
            if type_ann.is_some() {
                continue;
            }
            let Some(Some(ty)) = contract.get(i) else { continue };
            method_env.insert(pname.clone(), ty.clone());
            if let Some(ci_mut) = self.classes.get_mut(&class.name) {
                if let Some(sig) = ci_mut.methods.get_mut(&key) {
                    if i < sig.params.len() {
                        sig.params[i].1 = ty.clone();
                    }
                }
            }
        }
    }

    /// Pins the return type of an undeclared stream-wrapper contract method to the contract's.
    ///
    /// The wrapper's methods are reached through a runtime vtable with a FIXED ABI, and that ABI
    /// is chosen by the return type: `stream_read()` and `dir_readdir()` are read back out of the
    /// string-result pair (x1/x2 on AArch64, rax/rdx on x86_64), while every scalar return travels
    /// in the single result register. Inference does not know that, so an ordinary
    /// `stream_read($n) { return fread($this->fh, $n); }` — `fread()` answers `string|false`, so the
    /// method infers `mixed` — returned a BOXED cell where the dispatcher reads a pointer/length
    /// pair, and the wrapper handed back the box's own bytes instead of the data it read.
    ///
    /// On AArch64 the boxed cell lands in x0 while the dispatcher reads x1/x2, which the body
    /// happened to leave holding the last string it built, so the defect stayed invisible; on
    /// x86_64 the boxed cell lands in rax, which IS the pointer half of the pair, and the wrapper
    /// answered three bytes of the box. Pinning the type here converts at the return instead, so
    /// both arches hand over the same pair — the same seeding [`patch_stream_contract_method_env`]
    /// already does for the contract's PARAMETERS, and for the same reason.
    ///
    /// Only methods WITHOUT a return hint are touched, only on a class that really is a wrapper,
    /// and only when the body returns values at all: a hint is the author's own answer, a lone
    /// `stream_read()` on an unrelated class is not a contract, and a body with no value return
    /// keeps the `void` it infers rather than being promised a string it never produces.
    fn stream_contract_return_type(
        &self,
        class: &FlattenedClass,
        method: &ClassMethod,
        raw_inferred: &Option<PhpType>,
    ) -> Option<PhpType> {
        if method.is_static {
            return None;
        }
        if raw_inferred.is_none() {
            return None;
        }
        let class_info = self.classes.get(&class.name)?;
        if !declares_stream_wrapper_marker(class_info) {
            return None;
        }
        stream_wrapper_contract_return_type(&crate::names::php_symbol_key(&method.name))
    }

    /// Patches untyped constructor parameters with property types when the constructor
    /// property-promotion rule applies.
    ///
    /// For each constructor parameter without an explicit type hint, if the class has a
    /// matching promoted property (`constructor_param_to_prop`), that property's declared
    /// type is injected into `method_env` for the parameter and also propagated back into
    /// the class signature's `params[i].1`. Skips parameters that have explicit type
    /// annotations or whose promoted property is redeclared as a normal property.
    fn patch_constructor_method_env(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
        method_env: &mut TypeEnv,
    ) {
        if let Some(ci) = self.classes.get(&class.name).cloned() {
            for (i, (pname, type_ann, _, _)) in method.params.iter().enumerate() {
                if type_ann.is_some() {
                    continue;
                }
                if let Some(Some(prop_name)) = ci.constructor_param_to_prop.get(i) {
                    if ci.visible_property_is_declared(prop_name) {
                        continue;
                    }
                    if let Some((_, (_, ty))) = ci.visible_property(prop_name) {
                        method_env.insert(pname.clone(), ty.clone());
                        if let Some(ci_mut) = self.classes.get_mut(&class.name) {
                            if let Some(sig) = ci_mut.methods.get_mut("__construct") {
                                if i < sig.params.len() {
                                    sig.params[i].1 = ty.clone();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Re-types a method's BY-REFERENCE array parameters to what its body left them holding.
    ///
    /// A by-reference parameter is compiled against the CALLER's storage. When the body widens
    /// its element representation — a loop storage contract, an element write of a `Mixed` value
    /// — the caller's array is rewritten to boxed slots, and a caller still typed `array<int>`
    /// reads those boxes back as ADDRESSES: `$o->go($ints)` printed two.
    ///
    /// Only a widening is recorded, exactly as `resolve_function_signature` does for a free
    /// function. A method that merely sorts its parameter keeps the narrow element type the
    /// backend can sort.
    fn update_method_by_ref_param_types(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
        method_env: &TypeEnv,
    ) {
        let widened: Vec<(usize, PhpType)> = method
            .params
            .iter()
            .enumerate()
            .filter(|(_, (_, _, _, is_ref))| *is_ref)
            .filter_map(|(index, (name, _, _, _))| {
                let exit_ty = method_env.get(name)?;
                Some((index, exit_ty.clone()))
            })
            .collect();
        if widened.is_empty() {
            return;
        }
        let key = php_symbol_key(&method.name);
        let Some(class_info) = self.classes.get_mut(&class.name) else {
            return;
        };
        let sig = if method.is_static {
            class_info.static_methods.get_mut(&key)
        } else {
            class_info.methods.get_mut(&key)
        };
        let Some(sig) = sig else {
            return;
        };
        let mut recorded = Vec::new();
        for (index, exit_ty) in widened {
            let Some((_, param_ty)) = sig.params.get_mut(index) else {
                continue;
            };
            if crate::types::checker::array_element_representation_widens(param_ty, &exit_ty) {
                *param_ty = exit_ty;
                recorded.push(index);
            }
        }
        let owner = format!("{}::{}", class.name, key);
        for index in recorded {
            self.by_ref_widened_params.insert((owner.clone(), index));
        }
    }

    /// Infers the return type from method body `return` statements, validates it against
    /// any declared return type hint, and writes the effective return type back into
    /// `self.classes`.
    ///
    /// Return type inference scans `method.body` for `return` statements, widens all
    /// observed types to the common supertype, and falls back to `PhpType::Void` when
    /// the body is empty. If a declared hint exists, `require_declared_return_coverage`
    /// checks for unreachable returns and `require_compatible_return_type` checks each
    /// observed return for assignability to the declared type. A `Never` declared return
    /// suppresses the compatibility check (the body is allowed to have no returns when
    /// it always throws/exits/loops). `Never` combined with a body that *does* contain
    /// return statements produces a compile error. Generic array hints are passed
    /// through as-is to preserve inference.
    ///
    /// A method body containing `yield` is a generator: calling it produces a
    /// `Generator` object regardless of what the body's `return` statements say, so
    /// generator detection short-circuits the whole inference/validation chain the
    /// same way the free-function path in `functions::resolution::signature` does.
    /// Without that short-circuit an unhinted generator method infers `void` (the
    /// body has no value return) and a `: Generator` hint trips the
    /// "must return a value on every path" coverage check.
    fn update_method_return_type(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
        method_env: &TypeEnv,
        pass_errors: &mut Vec<CompileError>,
    ) {
        let mut return_infos = Vec::new();
        let mut callable_return_sigs = Vec::new();
        let mut callable_array_return_sigs = Vec::new();
        for stmt in &method.body {
            self.collect_return_infos(stmt, method_env, &mut return_infos);
            self.collect_return_callable_sigs(stmt, method_env, &mut callable_return_sigs);
            self.collect_return_callable_array_sigs(
                stmt,
                method_env,
                &mut callable_array_return_sigs,
            );
        }
        let raw_inferred = if return_infos.is_empty() {
            None
        } else {
            let mut widest = return_infos[0].ty.clone();
            for return_info in &return_infos[1..] {
                widest = Self::wider_type(&widest, &return_info.ty);
            }
            Some(widest)
        };
        let inferred_return = raw_inferred.clone().unwrap_or(PhpType::Void);
        let effective_return = if crate::types::checker::yield_validation::body_contains_yield(
            &method.body,
        ) {
            match self.generator_method_return_type(class, method) {
                Ok(generator_ty) => generator_ty,
                Err(error) => {
                    pass_errors.extend(error.flatten());
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                    return;
                }
            }
        } else if let Some(type_ann) = method.return_type.as_ref() {
            match self.resolve_declared_return_type_hint(
                type_ann,
                method.span,
                &format!("Method '{}::{}'", class.name, method.name),
            ) {
                Ok(declared) => {
                    if matches!(declared, PhpType::Never)
                        && Self::body_contains_return(&method.body)
                    {
                        pass_errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Method '{}::{}' declared never must not return",
                                class.name, method.name
                            ),
                        ));
                        self.current_class = None;
                        self.current_method = None;
                        self.current_method_is_static = false;
                        return;
                    }
                    if let Err(error) = self.require_declared_return_coverage(
                        &declared,
                        &method.body,
                        method.span,
                        &format!("Method '{}::{}'", class.name, method.name),
                    ) {
                        pass_errors.extend(error.flatten());
                        self.current_class = None;
                        self.current_method = None;
                        self.current_method_is_static = false;
                        return;
                    }
                    // :never methods are allowed to have no return statements (they always throw/exit/loop).
                    let skip_compat_check = matches!(declared, PhpType::Never);
                    if !skip_compat_check {
                        for return_info in &return_infos {
                            if let Err(error) = self.require_compatible_return_type(
                                &declared,
                                &return_info.ty,
                                return_info.has_value,
                                method.span,
                                &format!("Method '{}::{}' return type", class.name, method.name),
                            ) {
                                pass_errors.extend(error.flatten());
                                self.current_class = None;
                                self.current_method = None;
                                self.current_method_is_static = false;
                                return;
                            }
                        }
                    }
                    if Self::is_generic_array_hint(&declared)
                        && matches!(inferred_return, PhpType::Array(_) | PhpType::AssocArray { .. })
                    {
                        inferred_return
                    } else {
                        declared
                    }
                }
                Err(error) => {
                    pass_errors.extend(error.flatten());
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                    return;
                }
            }
        } else {
            self.stream_contract_return_type(class, method, &raw_inferred)
                .unwrap_or(inferred_return)
        };
        if !method.is_static {
            if let Some(ci) = self.classes.get_mut(&class.name) {
                if let Some(sig) = ci.methods.get_mut(&php_symbol_key(&method.name)) {
                    sig.return_type = effective_return.clone();
                }
            }
        } else if let Some(ci) = self.classes.get_mut(&class.name) {
            if let Some(sig) = ci.static_methods.get_mut(&php_symbol_key(&method.name)) {
                sig.return_type = effective_return.clone();
            }
        }
        self.update_method_callable_return_metadata(
            &class.name,
            &php_symbol_key(&method.name),
            &effective_return,
            &callable_return_sigs,
            &callable_array_return_sigs,
        );
    }

    /// Resolves the return type of a method whose body contains `yield`.
    ///
    /// The result is always `Generator`, because that is the object PHP hands back when
    /// the generator method is called. A declared return hint is still resolved and
    /// validated: hints that accept a `Generator` (`Generator`, `Traversable`,
    /// `iterable`, `mixed`, …) pass through, anything else is reported as an
    /// incompatible return type. Unlike the non-generator path there is no
    /// return-coverage check — a generator body legitimately has no `return` at all.
    fn generator_method_return_type(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
    ) -> Result<PhpType, CompileError> {
        let generator_ty = PhpType::Object("Generator".to_string());
        if let Some(type_ann) = method.return_type.as_ref() {
            let declared = self.resolve_declared_return_type_hint(
                type_ann,
                method.span,
                &format!("Method '{}::{}'", class.name, method.name),
            )?;
            if !self.generator_return_type_accepts(&declared) {
                self.require_compatible_return_type(
                    &declared,
                    &generator_ty,
                    true,
                    method.span,
                    &format!("Method '{}::{}' return type", class.name, method.name),
                )?;
            }
        }
        Ok(generator_ty)
    }

    /// Updates callable-return metadata for one checked method body.
    fn update_method_callable_return_metadata(
        &mut self,
        class_name: &str,
        method_key: &str,
        return_type: &PhpType,
        callable_return_sigs: &[FunctionSig],
        callable_array_return_sigs: &[FunctionSig],
    ) {
        let Some(class_info) = self.classes.get_mut(class_name) else {
            return;
        };
        if return_type == &PhpType::Callable {
            if let Some(callable_sig) = matching_callable_sig(callable_return_sigs) {
                class_info
                    .callable_method_return_sigs
                    .insert(method_key.to_string(), callable_sig);
            } else {
                class_info.callable_method_return_sigs.remove(method_key);
            }
        } else {
            class_info.callable_method_return_sigs.remove(method_key);
        }
        if is_callable_array_return_type(return_type) {
            if let Some(callable_sig) = matching_callable_sig(callable_array_return_sigs) {
                class_info
                    .callable_array_method_return_sigs
                    .insert(method_key.to_string(), callable_sig);
            } else {
                class_info
                    .callable_array_method_return_sigs
                    .remove(method_key);
            }
        } else {
            class_info
                .callable_array_method_return_sigs
                .remove(method_key);
        }
    }
}

/// Returns true when a method return type is a homogeneous array of callables.
fn is_callable_array_return_type(return_type: &PhpType) -> bool {
    match return_type {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}

/// Returns one callable signature only when every return path has the same contract.
fn matching_callable_sig(return_sigs: &[FunctionSig]) -> Option<FunctionSig> {
    let first = return_sigs.first()?.clone();
    if return_sigs.iter().all(|sig| sig == &first) {
        Some(callable_return_codegen_sig(first))
    } else {
        None
    }
}

/// Normalizes untyped mixed parameters in callable-return metadata for codegen.
fn callable_return_codegen_sig(mut sig: FunctionSig) -> FunctionSig {
    for (idx, (_, ty)) in sig.params.iter_mut().enumerate() {
        if !sig.declared_params.get(idx).copied().unwrap_or(false)
            && matches!(ty, PhpType::Mixed)
        {
            *ty = PhpType::Int;
        }
    }
    sig
}

/// Whether a class declares a method the `streamWrapper` protocol reserves.
///
/// Shares [`is_user_wrapper_marker_method`] with the EIR normalizer on purpose: the two gates
/// decide the SAME question — does the fixed raw-argument ABI apply to this class — and a class
/// only one of them accepts gets a body expecting a boxed Mixed fed a (ptr,len) pair.
///
/// [`is_user_wrapper_marker_method`]: crate::codegen_support::runtime::is_user_wrapper_marker_method
fn declares_stream_wrapper_marker(class_info: &crate::types::schema::ClassInfo) -> bool {
    class_info
        .methods
        .keys()
        .any(|key| crate::codegen_support::runtime::is_user_wrapper_marker_method(key))
}

/// Whether a class carries the userspace stream-FILTER contract.
///
/// A filter declares `filter()`; unlike the wrapper hooks the name is not reserved on its own, so
/// the class must also descend from `php_user_filter`, which is what `stream_filter_register()`
/// requires of it.
fn declares_stream_filter_marker(class_info: &crate::types::schema::ClassInfo) -> bool {
    class_info.methods.contains_key("filter")
        && class_info
            .parent
            .as_deref()
            .is_some_and(|parent| {
                crate::names::php_symbol_key(parent.trim_start_matches('\\')) == "php_user_filter"
            })
}

/// The parameter types PHP documents for each `streamWrapper` contract method.
///
/// Only methods that TAKE parameters appear; the rest need no seeding. `stream_open`'s fourth
/// parameter is by-reference `?string`, and is seeded as a string because that is what the body
/// may assign into it. Filter classes are deliberately absent: `filter()`'s first two parameters
/// are bucket brigades, which have no PHP type to seed.
fn stream_wrapper_contract_param_types(method_key: &str) -> Option<Vec<Option<PhpType>>> {
    let ints = |n: usize| vec![Some(PhpType::Int); n];
    let strs = |types: Vec<PhpType>| types.into_iter().map(Some).collect::<Vec<_>>();
    Some(match method_key {
        "stream_open" => strs(vec![PhpType::Str, PhpType::Str, PhpType::Int, PhpType::Str]),
        "stream_write" => strs(vec![PhpType::Str]),
        "stream_read" | "stream_truncate" | "stream_lock" | "stream_cast" => ints(1),
        "stream_seek" => ints(2),
        // `$arg2` is php's `mixed`: NULL for STREAM_OPTION_BLOCKING, an int for the buffer and
        // timeout options. MEASURED — `set_option(1, 0, NULL)` beside `set_option(4, 1, 500)`.
        // Only Mixed spans both, and a boxed Mixed travels in the same register an int does, so
        // the call shape is unchanged; `__rt_user_wrapper_set_option` does the boxing.
        "stream_set_option" => strs(vec![PhpType::Int, PhpType::Int, PhpType::Mixed]),
        "unlink" => strs(vec![PhpType::Str]),
        "rename" => strs(vec![PhpType::Str, PhpType::Str]),
        "url_stat" | "rmdir" | "dir_opendir" => strs(vec![PhpType::Str, PhpType::Int]),
        "mkdir" => strs(vec![PhpType::Str, PhpType::Int, PhpType::Int]),
        // A FILTER's `filter($in, $out, &$consumed, $closing)`: the first three have no PHP type
        // to pin — two bucket brigades and a by-reference counter whose ABI the dispatcher fixes
        // — but `$closing` is documented `bool`, and an untyped parameter otherwise infers Int.
        // The register value is the same 0/1 either way; the TYPE is what `var_dump($closing)`
        // renders and what `$closing === true` compares, so php printed `bool(false)` where
        // elephc printed `int(0)` and a strict comparison against `true` could never hold.
        "filter" => vec![None, None, None, Some(PhpType::Bool)],
        // `$value` carries whatever the option needs: an [mtime, atime] array for
        // STREAM_META_TOUCH, an int for ACCESS/OWNER/GROUP, a string for the
        // *_NAME options. Only Mixed spans all three, and the ABI keeps a boxed
        // Mixed in one register, so the pair-carrying `$path` stays aligned.
        "stream_metadata" => strs(vec![PhpType::Str, PhpType::Int, PhpType::Mixed]),
        _ => return None,
    })
}

/// The return type each `streamWrapper` contract method's FIXED runtime ABI is built around.
///
/// Only the slots the dispatcher reads out of the string-result pair appear. The scalar slots need
/// no entry: `bool`, `int` and a boxed `mixed` all travel in the same single result register, so an
/// inference that lands on the wrong one of those still reaches the dispatcher intact. The stat
/// slots are deliberately absent for the opposite reason — `stream_stat()`/`url_stat()` hand back a
/// boxed Mixed cell on purpose, which is why the vtable documents them as having to stay untyped.
fn stream_wrapper_contract_return_type(method_key: &str) -> Option<PhpType> {
    match method_key {
        "stream_read" | "dir_readdir" => Some(PhpType::Str),
        _ => None,
    }
}
