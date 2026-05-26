//! Purpose:
//! Rewrites scope-dependent magic constants according to lexical PHP context.
//! Tracks namespaces, functions, classes, traits, and methods while walking one file's AST.
//!
//! Called from:
//! - `crate::magic_constants::substitute_scope_constants_in_file()`.
//!
//! Key details:
//! - Trait-origin `__CLASS__` uses a placeholder so concrete class binding can happen after trait flattening.

use std::path::Path;

use crate::names::Name;
use crate::parser::ast::{ExprKind, MagicConstant, Program};
use crate::span::Span;

use super::walker::{walk_program, Pass};
use super::{namespace_string, qualify, TRAIT_CLASS_PLACEHOLDER};

/// Rewrites scope-dependent magic constants (`__CLASS__`, `__TRAIT__`, `__FUNCTION__`,
/// `__METHOD__`, `__NAMESPACE__`) for a single PHP file using its canonical file path.
///
/// Canonicalizes the file path and delegates to `substitute_scope_constants_with_file`.
pub(super) fn substitute_scope_constants_in_file(program: Program, file_path: &Path) -> Program {
    let canonical = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    substitute_scope_constants_with_file(program, Some(canonical.display().to_string()))
}

/// Inner implementation that accepts an optional already-canonicalized file string.
fn substitute_scope_constants_with_file(program: Program, file: Option<String>) -> Program {
    let mut pass = ScopePass {
        scope: Scope::default(),
        file,
    };
    walk_program(program, &mut pass)
}

/// Tracks the current lexical scope while walking a PHP file's AST.
/// Each field uses `Option<String>` to represent "unknown" and stacks to support nesting.
#[derive(Default)]
struct Scope {
    namespace: Option<String>,
    namespace_stack: Vec<Option<String>>,
    class: Option<String>,
    class_stack: Vec<Option<String>>,
    trait_: Option<String>,
    trait_stack: Vec<Option<String>>,
    function: Option<String>,
    function_stack: Vec<Option<String>>,
    closure_depth: usize,
    closure_names: Vec<String>,
}

/// Holds the current scope state and the canonical file path being processed.
struct ScopePass {
    scope: Scope,
    file: Option<String>,
}

impl ScopePass {
    /// Returns the fully-qualified class name based on current namespace and class scope.
    fn fqn_class(&self) -> Option<String> {
        let class = self.scope.class.as_ref()?;
        Some(qualify(self.scope.namespace.as_deref(), class))
    }

    /// Returns the fully-qualified trait name based on current namespace and trait scope.
    fn fqn_trait(&self) -> Option<String> {
        let trait_name = self.scope.trait_.as_ref()?;
        Some(qualify(self.scope.namespace.as_deref(), trait_name))
    }

    /// Returns the fully-qualified function name. Inside a class or trait, returns
    /// the unqualified method name per PHP semantics; otherwise returns the namespace-qualified name.
    fn fqn_function(&self) -> Option<String> {
        let function = self.scope.function.as_ref()?;
        // For methods inside a class/trait, __FUNCTION__ is the unqualified
        // method name in PHP; for free functions it is namespace-qualified.
        if self.scope.class.is_some() || self.scope.trait_.is_some() {
            Some(function.clone())
        } else {
            Some(qualify(self.scope.namespace.as_deref(), function))
        }
    }

    /// Returns the FQN of the class or trait that owns the current closure context.
    fn method_owner_for_closure(&self) -> Option<String> {
        self.fqn_class().or_else(|| self.fqn_trait())
    }

    /// Constructs a closure name string using the parent function/method context and source span line number.
    /// The format is `{closure:<context>:<line>}` where context is derived from the nearest enclosing
    /// function, method, or file path.
    fn closure_name(&self, span: Span) -> String {
        let context = if let Some(parent_closure) = self.scope.closure_names.last() {
            parent_closure.clone()
        } else if let Some(function) = &self.scope.function {
            if let Some(owner) = self.method_owner_for_closure() {
                format!("{}::{}()", owner, function)
            } else {
                format!("{}()", qualify(self.scope.namespace.as_deref(), function))
            }
        } else {
            self.file.clone().unwrap_or_else(|| "unknown".to_string())
        };
        format!("{{closure:{}:{}}}", context, span.line)
    }
}

