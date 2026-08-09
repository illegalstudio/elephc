//! Purpose:
//! Selects every class descriptor required by generated code and dynamic runtime factories.
//!
//! Called from:
//! - "crate::codegen_support::runtime_features" while deriving runtime requirements.
//!
//! Key details:
//! - Expands inheritance, implementation, method-body, reflection, and internal factory dependencies to a fixed point.

use super::program_usage::{collect_required_class_names, collect_required_class_names_in_stmts};
use super::{dynamic_new, reflection};
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::ClassInfo;
use std::collections::{HashMap, HashSet};

/// Returns the class names a program's emitted metadata can reach. Starts from required
/// classes, adds the throwable hierarchy a runtime helper can materialize, reflection
/// classes, and attribute factories, then expands to cover the full inheritance and
/// implementation dependency chain.
///
/// WHAT THIS DOES NOT DO, despite what its name suggests: decide what gets emitted. Its two
/// callers both live in `runtime_features`, which asks whether the reachable class set
/// contains a regex iterator or a PHAR helper — that is, whether to link pcre2 / the phar
/// bridge. The set that actually drives class emission is
/// `codegen::runtime_metadata::classes::runtime_referenced_class_names`, and that is where a
/// narrowing has to go: gating the SPL-thrown exceptions here alone moved a trivial program's
/// assembly by exactly nothing (7,990 lines before, 7,990 after). The lists are kept in step
/// so the two never disagree about what a helper can throw, not because both are load-bearing.
pub(super) fn collect_emitted_class_names(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> HashSet<String> {
    let mut names = collect_required_class_names(program);
    if names.contains("Fiber") {
        names.insert("FiberError".to_string());
    }
    // The throwables a runtime helper can materialize with no EIR class reference to hang
    // the id off: `json_encode`/`json_decode`/`json_validate` raise JsonException through
    // JSON_THROW_ON_ERROR, and `intdiv($a, 0)` / `$a % 0` raise DivisionByZeroError from a
    // codegen guard. User code that only catches a wider type (`catch (Exception $e)`) still
    // needs every ancestor present, because the catch-time walk in `__rt_exception_matches`
    // climbs `_class_parent_ids` from the THROWN class.
    //
    // The membership test is the class-id symbol table in `runtime::data::user`: a helper
    // stamps `[obj+0]` from a `_*_class_id` symbol, so a throwable without one has no
    // unreferenced producer. That rules out ArgumentCountError (elephc rejects a bad builtin
    // arity at compile time, where reference PHP throws at runtime), AssertionError
    // (`assert()` is not implemented) and UnhandledMatchError (an unmatched `match` ends in
    // `Terminator::Fatal`, not a throw — a real gap against reference PHP, and closing it
    // will give the class the EIR reference it currently lacks).
    //
    // WHAT THIS COSTS, so the next person to weigh it has both halves. For `<?php echo 1;`
    // the sixteen originally seeded classes were 2,643 lines of a 7,990-line assembly — 33%
    // of everything emitted, and 81% of all class metadata — at roughly 215 lines each.
    // Assembling and linking are 81% of that program's compile, so the seeding is not free.
    //
    // One narrowing was considered and rejected: KEEP THE DESCRIPTORS, DROP THE CONSTRUCTION
    // MACHINERY. The single largest item per class is `_eir__class_propinit_<id>_entry_0` at
    // ~50 lines, and a class that is only ever caught never runs it. Tempting, and the
    // metadata layer already tolerates an absent thunk (`runtime_class_infos` nulls the
    // defaults for one). But `catch (Exception $e) { echo $e->getMessage(); }` dispatches on
    // the THROWN class, not the caught type, so the vtable and callable-method tables stay
    // live for a class the source never names — and the same argument reaches the var_dump
    // and JSON descriptors through `var_dump($e)` and `json_encode($e)`.
    //
    // Getting any of this wrong used to fail silently, at catch time, in a program that
    // looked unrelated. It no longer can: a class id whose metadata was never emitted now
    // reads back as `-2` and `__rt_exception_matches` aborts instead of reporting no match.
    for builtin in [
        "Throwable",
        "Error",
        "TypeError",
        "ValueError",
        "ArithmeticError",
        "DivisionByZeroError",
        "Exception",
        "JsonException",
    ] {
        names.insert(builtin.to_string());
    }
    // These five are thrown BY ID from SPL container helpers only — `runtime/spl/
    // doubly_linked_list.rs`, `runtime/spl/fixed_array.rs` and
    // `lower_inst/objects/iterator_iterator.rs` are the sole readers of their
    // `_spl_*_class_id` symbols — so a program with no SPL surface has no unreferenced
    // path to them. Any other route already covers itself: user code that names one is
    // in `collect_required_class_names`, and `expand_emitted_class_dependencies` pulls
    // every ancestor to a fixed point. RuntimeException used to survive this gate regardless
    // because `JsonException extends RuntimeException` — elephc's own invention, since reference
    // PHP puts JsonException directly under Exception. With that corrected it survives only when
    // the SPL surface is present, which is exactly when a helper can throw it.
    //
    // Getting this wrong is no longer silent: the walk meets `-2` and aborts.
    if spl_surface_is_present(classes) {
        for builtin in [
            "LogicException",
            "RuntimeException",
            "InvalidArgumentException",
            "OutOfBoundsException",
            "OutOfRangeException",
        ] {
            names.insert(builtin.to_string());
        }
    }
    // Likewise ReflectionException, whose unreferenced throwers are all inside the
    // Reflection surface that `types::checker::builtin_types::reflection::gate` decides.
    if classes.contains_key("ReflectionClass") {
        names.insert("ReflectionException".to_string());
    }
    for builtin in [
        "ReflectionAttribute",
        "ReflectionClass",
        "ReflectionMethod",
        "ReflectionProperty",
    ] {
        names.insert(builtin.to_string());
    }
    for factory in reflection::collect_attribute_factories(classes) {
        // Only resolvable attribute classes are emitted; non-class attributes
        // are registered solely so `getArguments()` can return their arguments.
        if factory.resolvable {
            names.insert(factory.class_name);
        }
    }
    collect_dynamic_object_factory_classes(program, classes, &mut names);
    expand_emitted_class_dependencies(&mut names, classes);
    names
}

/// Whether any SPL container that throws one of the SPL exception ids is registered.
///
/// One representative per throwing helper rather than the whole family: SPL registration is
/// all-or-nothing (`types::checker::builtin_spl_classes::program_may_reference_spl` decides it
/// for the entire surface at once), so a single hit means the family is there. The three names
/// are the containers whose runtime helpers actually read `_spl_*_class_id`.
fn spl_surface_is_present(classes: &HashMap<String, ClassInfo>) -> bool {
    ["SplDoublyLinkedList", "SplFixedArray", "IteratorIterator"]
        .iter()
        .any(|name| classes.contains_key(*name))
}

/// Repeatedly expands `names` by adding parent classes and all
/// method-implementation classes (both instance and static) until a
/// fixed point is reached, ensuring emitted vtables and interface
/// tables are complete.
fn expand_emitted_class_dependencies(
    names: &mut HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
) {
    loop {
        let mut changed = false;
        let snapshot: Vec<String> = names.iter().cloned().collect();
        for class_name in snapshot {
            let Some(class_info) = classes.get(&class_name) else {
                continue;
            };
            if let Some(parent) = &class_info.parent {
                changed |= names.insert(parent.clone());
            }
            for impl_class in class_info
                .method_impl_classes
                .values()
                .chain(class_info.static_method_impl_classes.values())
            {
                changed |= names.insert(impl_class.clone());
            }
            let previous_len = names.len();
            for method in &class_info.method_decls {
                collect_dynamic_object_factory_classes(&method.body, classes, names);
                collect_required_class_names_in_stmts(&method.body, names);
            }
            changed |= names.len() != previous_len;
        }
        if !changed {
            break;
        }
    }
}

/// Adds every concrete class that an internal dynamic object factory can instantiate.
fn collect_dynamic_object_factory_classes(
    stmts: &[Stmt],
    classes: &HashMap<String, ClassInfo>,
    names: &mut HashSet<String>,
) {
    for stmt in stmts {
        collect_dynamic_object_factory_classes_in_stmt(stmt, classes, names);
    }
}

/// Adds dynamic factory class dependencies found in a statement.
fn collect_dynamic_object_factory_classes_in_stmt(
    stmt: &Stmt,
    classes: &HashMap<String, ClassInfo>,
    names: &mut HashSet<String>,
) {
    match &stmt.kind {
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => methods.iter().for_each(|method| {
            collect_dynamic_object_factory_classes(&method.body, classes, names)
        }),
        StmtKind::FunctionDecl { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            collect_dynamic_object_factory_classes(body, classes, names);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_dynamic_object_factory_classes(try_body, classes, names);
            for catch in catches {
                collect_dynamic_object_factory_classes(&catch.body, classes, names);
            }
            if let Some(finally_body) = finally_body {
                collect_dynamic_object_factory_classes(finally_body, classes, names);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_dynamic_object_factory_classes(then_body, classes, names);
            if let Some(else_body) = else_body {
                collect_dynamic_object_factory_classes(else_body, classes, names);
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
            collect_dynamic_object_factory_classes(then_body, classes, names);
            for (condition, body) in elseif_clauses {
                collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
                collect_dynamic_object_factory_classes(body, classes, names);
            }
            if let Some(else_body) = else_body {
                collect_dynamic_object_factory_classes(else_body, classes, names);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
            collect_dynamic_object_factory_classes(body, classes, names);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_dynamic_object_factory_classes_in_stmt(init, classes, names);
            }
            if let Some(condition) = condition {
                collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
            }
            if let Some(update) = update {
                collect_dynamic_object_factory_classes_in_stmt(update, classes, names);
            }
            collect_dynamic_object_factory_classes(body, classes, names);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_dynamic_object_factory_classes_in_expr(array, classes, names);
            collect_dynamic_object_factory_classes(body, classes, names);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_dynamic_object_factory_classes_in_expr(subject, classes, names);
            for (patterns, body) in cases {
                for pattern in patterns {
                    collect_dynamic_object_factory_classes_in_expr(pattern, classes, names);
                }
                collect_dynamic_object_factory_classes(body, classes, names);
            }
            if let Some(default) = default {
                collect_dynamic_object_factory_classes(default, classes, names);
            }
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::PropertyAssign { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => {
            collect_dynamic_object_factory_classes_in_expr(expr, classes, names);
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_dynamic_object_factory_classes_in_expr(index, classes, names);
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_dynamic_object_factory_classes_in_expr(target, classes, names);
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
        }
        _ => {}
    }
}

/// Adds dynamic factory class dependencies found in an expression.
fn collect_dynamic_object_factory_classes_in_expr(
    expr: &Expr,
    classes: &HashMap<String, ClassInfo>,
    names: &mut HashSet<String>,
) {
    match &expr.kind {
        ExprKind::NewDynamicObject {
            class_name,
            required_parent,
            args,
            ..
        } => {
            collect_dynamic_factory_descendants(required_parent.as_str(), classes, names);
            collect_dynamic_object_factory_classes_in_expr(class_name, classes, names);
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_dynamic_object_factory_classes_in_expr(left, classes, names);
            collect_dynamic_object_factory_classes_in_expr(right, classes, names);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            if let crate::parser::ast::InstanceOfTarget::Expr(expr) = target {
                collect_dynamic_object_factory_classes_in_expr(expr, classes, names);
            }
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. } => {
            collect_dynamic_object_factory_classes_in_expr(expr, classes, names);
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            collect_dynamic_object_factory_classes_in_expr(default, classes, names);
        }
        ExprKind::Pipe { value, callable } => {
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            collect_dynamic_object_factory_classes_in_expr(callable, classes, names);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            collect_dynamic_object_factory_classes(prelude, classes, names);
            collect_dynamic_object_factory_classes_in_expr(target, classes, names);
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            if let Some(result_target) = result_target {
                collect_dynamic_object_factory_classes_in_expr(result_target, classes, names);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            for class_name in dynamic_new::supported_dynamic_new_builtin_class_names() {
                if classes.contains_key(*class_name) {
                    names.insert((*class_name).to_string());
                }
            }
            collect_dynamic_object_factory_classes_in_expr(name_expr, classes, names);
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_dynamic_object_factory_classes_in_expr(callee, classes, names);
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_dynamic_object_factory_classes_in_expr(item, classes, names);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_dynamic_object_factory_classes_in_expr(key, classes, names);
                collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_dynamic_object_factory_classes_in_expr(subject, classes, names);
            for (patterns, value) in arms {
                for pattern in patterns {
                    collect_dynamic_object_factory_classes_in_expr(pattern, classes, names);
                }
                collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            }
            if let Some(default) = default {
                collect_dynamic_object_factory_classes_in_expr(default, classes, names);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_dynamic_object_factory_classes_in_expr(array, classes, names);
            collect_dynamic_object_factory_classes_in_expr(index, classes, names);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
            collect_dynamic_object_factory_classes_in_expr(then_expr, classes, names);
            collect_dynamic_object_factory_classes_in_expr(else_expr, classes, names);
        }
        ExprKind::Closure { body, .. } => {
            collect_dynamic_object_factory_classes(body, classes, names)
        }
        ExprKind::NamedArg { value, .. } => {
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_dynamic_object_factory_classes_in_expr(object, classes, names);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_dynamic_object_factory_classes_in_expr(object, classes, names);
            collect_dynamic_object_factory_classes_in_expr(property, classes, names);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_dynamic_object_factory_classes_in_expr(object, classes, names);
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method {
            object, ..
        }) => collect_dynamic_object_factory_classes_in_expr(object, classes, names),
        ExprKind::BufferNew { len, .. } => {
            collect_dynamic_object_factory_classes_in_expr(len, classes, names);
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_dynamic_object_factory_classes_in_expr(key, classes, names);
            }
            if let Some(value) = value {
                collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            }
        }
        ExprKind::YieldFrom(inner) => {
            collect_dynamic_object_factory_classes_in_expr(inner, classes, names);
        }
        _ => {}
    }
}

/// Adds every known class that can satisfy an internal dynamic factory parent constraint.
fn collect_dynamic_factory_descendants(
    required_parent: &str,
    classes: &HashMap<String, ClassInfo>,
    names: &mut HashSet<String>,
) {
    for class_name in classes.keys() {
        if emitted_class_descends_from(class_name, required_parent, classes) {
            names.insert(class_name.clone());
        }
    }
}

/// Returns true if `class_name` is the required class or extends it.
fn emitted_class_descends_from(
    class_name: &str,
    required_parent: &str,
    classes: &HashMap<String, ClassInfo>,
) -> bool {
    let mut current = Some(class_name);
    while let Some(name) = current {
        if crate::names::php_symbol_key(name.trim_start_matches('\\'))
            == crate::names::php_symbol_key(required_parent.trim_start_matches('\\'))
        {
            return true;
        }
        current = classes.get(name).and_then(|info| info.parent.as_deref());
    }
    false
}
