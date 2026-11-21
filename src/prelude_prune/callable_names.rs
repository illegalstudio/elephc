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
    /// Names written by anything other than a plain copy of another variable.
    hard_opaque: HashSet<String>,
    /// Names assigned a copy, or a choice between simple callables and copies.
    composite: HashSet<String>,
    /// `(variable, source)` edges from `$variable = $source` copies.
    aliases: Vec<(String, String)>,
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
        } else if let Some(sources) = composite_callable_sources(value) {
            // A copy, or a choice between simple callables and copies, stays opaque for the
            // environment gate (`has_unknown_target`), which does not follow copies, but is
            // remembered for `has_unknown_target_through_copies`.
            self.opaque.insert(name.to_owned());
            self.composite.insert(name.to_owned());
            for source in sources {
                self.aliases.push((name.to_owned(), source));
            }
        } else {
            self.write(name);
        }
    }

    /// Invalidates a name whose value can be supplied or changed outside a simple assignment.
    pub(crate) fn write(&mut self, name: &str) {
        self.opaque.insert(name.to_owned());
        self.hard_opaque.insert(name.to_owned());
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

    /// Like [`Self::has_unknown_target`], but a variable that only ever receives simple
    /// callables or copies of variables that do (`$g = $f; $g()`) is known too. Copy cycles
    /// are known: they can only ever hold what some simple binding gave them.
    pub(crate) fn has_unknown_target_through_copies(&self) -> bool {
        if self.opaque_expression {
            return true;
        }
        let mut known: HashSet<&str> = self
            .assigned
            .iter()
            .chain(self.composite.iter())
            .filter(|name| !self.hard_opaque.contains(*name))
            .map(String::as_str)
            .collect();
        loop {
            let before = known.len();
            for (name, source) in &self.aliases {
                if !known.contains(source.as_str()) {
                    known.remove(name.as_str());
                }
            }
            if known.len() == before {
                break;
            }
        }
        self.invoked.iter().any(|name| !known.contains(name.as_str()))
    }

    /// Returns true when any call target has not been proven to have only simple bindings.
    pub(crate) fn has_unknown_target(&self) -> bool {
        self.opaque_expression || self.invoked.iter().any(|name| {
            !self.assigned.contains(name) || self.opaque.contains(name)
        })
    }
}

/// For a copy of a variable, or a ternary / `match` / `??` choosing among simple callables and
/// variable copies, returns the variables copied from; `None` for any other value.
fn composite_callable_sources(value: &Expr) -> Option<Vec<String>> {
    fn collect(value: &Expr, sources: &mut Vec<String>) -> bool {
        match &value.kind {
            ExprKind::Variable(source) => {
                sources.push(source.clone());
                true
            }
            ExprKind::Ternary { then_expr, else_expr, .. } => {
                collect(then_expr, sources) && collect(else_expr, sources)
            }
            ExprKind::Match { arms, default, .. } => {
                arms.iter().all(|(_, arm)| collect(arm, sources))
                    && default.as_ref().is_none_or(|default| collect(default, sources))
            }
            ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
                collect(value, sources) && collect(default, sources)
            }
            _ => simple_callable(value),
        }
    }
    let mut sources = Vec::new();
    collect(value, &mut sources).then_some(sources)
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
