//! Purpose:
//! Resolves reference effects of calls without treating ordinary values as mutable locals.
//!
//! Called from:
//! - XML receiver expression detection and declaration collection.
//!
//! Key details:
//! - Uses the shared call planner for named and unpacked argument binding.
//! - Unresolved callees stay conservative; known value parameters preserve receiver facts.

use std::collections::HashSet;
use crate::names::php_symbol_key;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::types::{FunctionSig, PhpType};
use super::{class_key, DomReceiver, Parameter};

/// Builds a reference-only signature from the source declaration's visible parameters.
pub(super) fn signature(params: &[Parameter], variadic: Option<&str>, variadic_by_ref: bool) -> FunctionSig {
    let mut sig = FunctionSig {
        params: params.iter().map(|(name, _, _, _)| (name.clone(), PhpType::Mixed)).collect(),
        param_type_exprs: vec![None; params.len()], param_attributes: vec![Vec::new(); params.len()],
        defaults: params.iter().map(|(_, _, default, _)| default.clone()).collect(),
        return_type: PhpType::Mixed, declared_return: false, by_ref_return: false,
        ref_params: params.iter().map(|(_, _, _, by_ref)| *by_ref).collect(),
        declared_params: vec![false; params.len()], variadic: variadic.map(str::to_string),
        deprecation: None, is_generator: false,
    };
    if let Some(name) = variadic {
        sig.params.push((name.to_string(), PhpType::Mixed));
        sig.param_type_exprs.push(None); sig.param_attributes.push(Vec::new());
        sig.defaults.push(None); sig.ref_params.push(variadic_by_ref); sig.declared_params.push(false);
    }
    sig
}

impl DomReceiver {
    /// Finds a method signature through known inheritance without assuming unknown methods are pure.
    fn method_signature(&self, mut class: String, method: &str) -> Option<FunctionSig> {
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(class.clone()) { return None; }
            if let Some(sig) = self.declarations.methods.get(&(class.clone(), php_symbol_key(method))) { return Some(sig.clone()); }
            class = self.declarations.parents.get(&class)?.clone();
        }
    }

    /// Finds the callable signature of a closure, alias, or first-class function/method value.
    pub(super) fn callable_signature(&self, expr: &Expr) -> Option<FunctionSig> {
        match &expr.kind {
            ExprKind::Closure { params, variadic, variadic_by_ref, .. } => Some(signature(params, variadic.as_deref(), *variadic_by_ref)),
            ExprKind::Variable(name) => self.callables.borrow().get(name).cloned(),
            ExprKind::FirstClassCallable(CallableTarget::Function(name)) => self.function_signature(name.as_str()),
            ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
                let classes = self.classes(object)?;
                if classes.len() != 1 { return None; }
                self.method_signature(classes.into_iter().next()?, method)
            }
            _ => None,
        }
    }

    /// Uses user declarations first, then the authoritative builtin signature registry.
    fn function_signature(&self, name: &str) -> Option<FunctionSig> {
        self.declarations.functions.get(&php_symbol_key(name)).cloned()
            .or_else(|| crate::builtins::registry::function_sig(name))
    }

    /// Computes possible argument mutations before their evaluation changes receiver provenance.
    pub(in crate::xml_prelude) fn call_mutations(&self, expr: &Expr) -> Vec<String> {
        let (args, signatures) = match &expr.kind {
            ExprKind::FunctionCall { name, args } => (args, vec![self.function_signature(name.as_str())]),
            ExprKind::ClosureCall { var, args } => (args, vec![self.callables.borrow().get(var).cloned()]),
            ExprKind::ExprCall { callee, args } => (args, vec![self.callable_signature(callee)]),
            ExprKind::MethodCall { object, method, args, .. } | ExprKind::NullsafeMethodCall { object, method, args, .. } => {
                let signatures = self.classes(object).map(|classes| classes.into_iter().map(|class| self.method_signature(class, method)).collect()).unwrap_or_else(|| vec![None]);
                (args, signatures)
            }
            ExprKind::StaticMethodCall { receiver: StaticReceiver::Named(name), method, args } => (args, vec![self.method_signature(class_key(name), method)]),
            ExprKind::NewObject { class_name, args } => {
                // A known class with no constructor has no reference parameters.
                let class = class_key(class_name);
                let sig = self.method_signature(class.clone(), "__construct").or_else(|| self.declarations.classes.contains(&class).then(|| signature(&[], None, false)));
                (args, vec![sig])
            }
            ExprKind::NullsafeDynamicMethodCall { args, .. } | ExprKind::StaticMethodCall { args, .. }
            | ExprKind::NewGeneric { args, .. } | ExprKind::NewDynamic { args, .. }
            | ExprKind::NewDynamicObject { args, .. } | ExprKind::NewScopedObject { args, .. } => (args, vec![None]),
            _ => return Vec::new(),
        };
        let mut mutations = self.escaped_refs.borrow().clone();
        for sig in signatures {
            let planned = sig.as_ref().and_then(|sig| {
                crate::types::call_args::plan_call_args(sig, args, expr.span, false, true).ok()
                    .map(|plan| {
                        let args = plan.normalized_args();
                        crate::types::call_args::expand_planned_positional_spreads(&args).unwrap_or(args)
                    })
            });
            let selected = planned.as_deref().unwrap_or(args);
            for (index, arg) in selected.iter().enumerate() {
                let by_ref = sig.as_ref().is_none_or(|sig| {
                    let regular = crate::types::call_args::regular_param_count(sig);
                    let index = if index >= regular && sig.variadic.is_some() { regular } else { index };
                    sig.ref_params.get(index).copied().unwrap_or(false)
                });
                if !by_ref { continue; }
                let value = if let ExprKind::NamedArg { value, .. } = &arg.kind { value } else { arg };
                if let ExprKind::Variable(name) = &value.kind { mutations.insert(name.clone()); }
            }
        }
        mutations.into_iter().collect()
    }
}
