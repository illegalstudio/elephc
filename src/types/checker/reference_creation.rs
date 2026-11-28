//! Purpose:
//! Plans plain-local reference formation before source-order call argument effects.
//!
//! Called from:
//! - Argument effects for declared functions, members, constructors and tracked callables.
//!
//! Key details:
//! - The shared argument planner owns matching, unpack provenance and source indices.
//! - Selecting a reference source does not create it until its argument is reached.

use std::collections::HashSet;
use super::Checker;
use crate::{errors::CompileError, parser::ast::{Expr, ExprKind}, span::Span, types::{FunctionSig, PhpType, TypeEnv}};

/// Source-order arguments paired with the planner's directly supplied reference positions.
pub(crate) struct ReferenceCallArguments {
    pub args: Vec<Expr>,
    pub reference_sources: HashSet<usize>,
}

impl ReferenceCallArguments {
    /// Keeps the existing ordinary effects walk when no reference signature is available.
    fn ordinary(args: &[Expr], profile: elephc_semantics::SemanticProfile) -> Self {
        Self { args: crate::types::call_args::expand_static_assoc_spread_args(args, profile),
            reference_sources: HashSet::new() }
    }
}

impl Checker {
    /// Reads method and constructor shapes without creating a descriptor or specializing a body.
    pub(crate) fn member_call_reference_arguments(&self, call: &Expr, receiver: Option<&PhpType>,
        args: &[Expr], env: &TypeEnv,
    ) -> Result<ReferenceCallArguments, CompileError> {
        let (class, method, is_static) = match &call.kind {
            ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
                let class = receiver.and_then(|ty| match ty {
                    PhpType::Object(name) if !name.is_empty() => Some(name.clone()),
                    PhpType::Union(_) => self.union_single_object_class(ty),
                    _ => None,
                });
                (class, method.as_str(), false)
            }
            ExprKind::StaticMethodCall { receiver, method, .. } =>
                (self.resolve_static_method_metadata_class(receiver)?, method.as_str(), true),
            ExprKind::NewObject { class_name, .. } =>
                (self.resolve_class_name_for_metadata(class_name.as_str()), "__construct", false),
            ExprKind::NewScopedObject { receiver, .. } =>
                (self.resolve_static_method_metadata_class(receiver)?, "__construct", false),
            _ => return Ok(ReferenceCallArguments::ordinary(args, self.semantic_profile)),
        };
        let method = crate::names::php_symbol_key(method);
        let signature = class.as_ref().and_then(|class| {
            if let Some(class) = self.classes.get(class) {
                if is_static { class.static_methods.get(&method) } else { class.methods.get(&method) }
            } else {
                self.interfaces.get(class).and_then(|interface|
                    if is_static { interface.static_methods.get(&method) } else { interface.methods.get(&method) })
            }
        });
        Ok(self.reference_arguments_for_signature(signature, args, call.span, env))
    }

    /// Uses the authoritative callable resolver after evaluating the callee expression once.
    pub(crate) fn expression_call_reference_arguments(&mut self, callee: &Expr, args: &[Expr],
        span: Span, env: &TypeEnv,
    ) -> Result<ReferenceCallArguments, CompileError> {
        let signature = self.resolve_expr_callable_sig(callee, env)?;
        Ok(self.reference_arguments_for_signature(signature.as_ref(), args, span, env))
    }

    /// Reads a declared reference shape without walking the deferred body or specializing values.
    pub(crate) fn declared_call_reference_arguments(&self, name: &str, args: &[Expr], span: Span,
        env: &TypeEnv,
    ) -> ReferenceCallArguments {
        let Some(name) = self.canonical_function_name_folded(name) else {
            return ReferenceCallArguments::ordinary(args, self.semantic_profile);
        };
        if let Some(signature) = self.functions.get(&name) {
            return self.reference_arguments_for_signature(Some(signature), args, span, env);
        }
        let signature = self.fn_decls.get(&name).filter(|decl| decl.ref_params.iter().any(|flag| *flag))
            .map(|decl| {
                let parameters = decl.params.iter().cloned().map(|name| (name, PhpType::Mixed))
                    .chain(decl.variadic.iter().cloned().map(|name|
                        (name, PhpType::Array(Box::new(PhpType::Mixed))))).collect();
                self.provisional_function_sig(decl, parameters)
            });
        self.reference_arguments_for_signature(signature.as_ref(), args, span, env)
    }

    /// Uses the same source projection for an already tracked callable signature.
    pub(crate) fn callable_reference_arguments(&self, name: &str, args: &[Expr], span: Span,
        env: &TypeEnv,
    ) -> ReferenceCallArguments {
        self.reference_arguments_for_signature(self.callable_sigs.get(name), args, span, env)
    }

    /// Retains the planner's source list so generated or shared spans cannot select another argument.
    fn reference_arguments_for_signature(&self, signature: Option<&FunctionSig>, args: &[Expr], span: Span,
        env: &TypeEnv,
    ) -> ReferenceCallArguments {
        let prepared = signature.filter(|sig| sig.ref_params.iter().any(|flag| *flag)).and_then(|sig| {
            // The authoritative call checker owns planning diagnostics and their callee labels.
            let plan = self.plan_named_call_args(sig, args, span, "Reference argument", env).ok()?;
            let reference_sources = plan.reference_source_indices(sig).into_iter().collect();
            Some(ReferenceCallArguments { args: plan.source_args, reference_sources })
        });
        prepared.unwrap_or_else(|| ReferenceCallArguments::ordinary(args, self.semantic_profile))
    }

    /// Initializes a selected missing reference only at its source-order evaluation point.
    pub(crate) fn prepare_reference_source_argument(&mut self, argument: &Expr, index: usize,
        sources: &HashSet<usize>, env: &mut TypeEnv,
    ) {
        if sources.contains(&index) {
            if let Some(name) = reference_variable(argument) { self.initialize_missing_reference_local(name, env); }
        }
    }
}

/// Unwraps argument syntax without treating expression values or unpack elements as caller storage.
fn reference_variable(argument: &Expr) -> Option<&str> {
    match &argument.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) => reference_variable(value),
        _ => None,
    }
}
