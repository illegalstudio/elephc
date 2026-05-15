//! Purpose:
//! Implements the checker driver functions phase.
//! Owns one ordered step in building checker state and validating the program before optimization/codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Phase order controls diagnostics, available declarations, required libraries, and function-local environments.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, Program, StmtKind, TypeExpr};
use crate::types::FunctionSig;

use super::super::{Checker, FnDecl};

impl Checker {
    pub(super) fn collect_function_decls(
        &mut self,
        program: &Program,
        errors: &mut Vec<CompileError>,
    ) {
        let mut seen_functions = HashSet::new();
        for stmt in program {
            if let StmtKind::FunctionVariantGroup { name, variants } = &stmt.kind {
                if !seen_functions.insert(php_symbol_key(name)) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate function declaration: {}", name),
                    ));
                    continue;
                }
                if let Some(builtin) =
                    crate::types::checker::builtins::canonical_builtin_function_name(name)
                {
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
                params,
                variadic,
                return_type,
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
                if let Some(builtin) =
                    crate::types::checker::builtins::canonical_builtin_function_name(name)
                {
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
                let defaults: Vec<Option<Expr>> =
                    params.iter().map(|(_, _, d, _)| d.clone()).collect();
                let ref_flags: Vec<bool> = params.iter().map(|(_, _, _, r)| *r).collect();
                self.fn_decls.insert(
                    name.clone(),
                    FnDecl {
                        params: param_names,
                        param_types: param_type_anns,
                        defaults,
                        ref_params: ref_flags,
                        variadic: variadic.clone(),
                        return_type: return_type.clone(),
                        span: stmt.span,
                        body: body.clone(),
                        attributes: stmt.attributes.clone(),
                    },
                );
            }
        }
    }

    pub(super) fn has_function_decl_folded(&self, name: &str) -> bool {
        let key = php_symbol_key(name);
        self.fn_decls
            .keys()
            .any(|existing| php_symbol_key(existing) == key)
            || self
                .function_variant_groups
                .keys()
                .any(|existing| php_symbol_key(existing) == key)
            || self
                .extern_functions
                .keys()
                .any(|existing| php_symbol_key(existing) == key)
    }

    pub(crate) fn canonical_function_name_folded(&self, name: &str) -> Option<String> {
        folded_map_key(&self.functions, name)
            .or_else(|| folded_map_key(&self.function_variant_groups, name))
            .or_else(|| folded_map_key(&self.fn_decls, name))
    }

    pub(crate) fn canonical_extern_function_name_folded(&self, name: &str) -> Option<String> {
        folded_map_key(&self.extern_functions, name)
    }

    pub(super) fn resolve_unchecked_functions(&mut self, errors: &mut Vec<CompileError>) {
        let unchecked: Vec<String> = self
            .fn_decls
            .keys()
            .filter(|name| !self.functions.contains_key(*name))
            .cloned()
            .collect();
        for name in unchecked {
            if let Some(decl) = self.fn_decls.get(&name).cloned() {
                match self.initial_function_param_types(&name, &decl) {
                    Ok(param_types) => {
                        if let Err(error) =
                            self.resolve_function_signature(&name, &decl, param_types)
                        {
                            errors.extend(error.flatten());
                        }
                    }
                    Err(error) => errors.extend(error.flatten()),
                }
            }
        }
        self.resolve_function_variant_groups(errors);
    }

    fn resolve_function_variant_groups(&mut self, errors: &mut Vec<CompileError>) {
        let names: Vec<String> = self.function_variant_groups.keys().cloned().collect();
        for name in names {
            if self.functions.contains_key(&name) {
                continue;
            }
            if let Err(error) =
                self.ensure_function_variant_group_signature(&name, crate::span::Span::dummy())
            {
                errors.extend(error.flatten());
            }
        }
    }

    pub(crate) fn ensure_function_variant_group_signature(
        &mut self,
        name: &str,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if self.functions.contains_key(name) {
            return Ok(());
        }
        let variants = self
            .function_variant_groups
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;
        let first_variant = variants
            .first()
            .ok_or_else(|| CompileError::new(span, &format!("Function '{}' has no variants", name)))?
            .clone();

        if let Some(provisional) = self.provisional_variant_group_sig(&first_variant)? {
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
            let param_types = self.initial_function_param_types(variant, &decl)?;
            self.resolve_function_signature(variant, &decl, param_types)?;
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
        let first = sigs
            .next()
            .transpose()?
            .ok_or_else(|| CompileError::new(span, &format!("Function '{}' has no variants", name)))?;
        for sig in sigs {
            let sig = sig?;
            if sig != first {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function variants for '{}' must have identical signatures",
                        name
                    ),
                ));
            }
        }
        self.functions.insert(name.to_string(), first);
        Ok(())
    }

    fn provisional_variant_group_sig(
        &mut self,
        first_variant: &str,
    ) -> Result<Option<FunctionSig>, CompileError> {
        let Some(decl) = self.fn_decls.get(first_variant).cloned() else {
            return Ok(None);
        };
        let param_types = self.initial_function_param_types(first_variant, &decl)?;
        Ok(Some(FunctionSig {
            params: param_types,
            defaults: decl.defaults,
            return_type: crate::types::PhpType::Int,
            declared_return: decl.return_type.is_some(),
            ref_params: decl.ref_params,
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| false))
                .collect(),
            variadic: decl.variadic,
            deprecation: None,
        }))
    }
}

fn folded_map_key<T>(map: &HashMap<String, T>, name: &str) -> Option<String> {
    let key = php_symbol_key(name);
    map.keys()
        .find(|existing| php_symbol_key(existing) == key)
        .cloned()
}
