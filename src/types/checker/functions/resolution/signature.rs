//! Purpose:
//! Handles function resolution signature details for call checking.
//! Materializes signatures or specialized metadata used by argument validation and return inference.
//!
//! Called from:
//! - `crate::types::checker::functions::resolution`
//!
//! Key details:
//! - Specialized and builtin signatures must expose the caller-visible parameter contract expected by call-argument planning.

use std::collections::HashMap;

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
    pub(crate) fn resolve_function_signature(
        &mut self,
        name: &str,
        decl: &FnDecl,
        param_types: Vec<(String, PhpType)>,
    ) -> Result<PhpType, CompileError> {
        let mut local_env: TypeEnv = HashMap::new();
        for (pname, pty) in &param_types {
            local_env.insert(pname.clone(), pty.clone());
        }
        // Seed the request superglobals so a function body can read/write
        // `$_SERVER`/`$_GET`/`$_POST` without a `global` declaration. `or_insert`
        // never clobbers a parameter that happens to share a superglobal name.
        for name in crate::superglobals::SUPERGLOBALS {
            local_env
                .entry((*name).to_string())
                .or_insert_with(crate::superglobals::superglobal_type);
        }
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
        for pname in &callable_param_names {
            if let Some(sig) = self
                .callable_param_sigs
                .get(&(function_key.clone(), pname.clone()))
                .cloned()
            {
                self.closure_return_types
                    .insert(pname.clone(), sig.return_type.clone());
                self.callable_sigs.insert(pname.clone(), sig);
            } else {
                self.closure_return_types.remove(pname);
                self.callable_sigs.remove(pname);
            }
        }

        let provisional_sig = FunctionSig {
            params: param_types.clone(),
            defaults: decl.defaults.clone(),
            return_type: PhpType::Int,
            declared_return: decl.return_type.is_some(),
            by_ref_return: decl.by_ref_return,
            ref_params: decl.ref_params.clone(),
            deprecation: None,
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| false))
                .collect(),
            variadic: decl.variadic.clone(),
        };
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
        let prev_by_ref_return = self.current_by_ref_return;
        self.current_by_ref_return = decl.by_ref_return;
        let body_check_result = self.with_local_storage_context(ref_param_names, |checker| {
            for stmt in &decl.body {
                if let Err(error) = checker.check_stmt(stmt, &mut local_env) {
                    errors.extend(error.flatten());
                }
                checker.collect_return_infos(stmt, &local_env, &mut all_return_infos);
                checker.collect_return_callable_sigs(stmt, &local_env, &mut callable_return_sigs);
                checker.collect_return_callable_array_sigs(
                    stmt,
                    &local_env,
                    &mut callable_array_return_sigs,
                );
            }
            Ok(())
        });
        self.current_by_ref_return = prev_by_ref_return;
        self.callable_param_names = saved_callable_param_names;
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
                    self.require_compatible_return_type(
                        &declared_ret,
                        &generator_ty,
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
            self.require_declared_return_coverage(
                &declared_ret,
                &decl.body,
                decl.span,
                &format!("Function '{}'", name),
            )?;
            if !all_return_infos.is_empty() {
                for return_info in &all_return_infos {
                    self.require_compatible_return_type(
                        &declared_ret,
                        &return_info.ty,
                        return_info.has_value,
                        decl.span,
                        &format!("Function '{}' return type", name),
                    )?;
                }
            }
            return_type = if Self::is_generic_array_hint(&declared_ret)
                && matches!(inferred_specific_array_type_from_infos(&all_return_infos), Some(_))
            {
                inferred_specific_array_type_from_infos(&all_return_infos).unwrap()
            } else {
                declared_ret
            };
        } else if !all_return_infos.is_empty() {
            return_type = all_return_infos[0].ty.clone();
            for return_info in &all_return_infos[1..] {
                return_type = Self::wider_type(&return_type, &return_info.ty);
            }
        }

        let sig = FunctionSig {
            params: param_types,
            defaults: decl.defaults.clone(),
            return_type: return_type.clone(),
            declared_return: decl.return_type.is_some(),
            by_ref_return: decl.by_ref_return,
            ref_params: decl.ref_params.clone(),
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| false))
                .collect(),
            variadic: decl.variadic.clone(),
            deprecation: crate::types::checker::schema::validation::extract_deprecation(
                &decl.attributes,
            ),
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

        Ok(return_type)
    }

    /// Returns true when a declared generator return annotation accepts
    /// the actual `Generator` object returned when the body contains `yield`.
    fn generator_return_type_accepts(&self, declared_ret: &PhpType) -> bool {
        if matches!(declared_ret, PhpType::Object(name) if name == "Traversable") {
            return true;
        }
        self.type_accepts(declared_ret, &PhpType::Object("Generator".to_string()))
    }
}

/// Infers a concrete array type from return info when the declared return type is a generic `array` hint.
///
/// Returns `Some(PhpType)` only when every non-void return in `return_types` is the same
/// array type (including `array<T>` or `assocArray` shapes). Returns `None` if returns differ,
/// include non-array types, or are all `void`.
fn inferred_specific_array_type_from_infos(
    return_types: &[super::super::returns::ReturnInfo],
) -> Option<PhpType> {
    let mut specific: Option<PhpType> = None;
    for return_info in return_types {
        let return_ty = &return_info.ty;
        if matches!(return_ty, PhpType::Void) {
            continue;
        }
        if !matches!(return_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            return None;
        }
        match &specific {
            None => specific = Some(return_ty.clone()),
            Some(existing) if existing == return_ty => {}
            _ => return None,
        }
    }
    specific
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
