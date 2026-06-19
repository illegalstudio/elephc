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
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Stmt, TypeExpr};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::inference::syntactic::wider_type_syntactic;
use super::super::Checker;

/// Holds the resolved signature components for a closure: parameters with types, captured
/// environment bindings, default value expressions, by-reference flags, and declared-param flags.
pub(crate) struct ClosureSignatureContext {
    pub params: Vec<(String, PhpType)>,
    pub env: TypeEnv,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub declared_params: Vec<bool>,
}

impl Checker {
    /// Validates captured variables exist in the current environment, then builds a
    /// `ClosureSignatureContext` by resolving parameter type annotations, default value
    /// compatibility, and inserting parameter bindings into a cloned environment that
    /// includes variadic and capture bindings. Returns the context for use in closure
    /// return type inference.
    pub(crate) fn prepare_closure_signature_context(
        &mut self,
        params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
        variadic: &Option<String>,
        captures: &[String],
        span: Span,
        env: &TypeEnv,
    ) -> Result<ClosureSignatureContext, CompileError> {
        self.prepare_closure_signature_context_with_param_hints(
            params, variadic, captures, span, env, &[],
        )
    }

    /// Builds the closure signature/environment, typing unannotated parameters
    /// from `contextual_param_types` when a hint is available at that position.
    ///
    /// Callback builtins that know the argument types their comparator/visitor
    /// receives (for example `usort`/`uasort` over an array of objects) pass the
    /// element type as a hint so an unannotated parameter is checked against the
    /// real value type instead of the default `Int`/`Mixed` placeholder. An
    /// explicitly annotated parameter always keeps its declared type; the hint is
    /// only consulted for parameters with no type annotation.
    pub(crate) fn prepare_closure_signature_context_with_param_hints(
        &mut self,
        params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
        variadic: &Option<String>,
        captures: &[String],
        span: Span,
        env: &TypeEnv,
        contextual_param_types: &[PhpType],
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

        for (idx, (name, type_ann, default, is_ref)) in params.iter().enumerate() {
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
                None => match contextual_param_types.get(idx) {
                    Some(hint) => (hint.clone(), hint.clone()),
                    None => (PhpType::Int, PhpType::Mixed),
                },
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

    /// Infers the return type of a closure body by collecting all return statements, then
    /// comparing against an optional declared return type annotation. Validates coverage,
    /// compatibility, never-return constraints, and Generator yield semantics. Returns
    /// the inferred (or declared) return type and a boolean indicating whether a declared
    /// return type was present.
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

    /// Extracts the callable signature from an expression that may be a closure literal,
    /// first-class callable, variable referencing a callable, array-access callable,
    /// assignment-wrapped callable, or ternary/merge chains. Returns `None` for expressions
    /// that cannot be invoked as a callable at the type-check level.
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
            ExprKind::FunctionCall { name, .. } => {
                let resolved_name = self
                    .canonical_function_name_folded(name.as_str())
                    .unwrap_or_else(|| name.as_str().to_string());
                Ok(self.callable_return_sigs.get(&resolved_name).cloned())
            }
            ExprKind::MethodCall { object, method, .. } => {
                self.resolve_method_return_callable_sig(object, method, env, false)
            }
            ExprKind::StaticMethodCall {
                receiver, method, ..
            } => self.resolve_static_method_return_callable_sig(receiver, method, false),
            ExprKind::Variable(var_name) => Ok(self.callable_sigs.get(var_name).cloned()),
            ExprKind::ArrayAccess { array, .. } => {
                if let ExprKind::Variable(array_name) = &array.kind {
                    Ok(self.callable_sigs.get(array_name).cloned())
                } else {
                    Ok(None)
                }
            }
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

    /// Extracts the element callable signature from an expression that yields an array of callables.
    ///
    /// The checker stores homogeneous callable-array metadata under the array variable name.
    /// Function calls use `callable_array_return_sigs` so callers can recover the element
    /// signature without treating the function return itself as a callable.
    pub(crate) fn resolve_expr_callable_array_sig(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<FunctionSig>, CompileError> {
        match &expr.kind {
            ExprKind::ArrayLiteral(elems) => {
                self.resolve_matching_callable_array_element_sig(elems.iter(), env)
            }
            ExprKind::ArrayLiteralAssoc(entries) => {
                self.resolve_matching_callable_array_element_sig(
                    entries.iter().map(|(_, value)| value),
                    env,
                )
            }
            ExprKind::FunctionCall { name, .. } => {
                let resolved_name = self
                    .canonical_function_name_folded(name.as_str())
                    .unwrap_or_else(|| name.as_str().to_string());
                Ok(self.callable_array_return_sigs.get(&resolved_name).cloned())
            }
            ExprKind::MethodCall { object, method, .. } => {
                self.resolve_method_return_callable_sig(object, method, env, true)
            }
            ExprKind::StaticMethodCall {
                receiver, method, ..
            } => self.resolve_static_method_return_callable_sig(receiver, method, true),
            ExprKind::Variable(var_name) => Ok(self.callable_sigs.get(var_name).cloned()),
            ExprKind::Assignment { value, .. } => self.resolve_expr_callable_array_sig(value, env),
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => self.resolve_matching_branch_callable_array_sig(then_expr, else_expr, env),
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => {
                self.resolve_matching_branch_callable_array_sig(value, default, env)
            }
            _ => Ok(None),
        }
    }

    /// Resolves one shared callable signature for all values in a callable array expression.
    fn resolve_matching_callable_array_element_sig<'a>(
        &mut self,
        values: impl Iterator<Item = &'a Expr>,
        env: &TypeEnv,
    ) -> Result<Option<FunctionSig>, CompileError> {
        let mut shared_sig: Option<FunctionSig> = None;
        let mut saw_value = false;
        for value in values {
            saw_value = true;
            let Some(sig) = self.resolve_expr_callable_sig(value, env)? else {
                return Ok(None);
            };
            match &shared_sig {
                Some(existing) if existing != &sig => return Ok(None),
                Some(_) => {}
                None => shared_sig = Some(sig),
            }
        }
        if saw_value {
            Ok(shared_sig)
        } else {
            Ok(None)
        }
    }

    /// Resolves a callable-array signature only when both branches share one element contract.
    fn resolve_matching_branch_callable_array_sig(
        &mut self,
        left: &Expr,
        right: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<FunctionSig>, CompileError> {
        let Some(left_sig) = self.resolve_expr_callable_array_sig(left, env)? else {
            return Ok(None);
        };
        let Some(right_sig) = self.resolve_expr_callable_array_sig(right, env)? else {
            return Ok(None);
        };
        if left_sig == right_sig {
            Ok(Some(left_sig))
        } else {
            Ok(None)
        }
    }

    /// Resolves callable-return metadata for an instance method call expression.
    fn resolve_method_return_callable_sig(
        &mut self,
        object: &Expr,
        method: &str,
        env: &TypeEnv,
        array_return: bool,
    ) -> Result<Option<FunctionSig>, CompileError> {
        let object_ty = self.infer_type(object, env)?;
        let Some(class_name) = self.invokable_class_for_type(&object_ty) else {
            return Ok(None);
        };
        let method_key = php_symbol_key(method);
        let impl_class = self
            .classes
            .get(&class_name)
            .and_then(|class_info| class_info.method_impl_classes.get(&method_key))
            .cloned()
            .unwrap_or(class_name);
        Ok(self.method_return_callable_sig(&impl_class, &method_key, array_return))
    }

    /// Resolves callable-return metadata for a static method call expression.
    fn resolve_static_method_return_callable_sig(
        &self,
        receiver: &StaticReceiver,
        method: &str,
        array_return: bool,
    ) -> Result<Option<FunctionSig>, CompileError> {
        let Some(class_name) = self.resolve_static_method_metadata_class(receiver)? else {
            return Ok(None);
        };
        let method_key = php_symbol_key(method);
        let Some(class_info) = self.classes.get(&class_name) else {
            return Ok(None);
        };
        let impl_class = class_info
            .static_method_impl_classes
            .get(&method_key)
            .or_else(|| class_info.method_impl_classes.get(&method_key))
            .cloned()
            .unwrap_or(class_name);
        Ok(self.method_return_callable_sig(&impl_class, &method_key, array_return))
    }

    /// Looks up a stored callable-return signature in class method metadata.
    fn method_return_callable_sig(
        &self,
        class_name: &str,
        method_key: &str,
        array_return: bool,
    ) -> Option<FunctionSig> {
        let class_info = self.classes.get(class_name)?;
        if array_return {
            class_info
                .callable_array_method_return_sigs
                .get(method_key)
                .cloned()
        } else {
            class_info.callable_method_return_sigs.get(method_key).cloned()
        }
    }

    /// Resolves the concrete class whose method metadata should be inspected.
    fn resolve_static_method_metadata_class(
        &self,
        receiver: &StaticReceiver,
    ) -> Result<Option<String>, CompileError> {
        match receiver {
            StaticReceiver::Named(name) => Ok(self.resolve_class_name_for_metadata(name.as_str())),
            StaticReceiver::Self_ | StaticReceiver::Static => Ok(self.current_class.clone()),
            StaticReceiver::Parent => {
                let Some(current_class) = self.current_class.as_ref() else {
                    return Ok(None);
                };
                Ok(self
                    .classes
                    .get(current_class)
                    .and_then(|class_info| class_info.parent.clone())
                )
            }
        }
    }

    /// Resolves a class name case-insensitively for metadata lookups.
    fn resolve_class_name_for_metadata(&self, class_name: &str) -> Option<String> {
        let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
        self.classes
            .keys()
            .find(|existing| php_symbol_key(existing) == class_key)
            .cloned()
    }

    /// Resolves the callable signature for a ternary or merge branch pair by recursively
    /// resolving each branch and returning the signature only if both branches resolve to
    /// the same signature. Used for `?:` and `??` expressions whose both branches must
    /// agree on the callable type for the expression to be callable.
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
