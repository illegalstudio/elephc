//! Purpose:
//! Collects user callable bodies and their lexical class contexts for exception summaries.
//! Adapts free functions and methods into one fixed-point analysis input shape.
//!
//! Called from:
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis::from_program()`
//!
//! Key details:
//! - Method keys match the optimizer's shared effect-analysis naming convention.
//! - Only executable class method bodies participate; declarations themselves do not throw.

use crate::names::php_symbol_key;
use crate::optimize::effect_analysis::method_effect_key;
use crate::parser::ast::{Stmt, StmtKind, TypeExpr};
use std::collections::{HashMap, HashSet};

/// Lexical class context needed to resolve self/parent/static throw and call forms.
#[derive(Clone, Debug)]
pub(super) struct ExceptionClassContext {
    pub(super) class_name: String,
    pub(super) parent_name: Option<String>,
}

/// A callable body plus optional lexical class context.
#[derive(Clone, Copy)]
pub(super) struct ExceptionBody<'a> {
    pub(super) body: &'a [Stmt],
    pub(super) class_context: Option<&'a ExceptionClassContext>,
}

/// Collects callable bodies and per-class lexical contexts from the AST.
pub(super) fn collect_exception_bodies<'a>(
    stmts: &'a [Stmt],
    functions: &mut HashMap<String, &'a [Stmt]>,
    static_methods: &mut HashMap<String, (&'a [Stmt], String)>,
    instance_methods: &mut HashMap<String, (&'a [Stmt], String)>,
    class_contexts: &mut HashMap<String, ExceptionClassContext>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                functions.insert(name.clone(), body);
            }
            StmtKind::ClassDecl {
                name,
                extends,
                methods,
                ..
            } => {
                class_contexts.insert(
                    php_symbol_key(name),
                    ExceptionClassContext {
                        class_name: name.clone(),
                        parent_name: extends.as_ref().map(|name| name.as_str().to_string()),
                    },
                );
                for method in methods.iter().filter(|method| method.has_body) {
                    let entry = (&method.body[..], php_symbol_key(name));
                    if method.is_static {
                        static_methods.insert(method_effect_key(name, &method.name), entry);
                    } else {
                        instance_methods.insert(method_effect_key(name, &method.name), entry);
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_exception_bodies(
                body,
                functions,
                static_methods,
                instance_methods,
                class_contexts,
            ),
            _ => {}
        }
    }
}

/// Collects declaration-level entry checks when checker metadata is unavailable to the pass.
pub(super) fn collect_object_reference_entry_checks(
    stmts: &[Stmt],
    functions: &mut HashSet<String>,
    static_methods: &mut HashMap<String, bool>,
    instance_methods: &mut HashMap<String, bool>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, params, .. } => {
                if params_have_object_reference(params) {
                    functions.insert(name.clone());
                }
            }
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. } => {
                for method in methods.iter().filter(|method| params_have_object_reference(&method.params)) {
                    let key = method_effect_key(name, &method.name);
                    if method.is_static {
                        static_methods.insert(key, true);
                    } else {
                        instance_methods.insert(key, true);
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_object_reference_entry_checks(
                body, functions, static_methods, instance_methods,
            ),
            _ => {}
        }
    }
}

/// Finds parameter hints that can reject a `Class|false` / `Class|null` argument.
///
/// A class-typed parameter runs the union guard at the call boundary whether it is by-value
/// or by-reference, and a literal `false` parameter always checks its argument. Checker
/// metadata narrows this down; without it the declaration is the conservative source.
fn params_have_object_reference(
    params: &[(String, Option<TypeExpr>, Option<crate::parser::ast::Expr>, bool)],
) -> bool {
    params
        .iter()
        .any(|(_, hint, _, _)| hint.as_ref().is_some_and(type_hint_is_class))
}

/// Returns whether a declared parameter hint names a class or interface type.
fn type_hint_is_class(hint: &TypeExpr) -> bool {
    match hint {
        TypeExpr::False | TypeExpr::GenericClass { .. } | TypeExpr::Intersection(_) => true,
        TypeExpr::Named(name) => !matches!(
            name.as_str()
                .trim_start_matches('\\')
                .to_ascii_lowercase()
                .as_str(),
            "mixed" | "callable" | "closure" | "array" | "object" | "iterable"
                | "string" | "int" | "float" | "bool" | "null" | "true" | "static"
                | "self" | "parent" | "void" | "never"
        ),
        TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
            type_hint_is_class(inner)
        }
        TypeExpr::Union(members) => members.iter().any(type_hint_is_class),
        _ => false,
    }
}

/// Attaches stable lexical class references to raw callable body collections.
pub(super) fn attach_class_contexts<'a, V>(
    bodies: HashMap<String, V>,
    class_contexts: &'a HashMap<String, ExceptionClassContext>,
) -> HashMap<String, ExceptionBody<'a>>
where
    V: IntoExceptionBody<'a>,
{
    bodies
        .into_iter()
        .map(|(name, body)| (name, body.into_exception_body(class_contexts)))
        .collect()
}

/// Converts raw collected bodies into analysis bodies with optional class context.
pub(super) trait IntoExceptionBody<'a> {
    /// Attaches the appropriate lexical class context to this raw body.
    fn into_exception_body(
        self,
        class_contexts: &'a HashMap<String, ExceptionClassContext>,
    ) -> ExceptionBody<'a>;
}

impl<'a> IntoExceptionBody<'a> for &'a [Stmt] {
    /// Converts a free-function body with no class context.
    fn into_exception_body(
        self,
        _class_contexts: &'a HashMap<String, ExceptionClassContext>,
    ) -> ExceptionBody<'a> {
        ExceptionBody {
            body: self,
            class_context: None,
        }
    }
}

impl<'a> IntoExceptionBody<'a> for (&'a [Stmt], String) {
    /// Converts a method body and resolves its owning class context.
    fn into_exception_body(
        self,
        class_contexts: &'a HashMap<String, ExceptionClassContext>,
    ) -> ExceptionBody<'a> {
        ExceptionBody {
            body: self.0,
            class_context: class_contexts.get(&self.1),
        }
    }
}
