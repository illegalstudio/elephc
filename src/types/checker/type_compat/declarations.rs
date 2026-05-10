//! Purpose:
//! Checks type compatibility for declarations cases.
//! Supports the central assignability predicate used by declarations, calls, returns, and assignments.
//!
//! Called from:
//! - `crate::types::checker::type_compat`
//!
//! Key details:
//! - Rules here define accepted programs, so PHP covariance, inheritance, and extension-specific constraints must stay explicit.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, TypeExpr};
use crate::types::{callable_wrapper_sig, ClassInfo, FunctionSig, PhpType};

use super::super::inference::syntactic::infer_expr_type_syntactic;
use super::super::{Checker, FnDecl};

impl Checker {
    pub(crate) fn callable_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
        callable_wrapper_sig(sig)
    }

    pub(crate) fn resolve_declared_param_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        context: &str,
    ) -> Result<PhpType, CompileError> {
        let ty = self.resolve_type_expr(type_expr, span)?;
        match ty {
            PhpType::Void => Err(CompileError::new(
                span,
                &format!("{} cannot use type void", context),
            )),
            _ if Self::type_contains_never(&ty) => Err(CompileError::new(
                span,
                &format!("{} cannot use type never", context),
            )),
            _ => Ok(ty),
        }
    }

    pub(crate) fn resolve_declared_return_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        _context: &str,
    ) -> Result<PhpType, CompileError> {
        if !matches!(type_expr, TypeExpr::Never) && Self::type_expr_contains_never(type_expr) {
            return Err(CompileError::new(
                span,
                "never can only be used as a standalone return type",
            ));
        }
        self.resolve_type_expr(type_expr, span)
    }

    pub(crate) fn resolve_declared_local_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        context: &str,
    ) -> Result<PhpType, CompileError> {
        let ty = self.resolve_type_expr(type_expr, span)?;
        if Self::type_contains_never(&ty) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use type never", context),
            ));
        }
        Ok(ty)
    }

    pub(crate) fn resolve_declared_property_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        context: &str,
    ) -> Result<PhpType, CompileError> {
        let ty = self.resolve_type_expr(type_expr, span)?;
        if matches!(ty, PhpType::Void) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use type void", context),
            ));
        }
        if Self::type_contains_never(&ty) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use type never", context),
            ));
        }
        if Self::type_contains_callable(&ty) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use type callable", context),
            ));
        }
        Ok(ty)
    }

    fn type_contains_callable(ty: &PhpType) -> bool {
        match ty {
            PhpType::Callable => true,
            PhpType::Union(members) => members.iter().any(Self::type_contains_callable),
            PhpType::Array(inner) | PhpType::Buffer(inner) => Self::type_contains_callable(inner),
            PhpType::AssocArray { key, value } => {
                Self::type_contains_callable(key) || Self::type_contains_callable(value)
            }
            _ => false,
        }
    }

    fn type_contains_never(ty: &PhpType) -> bool {
        match ty {
            PhpType::Never => true,
            PhpType::Union(members) => members.iter().any(Self::type_contains_never),
            PhpType::Array(inner) | PhpType::Buffer(inner) => Self::type_contains_never(inner),
            PhpType::AssocArray { key, value } => {
                Self::type_contains_never(key) || Self::type_contains_never(value)
            }
            _ => false,
        }
    }

    fn type_expr_contains_never(type_expr: &TypeExpr) -> bool {
        match type_expr {
            TypeExpr::Never => true,
            TypeExpr::Nullable(inner) | TypeExpr::Buffer(inner) => {
                Self::type_expr_contains_never(inner)
            }
            TypeExpr::Union(members) => members.iter().any(Self::type_expr_contains_never),
            _ => false,
        }
    }

    pub(crate) fn require_boxed_by_ref_storage(
        &self,
        expected_ty: &PhpType,
        actual_ty: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if matches!(expected_ty.codegen_repr(), PhpType::Mixed)
            && !matches!(actual_ty.codegen_repr(), PhpType::Mixed)
        {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} requires a variable with mixed/union/nullable storage when passed by reference",
                    context
                ),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_declared_default_type(
        &self,
        expected_ty: &PhpType,
        default_expr: Option<&Expr>,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if let Some(default_expr) = default_expr {
            let default_ty = infer_expr_type_syntactic(default_expr);
            self.require_compatible_arg_type(expected_ty, &default_ty, span, context)?;
        }
        Ok(())
    }

    pub(crate) fn initial_function_param_types(
        &self,
        name: &str,
        decl: &FnDecl,
    ) -> Result<Vec<(String, PhpType)>, CompileError> {
        let mut param_types = Vec::new();
        for (idx, param_name) in decl.params.iter().enumerate() {
            if let Some(type_ann) = decl.param_types.get(idx).and_then(|t| t.as_ref()) {
                let declared_ty = self.resolve_declared_param_type_hint(
                    type_ann,
                    decl.span,
                    &format!("Function '{}' parameter ${}", name, param_name),
                )?;
                self.validate_declared_default_type(
                    &declared_ty,
                    decl.defaults.get(idx).and_then(|d| d.as_ref()),
                    decl.span,
                    &format!("Function '{}' parameter ${}", name, param_name),
                )?;
                param_types.push((param_name.clone(), declared_ty));
            } else if let Some(default_expr) = decl.defaults.get(idx).and_then(|d| d.as_ref()) {
                param_types.push((param_name.clone(), infer_expr_type_syntactic(default_expr)));
            } else {
                param_types.push((param_name.clone(), PhpType::Int));
            }
        }
        if let Some(variadic_name) = decl.variadic.as_ref() {
            param_types.push((
                variadic_name.clone(),
                PhpType::Array(Box::new(PhpType::Int)),
            ));
        }
        Ok(param_types)
    }

    pub(crate) fn declared_method_param_flags(
        class_info: &ClassInfo,
        method_name: &str,
        is_static: bool,
    ) -> Vec<bool> {
        let method_key = crate::names::php_symbol_key(method_name);
        class_info
            .method_decls
            .iter()
            .find(|method| {
                crate::names::php_symbol_key(&method.name) == method_key
                    && method.is_static == is_static
            })
            .map(|method| {
                method
                    .params
                    .iter()
                    .map(|(_, type_ann, _, _)| type_ann.is_some())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn callable_sig_for_declared_params(
        sig: &FunctionSig,
        declared_flags: &[bool],
    ) -> FunctionSig {
        let mut effective_sig = sig.clone();
        for (idx, (_, ty)) in effective_sig.params.iter_mut().enumerate() {
            if !declared_flags.get(idx).copied().unwrap_or(false) {
                *ty = PhpType::Mixed;
            }
        }
        effective_sig.declared_params = declared_flags.to_vec();
        effective_sig
    }

    pub(crate) fn with_local_storage_context<T, F>(
        &mut self,
        ref_param_names: Vec<String>,
        f: F,
    ) -> Result<T, CompileError>
    where
        F: FnOnce(&mut Self) -> Result<T, CompileError>,
    {
        let saved_ref_params = self.active_ref_params.clone();
        let saved_globals = self.active_globals.clone();
        let saved_statics = self.active_statics.clone();
        let saved_break_continue_depth = self.break_continue_depth;
        let saved_finally_break_continue_bases = self.finally_break_continue_bases.clone();

        self.active_ref_params = ref_param_names.into_iter().collect();
        self.active_globals.clear();
        self.active_statics.clear();
        self.break_continue_depth = 0;
        self.finally_break_continue_bases.clear();

        let result = f(self);

        self.active_ref_params = saved_ref_params;
        self.active_globals = saved_globals;
        self.active_statics = saved_statics;
        self.break_continue_depth = saved_break_continue_depth;
        self.finally_break_continue_bases = saved_finally_break_continue_bases;

        result
    }
}
