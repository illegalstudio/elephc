use crate::errors::CompileError;
use crate::parser::ast::{Expr, TypeExpr};
use crate::types::{ClassInfo, FunctionSig, PhpType};

use super::super::inference::syntactic::infer_expr_type_syntactic;
use super::super::{Checker, FnDecl};

impl Checker {
    pub(crate) fn callable_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
        let Some(variadic_name) = sig.variadic.as_ref() else {
            return sig.clone();
        };
        if sig
            .params
            .last()
            .is_some_and(|(name, ty)| name == variadic_name && matches!(ty, PhpType::Array(_)))
        {
            return sig.clone();
        }

        let mut wrapper_sig = sig.clone();
        wrapper_sig.params.push((
            variadic_name.clone(),
            PhpType::Array(Box::new(PhpType::Mixed)),
        ));
        wrapper_sig.defaults.push(None);
        wrapper_sig.ref_params.push(false);
        wrapper_sig.declared_params.push(false);
        wrapper_sig
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
            _ => Ok(ty),
        }
    }

    pub(crate) fn resolve_declared_return_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        _context: &str,
    ) -> Result<PhpType, CompileError> {
        self.resolve_type_expr(type_expr, span)
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
        class_info
            .method_decls
            .iter()
            .find(|method| method.name == method_name && method.is_static == is_static)
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

        self.active_ref_params = ref_param_names.into_iter().collect();
        self.active_globals.clear();
        self.active_statics.clear();

        let result = f(self);

        self.active_ref_params = saved_ref_params;
        self.active_globals = saved_globals;
        self.active_statics = saved_statics;

        result
    }
}
