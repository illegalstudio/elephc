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

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind};
use crate::types::deferred_defaults::{contains_deferred_error, prune_literal_branches, unconditional_error_message};

use super::super::Checker;

/// Resolves selectors against complete schemas and republishes class-initialization errors.
pub(super) fn normalize(checker: &mut Checker, program: &Program) {
    let mut globals = HashMap::new();
    collect_globals(program, &mut globals);
    // Walking every statement expression is only useful when some property default actually
    // names a constant that compile-time schemas cannot resolve. Most programs have none, and
    // the full-program expression walk is measurable on the large eval catalog.
    let needs_defines = checker.classes.values().any(|class| {
        class.defaults.iter().chain(&class.static_defaults).flatten()
            .any(|default| unresolved_constant(default, checker, &globals, &HashSet::new()).is_some())
    });
    let mut defines = HashSet::new();
    if needs_defines {
        collect_defines(program, &mut defines);
    }
    let mut updates = Vec::new();
    for (name, class) in &checker.classes {
        let existing_error = class.deferred_property_default_error.clone();
        let needs_rewrite = existing_error.is_none()
            && (class.defaults.iter().chain(&class.static_defaults).flatten()
                .any(contains_deferred_error)
                || class.defaults.iter().chain(&class.static_defaults).flatten()
                    .any(|default| {
                        unresolved_constant(default, checker, &globals, &defines).is_some()
                    }));
        if !needs_rewrite {
            continue;
        }
        let mut defaults = class.defaults.clone();
        let mut statics = class.static_defaults.clone();
        for default in defaults.iter_mut().chain(&mut statics).flatten() {
            bind_constants(default, name, checker, &globals, 0);
        }
        let error = existing_error.or_else(|| {
            defaults.iter().chain(&statics).flatten()
                .find_map(unconditional_error_message).map(str::to_owned)
                .or_else(|| defaults.iter().chain(&statics).flatten()
                    .find_map(|default| unresolved_constant(default, checker, &globals, &defines))
                    .map(|name| format!("Undefined constant \"{name}\"")))
        });
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

/// Collects literal `define("NAME", …)` names anywhere in the program.
///
/// The checker does not evaluate runtime `define()` calls, but a property default may legally
/// refer to one. Walking every statement body here is deliberately conservative: an extra name
/// only means the default is left to normal lowering instead of being reported as undefined.
fn collect_defines(program: &Program, defines: &mut HashSet<String>) {
    for statement in program {
        collect_defines_stmt(statement, defines);
    }
}

/// Walks one statement, its conditions and every nested statement body for literal `define()` calls.
fn collect_defines_stmt(statement: &Stmt, defines: &mut HashSet<String>) {
    use StmtKind::*;
    match &statement.kind {
        ExprStmt(expr) | Echo(expr) | Throw(expr) => collect_defines_expr(expr, defines),
        Assign { value, .. } | RefAssign { source: value, .. }
        | ArrayPush { value, .. } | TypedAssign { value, .. }
        | ListUnpack { value, .. } | StaticVar { init: value, .. }
        | ConstDecl { value, .. } | Return(Some(value)) => collect_defines_expr(value, defines),
        ArrayAssign { index, value, .. } => {
            collect_defines_expr(index, defines);
            collect_defines_expr(value, defines);
        }
        If { condition, then_body, elseif_clauses, else_body } => {
            collect_defines_expr(condition, defines);
            then_body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            for (condition, body) in elseif_clauses {
                collect_defines_expr(condition, defines);
                body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            }
            if let Some(body) = else_body {
                body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            }
        }
        IfDef { then_body, else_body, .. } => {
            then_body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            if let Some(body) = else_body {
                body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            }
        }
        While { condition, body } => {
            collect_defines_expr(condition, defines);
            body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
        }
        DoWhile { body, condition } => {
            body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            collect_defines_expr(condition, defines);
        }
        For { init, condition, update, body } => {
            if let Some(init) = init {
                collect_defines_stmt(init, defines);
            }
            if let Some(condition) = condition {
                collect_defines_expr(condition, defines);
            }
            if let Some(update) = update {
                collect_defines_stmt(update, defines);
            }
            body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
        }
        NestedArrayAssign { target, value } => {
            collect_defines_expr(target, defines);
            collect_defines_expr(value, defines);
        }
        Foreach { array, body, .. } => {
            collect_defines_expr(array, defines);
            body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
        }
        Switch { subject, cases, default } => {
            collect_defines_expr(subject, defines);
            for (conditions, body) in cases {
                conditions.iter().for_each(|condition| collect_defines_expr(condition, defines));
                body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            }
            if let Some(body) = default {
                body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            }
        }
        Include { path, .. } => collect_defines_expr(path, defines),
        FunctionDecl { body, .. } => {
            // Compiler-injected prelude functions are `SourceMode::Internal`; none defines a
            // user constant, and walking the full eval catalog is measurable at compile time.
            if statement.source_mode != crate::source::SourceMode::Internal {
                body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            }
        }
        IncludeOnceGuard { body, .. } | Synthetic(body) | NamespaceBlock { body, .. } => {
            body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
        }
        Try { try_body, catches, finally_body } => {
            try_body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            for catch in catches {
                catch.body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            }
            if let Some(body) = finally_body {
                body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
            }
        }
        ClassDecl { methods, .. } | EnumDecl { methods, .. }
        | TraitDecl { methods, .. } | InterfaceDecl { methods, .. } => {
            if statement.source_mode != crate::source::SourceMode::Internal {
                for method in methods {
                    method.body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
                }
            }
        }
        PropertyAssign { object, value, .. } => {
            collect_defines_expr(object, defines);
            collect_defines_expr(value, defines);
        }
        StaticPropertyAssign { value, .. } | StaticPropertyArrayPush { value, .. } => {
            collect_defines_expr(value, defines);
        }
        StaticPropertyArrayAssign { index, value, .. } => {
            collect_defines_expr(index, defines);
            collect_defines_expr(value, defines);
        }
        PropertyArrayPush { object, value, .. } => {
            collect_defines_expr(object, defines);
            collect_defines_expr(value, defines);
        }
        PropertyArrayAssign { object, index, value, .. } => {
            collect_defines_expr(object, defines);
            collect_defines_expr(index, defines);
            collect_defines_expr(value, defines);
        }
        Break(_) | Continue(_) | Return(None) | NamespaceDecl { .. } | UseDecl { .. }
        | FunctionVariantGroup { .. } | FunctionVariantMark { .. } | Global { .. }
        | PackedClassDecl { .. } | ExternFunctionDecl { .. } | ExternClassDecl { .. }
        | ExternGlobalDecl { .. } | IncludeOnceMark { .. } => {}
    }
}

/// Records a literal `define("NAME", …)` call found anywhere in one expression tree.
fn collect_defines_expr(expr: &Expr, defines: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            if crate::names::php_symbol_key(name.as_str().trim_start_matches('\\')) == "define" {
                let literal = args.first().and_then(|arg| match &arg.kind {
                    ExprKind::StringLiteral(name) => Some(name.as_str()),
                    ExprKind::NamedArg { name, value } if name == "name" => match &value.kind {
                        ExprKind::StringLiteral(name) => Some(name.as_str()),
                        _ => None,
                    },
                    _ => None,
                });
                if let Some(name) = literal {
                    defines.insert(name.trim_start_matches('\\').to_string());
                }
            }
            args.iter().for_each(|arg| collect_defines_expr(arg, defines));
        }
        ExprKind::ClosureCall { args, .. } | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. } | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            args.iter().for_each(|arg| collect_defines_expr(arg, defines));
        }
        ExprKind::ExprCall { callee, args } => {
            collect_defines_expr(callee, defines);
            args.iter().for_each(|arg| collect_defines_expr(arg, defines));
        }
        ExprKind::NewDynamic { name_expr, args } => {
            collect_defines_expr(name_expr, defines);
            args.iter().for_each(|arg| collect_defines_expr(arg, defines));
        }
        ExprKind::NewDynamicObject { class_name, args, .. } => {
            collect_defines_expr(class_name, defines);
            args.iter().for_each(|arg| collect_defines_expr(arg, defines));
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_defines_expr(object, defines);
            args.iter().for_each(|arg| collect_defines_expr(arg, defines));
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            collect_defines_expr(object, defines);
            collect_defines_expr(method, defines);
            args.iter().for_each(|arg| collect_defines_expr(arg, defines));
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_defines_expr(left, defines);
            collect_defines_expr(right, defines);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_defines_expr(value, defines);
            if let crate::parser::ast::InstanceOfTarget::Expr(target) = target {
                collect_defines_expr(target, defines);
            }
        }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::Throw(value) | ExprKind::ErrorSuppress(value) | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. } | ExprKind::Clone(value)
        | ExprKind::PtrCast { expr: value, .. } | ExprKind::YieldFrom(value)
        | ExprKind::ObjectClassName { object: value } | ExprKind::BufferNew { len: value, .. }
        | ExprKind::NamedArg { value, .. } | ExprKind::Spread(value)
        | ExprKind::IncludeValue { path: value, .. } => collect_defines_expr(value, defines),
        ExprKind::Pipe { value, callable } | ExprKind::NullCoalesce { value, default: callable }
        | ExprKind::ShortTernary { value, default: callable } => {
            collect_defines_expr(value, defines);
            collect_defines_expr(callable, defines);
        }
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            collect_defines_expr(target, defines);
            collect_defines_expr(value, defines);
            if let Some(result) = result_target {
                collect_defines_expr(result, defines);
            }
            prelude.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            collect_defines_expr(condition, defines);
            collect_defines_expr(then_expr, defines);
            collect_defines_expr(else_expr, defines);
        }
        ExprKind::Match { subject, arms, default } => {
            collect_defines_expr(subject, defines);
            for (conditions, value) in arms {
                conditions.iter().for_each(|condition| collect_defines_expr(condition, defines));
                collect_defines_expr(value, defines);
            }
            if let Some(default) = default {
                collect_defines_expr(default, defines);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_defines_expr(array, defines);
            collect_defines_expr(index, defines);
        }
        ExprKind::ArrayLiteral(items) => {
            items.iter().for_each(|item| collect_defines_expr(item, defines));
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_defines_expr(key, defines);
                collect_defines_expr(value, defines);
            }
        }
        ExprKind::ArrayLiteralMixed(items) => {
            for item in items {
                item.exprs().for_each(|value| collect_defines_expr(value, defines));
            }
        }
        ExprKind::Closure { body, .. } => {
            body.iter().for_each(|stmt| collect_defines_stmt(stmt, defines));
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_defines_expr(object, defines);
            collect_defines_expr(property, defines);
        }
        ExprKind::PropertyAccess { object, .. } | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_defines_expr(object, defines);
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method { object, .. }) => {
            collect_defines_expr(object, defines);
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_defines_expr(key, defines);
            }
            if let Some(value) = value {
                collect_defines_expr(value, defines);
            }
        }
        _ => {}
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
/// Returns one constant reference that is unknown to every compile-time and runtime source.
///
/// This runs after `bind_constants` and after constant folding/pruning, so an undefined name in
/// a branch PHP removes is never reported. A name known to the checker's prescan, to any catalog
/// route, or to a `const` declaration is left for normal lowering (which handles runtime
/// `define()` and target-dependent constants).
fn unresolved_constant(
    expr: &Expr, checker: &Checker, globals: &HashMap<String, Expr>, defines: &HashSet<String>,
) -> Option<String> {
    if let ExprKind::ConstRef(name) = &expr.kind {
        let name = name.as_str().trim_start_matches('\\');
        if globals.contains_key(name)
            || defines.contains(name)
            || checker.constants.contains_key(name)
            || elephc_builtin_contract::lookup_constant(name).is_some()
        {
            return None;
        }
        return Some(name.to_string());
    }
    let mut found = None;
    let mut visit = |value: &Expr| {
        if found.is_none() {
            found = unresolved_constant(value, checker, globals, defines);
        }
    };
    match &expr.kind {
        ExprKind::ArrayLiteral(items) | ExprKind::NewObject { args: items, .. }
        | ExprKind::NewScopedObject { args: items, .. } => items.iter().for_each(&mut visit),
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items { visit(key); visit(value); }
        }
        ExprKind::ArrayLiteralMixed(items) => {
            for item in items { item.exprs().for_each(&mut visit); }
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
            for (conditions, value) in arms {
                conditions.iter().for_each(&mut visit);
                visit(value);
            }
            if let Some(default) = default { visit(default); }
        }
        ExprKind::ArrayAccess { array, index } => { visit(array); visit(index); }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::NamedArg { value, .. } | ExprKind::Spread(value) | ExprKind::Cast { expr: value, .. } => visit(value),
        _ => {}
    }
    found
}

/// Binds canonical source constants into a property default and folds literal branches.
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
