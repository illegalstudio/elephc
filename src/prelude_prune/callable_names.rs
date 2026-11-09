//! Purpose:
//! Conservatively distinguishes simple callable bindings from computed call targets.
//!
//! Called from:
//! - The shared AST usage walk and the web environment demand gate.
//!
//! Key details:
//! - Facts are unions across scopes and paths. Any opaque binding or escape wins.
//! - Unknown targets can invoke a variable-table observer after later constant propagation.

use crate::fast_hash::FastSet as HashSet;

use crate::parser::ast::{CallableTarget, Expr, ExprKind};

/// Whole-program callable bindings used only to prove that environment copying can be omitted.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallableNames {
    assigned: HashSet<String>,
    opaque: HashSet<String>,
    invoked: HashSet<String>,
    opaque_expression: bool,
}

impl CallableNames {
    /// Seeds a parameter or by-value capture whose callers supply only simple identities.
    pub(super) fn simple_binding(&mut self, name: &str) {
        self.assigned.insert(name.to_owned());
    }

    /// Copies only explicit by-value captures into a fresh callable scope.
    pub(super) fn captures(&self, captures: &[String]) -> Self {
        let mut scope = Self::default();
        for name in captures {
            if self.assigned.contains(name) && !self.opaque.contains(name) {
                scope.simple_binding(name);
            } else { scope.write(name); }
        }
        scope
    }

    /// Defers invoked capture dependencies to the enclosing scope's complete binding summary.
    pub(super) fn propagate_capture_calls(&self, outer: &mut Self, captures: &[String]) {
        for name in captures {
            if self.invoked.contains(name) { outer.variable_call(name); }
        }
    }

    /// Records a binding, retaining every opaque assignment even if a later one is simple.
    pub(crate) fn assignment(&mut self, name: &str, value: &Expr) {
        if simple_callable(value) {
            self.assigned.insert(name.to_owned());
        } else {
            self.write(name);
        }
    }

    /// Invalidates a name whose value can be supplied or changed outside a simple assignment.
    pub(crate) fn write(&mut self, name: &str) {
        self.opaque.insert(name.to_owned());
    }

    /// Invalidates the local owner of a write through array or string dimensions.
    pub(crate) fn write_target(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::Variable(name) => self.write(name),
            ExprKind::ArrayAccess { array, .. } => self.write_target(array),
            _ => {}
        }
    }

    /// Records a call through a named variable.
    pub(crate) fn variable_call(&mut self, name: &str) {
        self.invoked.insert(name.to_owned());
    }

    /// Records a callable expression without attempting to normalize computed names locally.
    pub(crate) fn expression_call(&mut self, callee: &Expr) {
        if let ExprKind::Variable(name) = &callee.kind {
            self.variable_call(name);
        } else if !simple_callable(callee) {
            self.unknown_call();
        }
    }

    /// Retains the environment for callback APIs whose target is not proven by this scan.
    pub(crate) fn unknown_call(&mut self) {
        self.opaque_expression = true;
    }

    /// Treats argument variables as potential by-reference escapes, without guessing signatures.
    pub(crate) fn arguments(&mut self, args: &[Expr]) {
        for arg in args {
            let mut value = arg;
            while let ExprKind::NamedArg { value: inner, .. } | ExprKind::Spread(inner) = &value.kind {
                value = inner;
            }
            self.write_target(value);
        }
    }

    /// Returns true when any call target has not been proven to have only simple bindings.
    pub(crate) fn has_unknown_target(&self) -> bool {
        self.opaque_expression || self.invoked.iter().any(|name| {
            !self.assigned.contains(name) || self.opaque.contains(name)
        })
    }
}

/// Identifies values whose callable identity cannot be synthesized by string computation.
pub(super) fn simple_callable(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::StringLiteral(name) => crate::names::literal_symbol_name(name)
            .is_some_and(|name| !super::usage::callable_gateway(&name)),
        ExprKind::Closure { .. } => true,
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            !super::usage::callable_gateway(name.as_str())
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { method, .. }
            | CallableTarget::StaticMethod { method, .. }) => !method.eq_ignore_ascii_case("fromCallable"),
        ExprKind::ArrayLiteral(items) => match items.as_slice() {
            [_, Expr { kind: ExprKind::StringLiteral(name), .. }] => crate::names::literal_symbol_name(name)
                .is_some_and(|name| !name.eq_ignore_ascii_case("fromCallable")),
            _ => false,
        },
        _ => false,
    }
}
