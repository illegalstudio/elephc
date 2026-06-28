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
        global_env: &TypeEnv,
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
                    let mut method_env: TypeEnv = global_env.clone();
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
                                    .map(|t| Self::specialize_generic_array_hint(&declared, &t))
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
                        method_env.insert(pname.clone(), ty);
                    }
                    if let Some(variadic_name) = &method.variadic {
                        let ty = sig_params
                            .as_ref()
                            .and_then(|p| p.get(method.params.len()))
                            .map(|(_, t)| t.clone())
                            .unwrap_or(PhpType::Array(Box::new(PhpType::Int)));
                        method_env.insert(variadic_name.clone(), ty);
                    }
                    if method_key == "__construct" {
                        self.patch_constructor_method_env(class, method, &mut method_env);
                    }

                    self.current_class = Some(class.name.clone());
                    self.current_method = Some(method_key.clone());
                    self.current_method_is_static = method.is_static;
                    self.current_by_ref_return = method.by_ref_return;
                    let method_ref_params: Vec<String> = method
                        .params
                        .iter()
                        .filter(|(_, _, _, is_ref)| *is_ref)
                        .map(|(name, _, _, _)| name.clone())
                        .collect();
                    let mut method_errors = Vec::new();
                    self.with_local_storage_context(method_ref_params, |checker| {
                        for s in &method.body {
                            if let Err(error) = checker.check_stmt(s, &mut method_env) {
                                method_errors.extend(error.flatten());
                            }
                        }
                        Ok(())
                    })?;
                    let method_has_errors = !method_errors.is_empty();
                    pass_errors.extend(method_errors);

                    if !method_has_errors {
                        self.update_method_return_type(class, method, &method_env, &mut pass_errors);
                    }
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                    self.current_by_ref_return = false;
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
                    if ci.declared_properties.contains(prop_name) {
                        continue;
                    }
                    if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == prop_name) {
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
        let effective_return = if let Some(type_ann) = method.return_type.as_ref() {
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
            inferred_return
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
