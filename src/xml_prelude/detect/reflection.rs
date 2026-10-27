//! Purpose:
//! Detects reflective DOM XML loader targets before the reflection lowering pass runs.
//!
//! Called from:
//! - XML expression detection for ReflectionMethod construction and ReflectionClass lookups.
//!
//! Key details:
//! - Uses callback receiver provenance for literal targets and stays conservative for opaque arguments.

use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind};
use super::Detector;

impl Detector {
    /// Recognizes the positional constructor shapes accepted by ReflectionMethod lowering.
    pub(super) fn reflection_constructor_refs_xml(&self, class: &Name, args: &[Expr]) -> bool {
        if php_symbol_key(class.as_str()) != "reflectionmethod" { return false; }
        match args {
            [target, method] if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) =>
                self.callback_method_refs_xml(target, method),
            [target] => match &target.kind {
                ExprKind::StringLiteral(value) => {
                    let Some((class, method)) = value.split_once("::") else { return false; };
                    let class = Expr::new(ExprKind::StringLiteral(class.into()), target.span);
                    let method = Expr::new(ExprKind::StringLiteral(method.into()), target.span);
                    self.callback_method_refs_xml(&class, &method)
                }
                _ => self.dom.may_load_callback(target),
            },
            // Named and unpacked constructor arguments are normalized later by the shared planner.
            // Early capability discovery must not guess a different argument mapping.
            _ => self.dom.dom_available(),
        }
    }

    /// Reflective method lists and lookups may expose the loader for later static invocation.
    pub(super) fn reflection_lookup_refs_xml(&self, object: &Expr, method: &str, args: &[Expr]) -> bool {
        let lookup = if method.eq_ignore_ascii_case("getMethods") { true }
            else if method.eq_ignore_ascii_case("getMethod") {
                args.first().is_none_or(|arg| match &arg.kind {
                    ExprKind::NamedArg { value, .. } => Self::method_may_load_xml(value),
                    _ => Self::method_may_load_xml(arg),
                })
            } else { false };
        lookup && self.dom.may_reflect_dom(object)
    }
}
