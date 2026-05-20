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
        let function_key = name.to_string();
        let callable_param_names: Vec<String> = param_types
            .iter()
            .filter(|(_, pty)| pty == &PhpType::Callable)
            .map(|(pname, _)| pname.clone())
            .collect();
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
        let mut errors = Vec::new();
        let ref_param_names: Vec<String> = decl
            .params
            .iter()
            .zip(decl.ref_params.iter())
            .filter(|(_, is_ref)| **is_ref)
            .map(|(name, _)| name.clone())
            .collect();
        self.with_local_storage_context(ref_param_names, |checker| {
            for stmt in &decl.body {
                if let Err(error) = checker.check_stmt(stmt, &mut local_env) {
                    errors.extend(error.flatten());
                }
                checker.collect_return_infos(stmt, &local_env, &mut all_return_infos);
            }
            Ok(())
        })?;
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

        if let Some(type_ann) = decl.return_type.as_ref() {
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

        // Generator override: any function whose body contains `yield` is
        // implicitly a generator and returns a `Generator` object regardless
        // of its declared/inferred return type. PHP requires the declared
        // type to be `Generator`, `Iterator`, `Traversable`, or `iterable` —
        // we accept any of those plus the absence of an explicit annotation.
        if super::super::super::yield_validation::body_contains_yield(&decl.body) {
            return_type = PhpType::Object("Generator".to_string());
        }

        let sig = FunctionSig {
            params: param_types,
            defaults: decl.defaults.clone(),
            return_type: return_type.clone(),
            declared_return: decl.return_type.is_some(),
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

        Ok(return_type)
    }
}

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