impl Pass for ScopePass {
    /// Transforms a magic constant into a string literal based on current lexical scope.
    ///
    /// - `__NAMESPACE__`: current namespace or empty string.
    /// - `__CLASS__`: FQN of current class; uses `TRAIT_CLASS_PLACEHOLDER` when in a trait but outside a class.
    /// - `__TRAIT__`: FQN of current trait or empty string.
    /// - `__FUNCTION__`: closure name if inside a closure, otherwise FQN (unqualified inside class/trait).
    /// - `__METHOD__`: closure name if inside a closure, otherwise `ClassName::methodName`.
    /// - `__FILE__` / `__DIR__`: empty string (handled by the file pass; reaching here means it was skipped).
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        let s = &self.scope;
        match mc {
            MagicConstant::Namespace => {
                ExprKind::StringLiteral(s.namespace.clone().unwrap_or_default())
            }
            MagicConstant::Class => {
                if s.class.is_none() && s.trait_.is_some() {
                    ExprKind::StringLiteral(TRAIT_CLASS_PLACEHOLDER.to_string())
                } else {
                    ExprKind::StringLiteral(self.fqn_class().unwrap_or_default())
                }
            }
            MagicConstant::Trait => {
                ExprKind::StringLiteral(self.fqn_trait().unwrap_or_default())
            }
            MagicConstant::Function => {
                let name = if let Some(closure_name) = s.closure_names.last() {
                    closure_name.clone()
                } else {
                    self.fqn_function().unwrap_or_default()
                };
                ExprKind::StringLiteral(name)
            }
            MagicConstant::Method => {
                if let Some(closure_name) = s.closure_names.last() {
                    ExprKind::StringLiteral(closure_name.clone())
                } else {
                    let name = match (self.fqn_class().or_else(|| self.fqn_trait()), &s.function) {
                        (Some(c), Some(f)) => format!("{}::{}", c, f),
                        (None, Some(f)) => qualify(s.namespace.as_deref(), f),
                        _ => String::new(),
                    };
                    ExprKind::StringLiteral(name)
                }
            }
            // File/Dir are handled by the file pass; if they reach here it
            // means the file pass was skipped — substitute to empty rather
            // than panic, since this is best-effort.
            MagicConstant::File | MagicConstant::Dir => ExprKind::StringLiteral(String::new()),
        }
    }

    /// Pushes the current namespace onto the stack and sets the new namespace declaration.
    fn enter_namespace_decl(&mut self, name: &Option<Name>) {
        self.scope.namespace = Some(namespace_string(name));
    }

    /// Saves the current namespace on the stack and enters a new namespace block.
    fn enter_namespace_block(&mut self, name: &Option<Name>) {
        self.scope.namespace_stack.push(self.scope.namespace.clone());
        self.scope.namespace = Some(namespace_string(name));
    }

    /// Restores the previous namespace from the stack after leaving a namespace block.
    fn leave_namespace_block(&mut self) {
        if let Some(prev) = self.scope.namespace_stack.pop() {
            self.scope.namespace = prev;
        }
    }

    /// Saves the current function on the stack and enters a new function scope.
    fn enter_function(&mut self, name: &str) {
        self.scope.function_stack.push(self.scope.function.clone());
        self.scope.function = Some(name.to_string());
    }

    /// Restores the previous function from the stack after leaving a function scope.
    fn leave_function(&mut self) {
        if let Some(prev) = self.scope.function_stack.pop() {
            self.scope.function = prev;
        }
    }

    /// Saves the current class on the stack and enters a new class scope.
    fn enter_class(&mut self, name: &str) {
        self.scope.class_stack.push(self.scope.class.clone());
        self.scope.class = Some(name.to_string());
    }

    /// Restores the previous class from the stack after leaving a class scope.
    fn leave_class(&mut self) {
        if let Some(prev) = self.scope.class_stack.pop() {
            self.scope.class = prev;
        }
    }

    /// Saves the current trait on the stack and enters a new trait scope.
    fn enter_trait(&mut self, name: &str) {
        self.scope.trait_stack.push(self.scope.trait_.clone());
        self.scope.trait_ = Some(name.to_string());
    }

    /// Restores the previous trait from the stack after leaving a trait scope.
    fn leave_trait(&mut self) {
        if let Some(prev) = self.scope.trait_stack.pop() {
            self.scope.trait_ = prev;
        }
    }

    /// Delegates to `enter_function` since methods share function-level scope semantics.
    fn enter_method(&mut self, name: &str) {
        self.enter_function(name);
    }

    /// Delegates to `leave_function` since methods share function-level scope semantics.
    fn leave_method(&mut self) {
        self.leave_function();
    }

    /// Pushes a closure name constructed from the current context and source span onto the stack,
    /// incrementing the closure depth counter.
    fn enter_closure(&mut self, span: Span) {
        let name = self.closure_name(span);
        self.scope.closure_names.push(name);
        self.scope.closure_depth += 1;
    }

    /// Pops the most recent closure name from the stack and decrements the closure depth counter.
    fn leave_closure(&mut self) {
        self.scope.closure_names.pop();
        self.scope.closure_depth -= 1;
    }
}
