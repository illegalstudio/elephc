//! Purpose:
//! Routes the magic property accessors a program subclass of a hosted extension class declares
//! (`__get`, `__set`, `__isset`, `__unset`) as PHP does: the engine answers every access to a
//! property it declares and the caller's scope reaches, or that the object holds, and only the
//! rest reach the program's accessor.
//!
//! Called from:
//! - `crate::pipeline`, once autoloading has put every class in the AST with resolved names,
//!   when the project hosts extensions.
//!
//! Key details:
//! - A wrapper class (`crate::php_ext::prelude`) answers every access through its own accessors,
//!   since the extension's properties live on the engine object. A subclass's override replaces
//!   them, so each one gets a prologue asking the engine first (`__elephc_php_ext_direct`) and,
//!   when the engine answers, running the access there.
//! - The prologue takes the caller's scope the call site published, the first thing the
//!   override runs, exactly as the wrapper's own accessors do.
//! - Only methods declared in a class body are rewritten; an accessor a trait supplies keeps
//!   receiving every access.

use std::collections::HashMap;

use crate::names::php_symbol_key;
use crate::parser::ast::{CastType, ClassMethod, Program, Stmt, StmtKind};
use crate::synthetic_class::{e_call, e_cast, e_int, e_method_call, e_null, e_this, e_var, s_assign, s_expr, s_if, s_return, s_return_void};

use super::prelude::{DIRECT_FUNCTION, OBJECT_INTERFACE, OBJECT_METHOD, PROPERTY_FUNCTION};

/// The local the prologue keeps the caller's scope in.
const SCOPE_LOCAL: &str = "__elephc_scope";

/// Rewrites the magic accessors of every program class extending a hosted extension class.
pub fn route_program_accessors(mut program: Program) -> Program {
    let mut classes = HashMap::new();
    collect_classes(&program, &mut classes);
    let hosted_subclass = |name: &str| {
        let mut current = classes.get(&php_symbol_key(name));
        let mut steps = 0;
        // A wrapper implements the interface itself; a program subclass inherits it.
        let own = current.is_some_and(|(_, wraps)| *wraps);
        while let Some((parent, _)) = current {
            let Some(parent) = parent else { return false };
            let entry = classes.get(parent);
            if entry.is_some_and(|(_, wraps)| *wraps) {
                return !own;
            }
            current = entry;
            steps += 1;
            if steps > classes.len() {
                return false;
            }
        }
        false
    };
    rewrite(&mut program, &hosted_subclass);
    program
}

/// Records each class's parent and whether it is a wrapper, by case-insensitive name, descending
/// into the wrappers an included or conditionally declared file leaves around a class.
fn collect_classes(statements: &[Stmt], classes: &mut HashMap<String, (Option<String>, bool)>) {
    for statement in statements {
        match &statement.kind {
            StmtKind::ClassDecl { name, extends, implements, .. } => {
                let wraps = implements.iter().any(|interface| interface.as_canonical().trim_start_matches('\\') == OBJECT_INTERFACE);
                let parent = extends.as_ref().map(|parent| php_symbol_key(parent.as_canonical().trim_start_matches('\\')));
                classes.insert(php_symbol_key(name.trim_start_matches('\\')), (parent, wraps));
            }
            _ => for_each_body(statement, |body| collect_classes(body, classes)),
        }
    }
}

/// Rewrites the accessors of the hosted subclasses among `statements`.
fn rewrite(statements: &mut [Stmt], hosted_subclass: &dyn Fn(&str) -> bool) {
    for statement in statements {
        match &mut statement.kind {
            StmtKind::ClassDecl { name, methods, .. } => {
                if hosted_subclass(name) {
                    for method in methods.iter_mut() {
                        route_accessor(method);
                    }
                }
            }
            _ => for_each_body_mut(statement, |body| rewrite(body, hosted_subclass)),
        }
    }
}

/// The statement lists a class declaration can sit in.
fn for_each_body(statement: &Stmt, mut visit: impl FnMut(&[Stmt])) {
    match &statement.kind {
        StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } | StmtKind::NamespaceBlock { body, .. } => visit(body),
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            visit(then_body);
            for (_, body) in elseif_clauses {
                visit(body);
            }
            if let Some(body) = else_body {
                visit(body);
            }
        }
        _ => {}
    }
}

/// `for_each_body`, mutably.
fn for_each_body_mut(statement: &mut Stmt, mut visit: impl FnMut(&mut [Stmt])) {
    match &mut statement.kind {
        StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } | StmtKind::NamespaceBlock { body, .. } => visit(body),
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            visit(then_body);
            for (_, body) in elseif_clauses {
                visit(body);
            }
            if let Some(body) = else_body {
                visit(body);
            }
        }
        _ => {}
    }
}

/// Prepends the engine-first prologue to one magic accessor:
///
/// ```php
/// $__elephc_scope = elephc_php_ext_take_scope();
/// if (__elephc_php_ext_direct($this->__elephcObject(), (string) $name, get_class($this), $__elephc_scope)) {
///     return __elephc_php_ext_property($this->__elephcObject(), (string) $name, OP, $value, get_class($this), $__elephc_scope);
/// }
/// ```
fn route_accessor(method: &mut ClassMethod) {
    let op = match php_symbol_key(&method.name).as_str() {
        "__get" => 0,
        "__set" => 1,
        "__isset" => 2,
        "__unset" => 3,
        _ => return,
    };
    let arity = if op == 1 { 2 } else { 1 };
    if method.is_static || !method.has_body || method.params.len() < arity {
        return;
    }
    let name = e_cast(CastType::String, e_var(&method.params[0].0));
    let value = if op == 1 { e_var(&method.params[1].0) } else { e_null() };
    let object = || e_method_call(e_this(), OBJECT_METHOD, vec![]);
    let class = || e_call("\\get_class", vec![e_this()]);
    let access = e_call(
        &format!("\\{PROPERTY_FUNCTION}"),
        vec![object(), name.clone(), e_int(op), value, class(), e_var(SCOPE_LOCAL)],
    );
    let answer = match op {
        0 => vec![s_return(access)],
        2 => vec![s_return(e_cast(CastType::Bool, access))],
        _ => vec![s_expr(access), s_return_void()],
    };
    let direct = e_call(&format!("\\{DIRECT_FUNCTION}"), vec![object(), name, class(), e_var(SCOPE_LOCAL)]);
    let prologue = vec![
        s_assign(SCOPE_LOCAL, e_call("\\elephc_php_ext_take_scope", vec![])),
        s_if(direct, answer, vec![], None),
    ];
    method.body.splice(0..0, prologue);
}
