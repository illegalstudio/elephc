//! Purpose:
//! Moves a static property default that names a class the compiled world does not contain out of
//! the program prologue and into a run-once initializer evaluated at the property's first use.
//!
//! Called from:
//! - `crate::pipeline::compile()` after the autoload pass and before type checking.
//!
//! Key details:
//! - php evaluates a class's constant expressions when the class is first used, so
//!   `protected static array $formats = [\IntlDateFormatter::NONE, ...]` costs nothing until a
//!   method touches `$formats` -- and works when `IntlDateFormatter` is a polyfill stub that only
//!   exists at run time. The AOT prologue wrote every static default before the first statement,
//!   which cannot evaluate a constant of a class it has never seen, so the declaring file was
//!   handed back to the run-time autoloader.
//! - A default qualifies only when it names a class constant on a class that is neither declared
//!   anywhere in the program nor modelled by the compiler. Every other default keeps its
//!   prologue write.
//! - The default is replaced by a public static method `__elephc_static_init_<property>` that
//!   guards itself with a `static` flag and assigns the property. `crate::ir_lower` emits a call
//!   to it before every access of the property
//!   (`ir_lower::stmt::static_property_helpers::static_property_ref_data`), and the reachability
//!   pass seeds the method as a root so an unreferenced-looking initializer is never pruned.
//! - The initializer is built as AST, never parsed from PHP text.

use crate::fast_hash::FastSet as HashSet;

use crate::names::php_symbol_key;
use crate::parser::ast::{
    visit, ClassMethod, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, TypeExpr,
};
use crate::synthetic_class::{
    e_bool, e_var, method, s_assign, s_if, s_return_void, s_self_static_prop_assign, s_static,
};

/// Prefix of the generated per-property initializer methods.
pub(crate) const STATIC_INIT_PREFIX: &str = "__elephc_static_init_";

/// Returns the initializer method name for one static property.
pub(crate) fn static_init_method_name(property: &str) -> String {
    format!("{}{}", STATIC_INIT_PREFIX, php_symbol_key(property))
}

/// Rewrites every qualifying static default in `program`.
pub fn apply(mut program: Program) -> Program {
    let mut declared: HashSet<String> = HashSet::default();
    visit::walk_stmts(&program, &mut |stmt| match &stmt.kind {
        StmtKind::ClassDecl { name, .. }
        | StmtKind::EnumDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::PackedClassDecl { name, .. } => {
            declared.insert(class_key(name));
        }
        _ => {}
    });
    visit::walk_nodes_mut(&mut program, &mut |node| {
        let visit::NodeMut::Stmt(stmt) = node else {
            return;
        };
        let StmtKind::ClassDecl {
            properties,
            methods,
            ..
        } = &mut stmt.kind
        else {
            return;
        };
        for property in properties.iter_mut() {
            if !property.is_static {
                continue;
            }
            let Some(default) = &property.default else {
                continue;
            };
            if !names_unknown_class_constant(default, &declared) {
                continue;
            }
            let default = property.default.take().expect("default checked above");
            methods.push(initializer(&property.name, default));
        }
    });
    program
}

/// Normalizes a class name for the declared-name set.
fn class_key(name: &str) -> String {
    php_symbol_key(name.trim_start_matches('\\'))
}

/// Builds `public static function __elephc_static_init_<p>(): void` for one property.
fn initializer(property: &str, default: Expr) -> ClassMethod {
    method(&static_init_method_name(property))
        .static_()
        .returns(TypeExpr::Void)
        .body(vec![
            s_static("done", e_bool(false)),
            s_if(e_var("done"), vec![s_return_void()], Vec::new(), None),
            s_assign("done", e_bool(true)),
            s_self_static_prop_assign(property, default),
        ])
        .build()
}

/// Returns whether a constant expression reads a class constant of a class nobody declares.
fn names_unknown_class_constant(expr: &Expr, declared: &HashSet<String>) -> bool {
    let probe = [Stmt::new(StmtKind::Echo(expr.clone()), expr.span)];
    let mut found = false;
    visit::walk_nodes(&probe, &mut |node| {
        let visit::Node::Expr(inner) = node else {
            return;
        };
        let ExprKind::ScopedConstantAccess {
            receiver: StaticReceiver::Named(name),
            ..
        } = &inner.kind
        else {
            return;
        };
        let key = class_key(name.as_str());
        if !declared.contains(&key)
            && crate::types::builtin_classes::lookup_profile_class(&key).is_none() {
            found = true;
        }
    });
    found
}
