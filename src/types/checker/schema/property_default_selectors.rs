//! Purpose:
//! Selects pure constant branches before publishing deferred property initialization errors.
//!
//! Called from:
//! - `super::defaults::validate_deferred_declaration_defaults()` after schemas are complete.
//!
//! Key details:
//! - Only property copies containing deferred errors are rewritten; callable source defaults stay intact.
//! - Resolution is bounded, lexical and target-neutral, and never evaluates a constructor.
//! - Parent initialization failures propagate even when a private property has no child slot.

use std::collections::HashMap;

use crate::parser::ast::{Expr, ExprKind, Program, StaticReceiver, StmtKind};
use crate::types::deferred_defaults::{contains_deferred_error, prune_literal_branches, unconditional_error_message};

use super::super::Checker;

/// Resolves selectors against complete schemas and republishes class-initialization errors.
pub(super) fn normalize(checker: &mut Checker, program: &Program) {
    let mut globals = HashMap::new();
    collect_globals(program, &mut globals);
    let mut updates = Vec::new();
    for (name, class) in &checker.classes {
        if class.deferred_property_default_error.is_some()
            || !class.defaults.iter().chain(&class.static_defaults).flatten().any(contains_deferred_error)
        { continue; }
        let mut defaults = class.defaults.clone();
        let mut statics = class.static_defaults.clone();
        for default in defaults.iter_mut().chain(&mut statics).flatten() {
            bind_constants(default, name, checker, &globals, 0);
        }
        let error = defaults.iter().chain(&statics).flatten()
            .find_map(unconditional_error_message).map(str::to_owned);
        updates.push((name.clone(), defaults, statics, error));
    }
    for (name, defaults, statics, error) in updates {
        let class = checker.classes.get_mut(&name).unwrap();
        class.defaults = defaults;
        class.static_defaults = statics;
        class.deferred_property_default_error = error;
    }
    loop {
        let updates: Vec<_> = checker.classes.iter().filter_map(|(name, class)| {
            let parent = checker.classes.get(class.parent.as_ref()?)?;
            let error = parent.deferred_property_default_error.as_ref()?;
            (class.deferred_property_default_error.as_ref() != Some(error))
                .then(|| (name.clone(), error.clone()))
        }).collect();
        if updates.is_empty() { break; }
        for (name, error) in updates {
            checker.classes.get_mut(&name).unwrap().deferred_property_default_error = Some(error);
        }
    }
}

/// Retains canonical source constants without evaluating runtime `define()` calls.
fn collect_globals(program: &Program, globals: &mut HashMap<String, Expr>) {
    for statement in program {
        match &statement.kind {
            StmtKind::ConstDecl { name, value } => { globals.insert(name.clone(), value.clone()); }
            StmtKind::NamespaceBlock { body, .. } => collect_globals(body, globals),
            _ => {}
        }
    }
}

/// Finds a constant in its declaring class or interface, preserving lexical scope on chains.
fn class_constant(checker: &Checker, owner: &str, name: &str, depth: usize) -> Option<(String, Expr)> {
    if depth > 64 { return None; }
    if let Some(class) = checker.classes.get(owner) {
        if let Some(value) = class.constants.get(name) { return Some((owner.into(), value.clone())); }
        if let Some(value) = class.interfaces.iter().find_map(|interface|
            class_constant(checker, interface, name, depth + 1)) { return Some(value); }
        return class.parent.as_deref().and_then(|parent|
            class_constant(checker, parent, name, depth + 1));
    }
    let interface = checker.interfaces.get(owner)?;
    if let Some(value) = interface.constants.get(name) { return Some((owner.into(), value.clone())); }
    interface.parents.iter().find_map(|parent| class_constant(checker, parent, name, depth + 1))
}

/// Substitutes known constants and reuses PHP-equivalent folding before lazy-error selection.
fn bind_constants(
    expr: &mut Expr, owner: &str, checker: &Checker, globals: &HashMap<String, Expr>, depth: usize,
) {
    if depth > 64 { return; }
    let replacement = match &expr.kind {
        ExprKind::ConstRef(name) => globals.get(name.as_str().trim_start_matches('\\'))
            .cloned().or_else(|| {
                let constant = elephc_builtin_contract::lookup_constant(name.as_str())?;
                if !matches!(constant.route, elephc_builtin_contract::ConstantRoute::Predefined)
                    || matches!(constant.value, elephc_builtin_contract::ConstValue::StreamResource(_))
                { return None; }
                crate::types::predefined_constants::literal_of(constant.value)
                    .map(|kind| Expr::new(kind, expr.span))
            }).map(|value| (owner.to_string(), value)),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let class = match receiver.written_class_receiver() {
                StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
                StaticReceiver::Self_ => Some(owner.into()),
                StaticReceiver::Parent => checker.classes.get(owner).and_then(|class| class.parent.clone()),
                _ => None,
            };
            class.and_then(|class| class_constant(checker, &class, name, 0))
        }
        _ => None,
    };
    if let Some((scope, mut value)) = replacement {
        bind_constants(&mut value, &scope, checker, globals, depth + 1);
        value.span = expr.span;
        *expr = value;
        return;
    }
    let mut visit = |value: &mut Expr| bind_constants(value, owner, checker, globals, depth + 1);
    match &mut expr.kind {
        ExprKind::ArrayLiteral(items) | ExprKind::NewObject { args: items, .. }
        | ExprKind::NewScopedObject { args: items, .. } => items.iter_mut().for_each(&mut visit),
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items { visit(key); visit(value); }
        }
        ExprKind::ArrayLiteralMixed(items) => {
            for item in items { item.exprs_mut().for_each(&mut visit); }
        }
        ExprKind::BinaryOp { left, right, .. } => { visit(left); visit(right); }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            visit(condition); visit(then_expr); visit(else_expr);
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            visit(value); visit(default);
        }
        ExprKind::Match { subject, arms, default } => {
            visit(subject);
            for (conditions, value) in arms { conditions.iter_mut().for_each(&mut visit); visit(value); }
            if let Some(default) = default { visit(default); }
        }
        ExprKind::ArrayAccess { array, index } => { visit(array); visit(index); }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::NamedArg { value, .. } | ExprKind::Spread(value) | ExprKind::Cast { expr: value, .. } => visit(value),
        _ => {}
    }
    *expr = crate::optimize::fold_constant_expression(expr.clone());
    prune_literal_branches(expr);
}
