//! Purpose:
//! Selects pure constant branches before publishing deferred property initialization errors.
//!
//! Called from:
//! - `super::defaults::validate_deferred_declaration_defaults()` after schemas are complete.
//! - `super::enum_backing` (constant binding only) while backed enum case values are folded.
//! - `crate::types::checker::driver` (global constant collection) before enums are built.
//!
//! Key details:
//! - Only property copies containing deferred errors are rewritten; callable source defaults stay intact.
//! - Resolution is bounded, lexical and target-neutral, and never evaluates a constructor.
//! - Parent initialization failures propagate even when a private property has no child slot.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind};
use crate::types::deferred_defaults::{
    contains_deferred_error, prune_literal_branches, unconditional_error_message,
};

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
    let mut defines = DefineFacts::default();
    defines.semantic_profile = checker.semantic_profile;
    if needs_defines {
        collect_defines(program, &mut defines);
    }
    // The lowering pass runs on the optimized AST, which may have removed an unused function
    // holding the only `eval()`. Publish the pre-optimization fact so schema deferral and EIR
    // lowering always agree on whether an initializer constant needs the eval-aware lookup.
    checker.program_contains_eval |= defines.contains_eval;
    checker.native_defined_constants.extend(defines.names.iter().cloned());
    // An untyped slot whose default is left to the eval-aware runtime lookup must store the
    // fetched value as Mixed. Syntactic inference sees only the name and guesses `int`, which
    // would truncate an eval-defined string, float, array or object to its low word.
    if defines.contains_eval {
        let mut widen = Vec::new();
        for (class_name, class) in &checker.classes {
            for index in 0..class.properties.len() {
                let property = &class.properties[index].0;
                if class.property_slot_is_declared(index, property) {
                    continue;
                }
                let Some(default) = class.defaults.get(index).and_then(Option::as_ref) else {
                    continue;
                };
                // A compiled `define()` only makes the name *possibly* available from native
                // storage; `eval()` may be the definer that actually ran. Choose Mixed storage
                // for any constant the runtime decides, not just names without a compiled
                // `define()`.
                if unresolved_constant(default, checker, &globals, &HashSet::new()).is_some() {
                    widen.push((class_name.clone(), index));
                }
            }
        }
        for (class_name, index) in widen {
            if let Some(class) = checker.classes.get_mut(&class_name) {
                class.properties[index].1 = crate::types::PhpType::Mixed;
            }
        }
    }
    let mut updates = Vec::new();
    for (name, class) in &checker.classes {
        let existing_error = class.deferred_property_default_error.clone();
        let needs_rewrite = existing_error.is_none()
            && (class.defaults.iter().chain(&class.static_defaults).flatten()
                .any(contains_deferred_error)
                || class.defaults.iter().chain(&class.static_defaults).flatten()
                    .any(|default| {
                        unresolved_constant(default, checker, &globals, &defines.names).is_some()
                    }));
        if !needs_rewrite {
            continue;
        }
        let mut defaults = class.defaults.clone();
        let mut statics = class.static_defaults.clone();
        for default in defaults.iter_mut().flatten() {
            bind_constants(default, name, checker, &globals, 0);
            // A name that survives binding is unknown while compiling. PHP only fails when it
            // EVALUATES that name, so leave a lazy throw in its branch instead of failing the
            // whole class: `COND ? 1 : MISSING` must still build `COND`-true objects.
            //
            // Dynamic `eval()` can define a constant after schema creation. Replacing those
            // references would make construction throw before the eval-aware constant lookup
            // can see the definition, so leave them as ordinary constant reads. Static
            // initializers still materialize before user code runs, so they keep the deferred
            // error until they can be evaluated lazily.
            if !defines.contains_eval {
                replace_unresolved_constants(default, checker, &globals, &defines.names, 0);
            }
        }
        for default in statics.iter_mut().flatten() {
            bind_constants(default, name, checker, &globals, 0);
            replace_unresolved_constants(default, checker, &globals, &defines.names, 0);
        }
        let error = existing_error.or_else(|| {
            defaults.iter().chain(&statics).flatten()
                .find_map(unconditional_error_message).map(str::to_owned)
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
pub(crate) fn collect_globals(program: &Program, globals: &mut HashMap<String, Expr>) {
    for statement in program {
        match &statement.kind {
            StmtKind::ConstDecl { name, value } => { globals.insert(name.clone(), value.clone()); }
            // A `require_once`d file's declarations stay inside its include guard.
            StmtKind::NamespaceBlock { body, .. } | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::Synthetic(body) => collect_globals(body, globals),
            _ => {}
        }
    }
}

/// Returns the statically known `constant_name` argument of one `define()` call.
///
/// Named arguments can appear in any order, so `constant_name` is looked up wherever it is.
/// A literal unpack reaches the call as a positional argument after PHP's compile-time
/// expansion, so `define(...["X", 7])` defines `X` exactly like `define("X", 7)`.
fn literal_define_name(
    args: &[Expr],
    profile: elephc_semantics::SemanticProfile,
) -> Option<String> {
    // Let the shared planner expand literal associative unpacks; its integer-key order is
    // PHP array order, not a hard-coded key `0`.
    let expanded = crate::types::call_args::expand_static_assoc_spread_args(args, profile);
    expanded
        .iter()
        .find_map(named_define_name)
        .or_else(|| expanded.first().and_then(positional_define_name))
}

/// Returns the `constant_name:` name of one argument, including from a literal assoc unpack.
fn named_define_name(arg: &Expr) -> Option<String> {
    match &arg.kind {
        ExprKind::NamedArg { name, value } if name == "constant_name" => string_literal(value),
        _ => None,
    }
}

/// Returns the first positional `define()` argument, expanding one literal unpack.
fn positional_define_name(arg: &Expr) -> Option<String> {
    match &arg.kind {
        // Dynamic spreads stay opaque; a plain indexed literal keeps its first value.
        ExprKind::Spread(inner) => match &inner.kind {
            ExprKind::ArrayLiteral(items) => items.first().and_then(string_literal),
            _ => None,
        },
        _ => string_literal(arg),
    }
}

/// Returns one string literal's text, or `None` for any other expression.
fn string_literal(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        _ => None,
    }
}

/// Everything the define scan learns about one program.
#[derive(Default)]
struct DefineFacts {
    /// Literal `define("NAME", …)` names found anywhere in the program.
    names: HashSet<String>,
    /// Whether any body calls `eval()`, which can define a constant after schema creation.
    contains_eval: bool,
    /// Semantic key policy used to classify literal unpack arguments.
    semantic_profile: elephc_semantics::SemanticProfile,
}

/// Collects literal `define("NAME", …)` names and whether the program calls `eval()`.
///
/// The checker does not evaluate runtime `define()` calls, but a property default may legally
/// refer to one. Walking every statement body here is deliberately conservative: an extra name
/// only means the default is left to normal lowering instead of being reported as undefined.
/// `eval()` is the other runtime source of constants, so its presence has to be recorded in the
/// same walk.
fn collect_defines(program: &Program, defines: &mut DefineFacts) {
    for statement in program {
        collect_defines_stmt(statement, defines);
    }
}

/// Walks one statement, its conditions and every nested statement body for literal `define()` calls.
fn collect_defines_stmt(statement: &Stmt, defines: &mut DefineFacts) {
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
        | ExternGlobalDecl { .. } | IncludeOnceMark { .. } | Goto(_) | GotoLabel(_) => {}
    }
}

/// Records a literal `define("NAME", …)` call found anywhere in one expression tree.
fn collect_defines_expr(expr: &Expr, defines: &mut DefineFacts) {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            let key = crate::names::php_symbol_key(name.as_str().trim_start_matches('\\'));
            if key == "define" {
                if let Some(name) = literal_define_name(args, defines.semantic_profile) {
                    defines.names.insert(name.trim_start_matches('\\').to_string());
                }
            }
            // `eval()` can define a constant that no source `define()` declares. Record the
            // capability so an unresolved default read is left to the eval-aware runtime lookup
            // instead of being replaced with a compile-time error.
            defines.contains_eval |= key == "eval";
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
        if constant_resolves(name, checker, globals, defines) {
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

/// Returns whether one constant name resolves to a declaration, `define()` or builtin value.
fn constant_resolves(
    name: &str, checker: &Checker, globals: &HashMap<String, Expr>, defines: &HashSet<String>,
) -> bool {
    globals.contains_key(name)
        || defines.contains(name)
        || checker.constants.contains_key(name)
        || elephc_builtin_contract::lookup_constant(name).is_some()
}

/// Turns every constant name that is still unknown into the lazy error PHP raises on evaluation.
///
/// Replacing the reference instead of failing the class keeps lazy branches lazy: the throw sits
/// in the branch that references the name, so a condition that never selects it never fails.
fn replace_unresolved_constants(
    expr: &mut Expr, checker: &Checker, globals: &HashMap<String, Expr>, defines: &HashSet<String>,
    depth: usize,
) {
    if depth > 64 { return; }
    if let ExprKind::ConstRef(name) = &expr.kind {
        let raw = name.as_str().trim_start_matches('\\');
        if !constant_resolves(raw, checker, globals, defines) {
            *expr = super::classes::deferred_default_error(
                format!("Undefined constant \"{raw}\""),
                expr.span,
            );
        }
        return;
    }
    let mut visit = |value: &mut Expr| {
        replace_unresolved_constants(value, checker, globals, defines, depth + 1)
    };
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
}

/// Binds canonical source constants into a property default and folds literal branches.
pub(super) fn bind_constants(
    expr: &mut Expr, owner: &str, checker: &Checker, globals: &HashMap<String, Expr>, depth: usize,
) {
    bind_constant_reads(expr, owner, checker, globals, depth, true);
}

/// Resolves constant reads, optionally preserving their enclosing operations for strict checks.
pub(super) fn bind_constant_reads(
    expr: &mut Expr, owner: &str, checker: &Checker, globals: &HashMap<String, Expr>,
    depth: usize, fold: bool,
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
                    .map(|kind| expr.with_kind(kind))
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
        bind_constant_reads(&mut value, &scope, checker, globals, depth + 1, fold);
        value.span = expr.span;
        *expr = value;
        return;
    }
    let mut visit = |value: &mut Expr| bind_constant_reads(value, owner, checker, globals, depth + 1, fold);
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
        ExprKind::UnaryPlus(value) | ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::NamedArg { value, .. } | ExprKind::Spread(value) | ExprKind::Cast { expr: value, .. } => visit(value),
        _ => {}
    }
    if fold {
        *expr = crate::optimize::fold_constant_expression(expr.clone());
        prune_literal_branches(expr);
    }
}
