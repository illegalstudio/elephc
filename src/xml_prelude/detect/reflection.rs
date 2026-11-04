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

    /// Reflection of a SimpleXML/XML class name keeps that class surface available.
    ///
    /// `ReflectionClass('SimpleXMLElement')` names the class only as a string literal, which
    /// the ordinary class-position walk cannot see. `ReflectionObject($xml)` is covered by the
    /// surrounding program's other SimpleXML references; an opaque argument stays conservative
    /// only when it is itself a literal class name.
    pub(super) fn reflection_class_refs_xml(&self, class: &Name, args: &[Expr]) -> bool {
        let key = php_symbol_key(class.as_str());
        if key == "reflectionmethod" { return false; }
        if key != "reflectionclass" && key != "reflectionobject" { return false; }
        args.first().is_some_and(|target| match &target.kind {
            ExprKind::StringLiteral(value) => super::class_string_is_xml(value),
            _ => self.expr_refs_xml(target),
        })
    }

    /// A stream wrapper class is named by a string literal, not a class position.
    pub(super) fn stream_wrapper_refs_xml(&self, name: &Name, args: &[Expr]) -> bool {
        if php_symbol_key(name.as_str()) != "stream_wrapper_register" { return false; }
        args.get(1).is_some_and(|target| match &target.kind {
            ExprKind::StringLiteral(value) => super::class_string_is_xml(value),
            _ => true,
        })
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
