//! Purpose:
//! Type-checks callable closures behavior.
//! Infers callable signatures and validates invocation details that affect later lowering and optimizer effects.
//!
//! Called from:
//! - `crate::types::checker::callables`
//! - `crate::types::checker::inference`
//!
//! Key details:
//! - Closure captures, first-class callable syntax, and extern calls must agree with shared call argument planning.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt, TypeExpr};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::inference::syntactic::wider_type_syntactic;
use super::super::Checker;

pub(crate) struct ClosureSignatureContext {
    pub params: Vec<(String, PhpType)>,
    pub env: TypeEnv,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub declared_params: Vec<bool>,
}

impl Checker {
    pub(crate) fn prepare_closure_signature_context(
        &mut self,
        params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
        variadic: &Option<String>,
        captures: &[String],
        span: Span,
        env: &TypeEnv,
    ) -> Result<ClosureSignatureContext, CompileError> {
        for cap in captures {
            if !env.contains_key(cap) {
                return Err(CompileError::new(
                    span,
                    &format!("Undefined variable in use(): ${}", cap),
                ));
            }
        }

        let mut closure_env = env.clone();
        let mut param_types = Vec::new();
        let mut defaults = Vec::new();
        let mut ref_params = Vec::new();
        let mut declared_params = Vec::new();

        for (name, type_ann, default, is_ref) in params {
            let (env_ty, sig_ty) = match type_ann {
                Some(type_ann) => {
                    let declared_ty = self.resolve_declared_param_type_hint(
                        type_ann,
                        span,
                        &format!("Closure parameter ${}", name),
                    )?;
                    self.validate_declared_default_type(
                        &declared_ty,
                        default.as_ref(),
                        span,
                        &format!("Closure parameter ${}", name),
                    )?;
                    (declared_ty.clone(), declared_ty)
                }
                None => (PhpType::Int, PhpType::Mixed),
            };

            closure_env.insert(name.clone(), env_ty);
            param_types.push((name.clone(), sig_ty));
            defaults.push(default.clone());
            ref_params.push(*is_ref);
            declared_params.push(type_ann.is_some());
        }

        if let Some(name) = variadic {
            closure_env.insert(name.clone(), PhpType::Array(Box::new(PhpType::Int)));
            param_types.push((name.clone(), PhpType::Array(Box::new(PhpType::Mixed))));
            defaults.push(None);
            ref_params.push(false);
            declared_params.push(false);
        }

        Ok(ClosureSignatureContext {
            params: param_types,
            env: closure_env,
            defaults,
            ref_params,
            declared_params,
        })
    }

    pub(crate) fn resolve_closure_return_type(
        &mut self,
        body: &[Stmt],
        return_type: &Option<TypeExpr>,
        span: Span,
        env: &TypeEnv,
    ) -> Result<(PhpType, bool), CompileError> {
        if super::super::yield_validation::body_contains_yield(body) {
            let generator_ty = PhpType::Object("Generator".to_string());
            if let Some(type_ann) = return_type {
                let declared_ret =
                    self.resolve_declared_return_type_hint(type_ann, span, "Closure")?;
                self.require_compatible_return_type(
                    &declared_ret,
                    &generator_ty,
                    true,
                    span,
                    "Closure return type",
                )?;
                return Ok((generator_ty, true));
            }
            return Ok((generator_ty, false));
        }

        let mut all_return_infos = Vec::new();
        for stmt in body {
            self.collect_return_infos(stmt, env, &mut all_return_infos);
        }

        if let Some(type_ann) = return_type {
            let declared_ret =
                self.resolve_declared_return_type_hint(type_ann, span, "Closure")?;
            if matches!(declared_ret, PhpType::Never) && Self::body_contains_return(body) {
                return Err(CompileError::new(
                    span,
                    "Closure declared never must not return",
                ));
            }
            self.require_declared_return_coverage(&declared_ret, body, span, "Closure")?;
            if all_return_infos.is_empty() {
                return Ok((declared_ret, true));
            }

            for return_info in &all_return_infos {
                self.require_compatible_return_type(
                    &declared_ret,
                    &return_info.ty,
                    return_info.has_value,
                    span,
                    "Closure return type",
                )?;
            }

            let mut inferred_return = all_return_infos[0].ty.clone();
            for return_info in &all_return_infos[1..] {
                inferred_return = wider_type_syntactic(&inferred_return, &return_info.ty);
            }

            Ok((
                Self::specialize_generic_array_hint(&declared_ret, &inferred_return),
                true,
            ))
        } else if all_return_infos.is_empty() {
            Ok((PhpType::Int, false))
        } else {
            let mut inferred_return = all_return_infos[0].ty.clone();
            for return_info in &all_return_infos[1..] {
                inferred_return = wider_type_syntactic(&inferred_return, &return_info.ty);
            }
            Ok((inferred_return, false))
        }
    }

    pub(crate) fn resolve_expr_callable_sig(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<FunctionSig>, CompileError> {
        match &expr.kind {
            ExprKind::Closure {
                params,
                variadic,
                return_type,
                body,
                captures,
                capture_refs: _,
                ..
            } => {
                let closure_sig = self.prepare_closure_signature_context(
                    params,
                    variadic,
                    captures,
                    expr.span,
                    env,
                )?;
                let (return_type, declared_return) = self.resolve_closure_return_type(
                    body,
                    return_type,
                    expr.span,
                    &closure_sig.env,
                )?;
                Ok(Some(FunctionSig {
                    params: closure_sig.params,
                    defaults: closure_sig.defaults,
                    return_type,
                    declared_return,
                    ref_params: closure_sig.ref_params,
                    declared_params: closure_sig.declared_params,
                    variadic: variadic.clone(),
                    deprecation: None,
                }))
            }
            ExprKind::FirstClassCallable(target) => self
                .resolve_first_class_callable_sig(target, expr.span, env)
                .map(Some),
            ExprKind::Variable(var_name) => Ok(self.callable_sigs.get(var_name).cloned()),
            ExprKind::Assignment { value, .. } => self.resolve_expr_callable_sig(value, env),
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => self.resolve_matching_branch_callable_sig(then_expr, else_expr, env),
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => {
                self.resolve_matching_branch_callable_sig(value, default, env)
            }
            _ => Ok(None),
        }
    }

    fn resolve_matching_branch_callable_sig(
        &mut self,
        left: &Expr,
        right: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<FunctionSig>, CompileError> {
        let Some(left_sig) = self.resolve_expr_callable_sig(left, env)? else {
            return Ok(None);
        };
        let Some(right_sig) = self.resolve_expr_callable_sig(right, env)? else {
            return Ok(None);
        };
        if left_sig == right_sig {
            Ok(Some(left_sig))
        } else {
            Ok(None)
        }
    }
}
