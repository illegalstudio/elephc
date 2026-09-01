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
use crate::parser::ast::{Stmt, StmtKind};
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
    function_declared_returns: &mut HashSet<String>,
    static_method_declared_returns: &mut HashSet<String>,
    instance_method_declared_returns: &mut HashSet<String>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                body,
                return_type,
                ..
            } => {
                functions.insert(name.clone(), body);
                if return_type.is_some() {
                    function_declared_returns.insert(php_symbol_key(name));
                }
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
                    let class_key = php_symbol_key(name);
                    let method_key = method_effect_key(name, &method.name);
                    let entry = (&method.body[..], class_key);
                    if method.is_static {
                        static_methods.insert(method_key.clone(), entry);
                        if method.return_type.is_some() {
                            static_method_declared_returns.insert(method_key);
                        }
                    } else {
                        instance_methods.insert(method_key.clone(), entry);
                        if method.return_type.is_some() {
                            instance_method_declared_returns.insert(method_key);
                        }
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_exception_bodies(
                body,
                functions,
                static_methods,
                instance_methods,
                class_contexts,
                function_declared_returns,
                static_method_declared_returns,
                instance_method_declared_returns,
            ),
            _ => {}
        }
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
