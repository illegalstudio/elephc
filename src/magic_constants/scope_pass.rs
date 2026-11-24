//! Purpose:
//! Rewrites scope-dependent magic constants according to lexical PHP context.
//! Tracks namespaces, functions, classes, traits, and methods while walking one file's AST.
//!
//! Called from:
//! - `crate::magic_constants::substitute_scope_constants_in_file()`.
//!
//! Key details:
//! - Trait-origin `__CLASS__` uses a placeholder so concrete class binding can happen after trait flattening.
//! - Also widens implicitly nullable parameters (`T $p = null`) and queues their PHP 8.4
//!   deprecation, since this is the pass that knows each parameter's php-src owner name.

use std::collections::HashMap;
use std::path::Path;

use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{AttributeGroup, Expr, ExprKind, MagicConstant, Program, TypeExpr, UseItem, UseKind};
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
        deprecations: Vec::new(),
    };
    let program = walk_program(program, &mut pass);
    // php-src raises these while compiling the file top to bottom. The walk visits hoisted
    // anonymous classes after everything else, so restore source order before queueing.
    pass.deprecations.sort_by_key(|warning| (warning.span.line, warning.span.col));
    for warning in pass.deprecations {
        crate::errors::defer_warning(warning);
    }
    program
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
    /// The enclosing closures set aside while a named function or method declared inside one
    /// is walked: that declaration is its own owner, not the closure.
    closure_names_stack: Vec<Vec<String>>,
    /// Class `use` imports in effect, keyed by lowercased alias, as the name resolver keys them.
    class_imports: HashMap<String, Name>,
    class_imports_stack: Vec<HashMap<String, Name>>,
    /// php-src's display name for each anonymous class being walked (`class@anonymous`,
    /// `Parent@anonymous`), keyed by its synthetic name.
    anonymous_class_labels: HashMap<String, String>,
}

/// Holds the current scope state and the canonical file path being processed.
struct ScopePass {
    scope: Scope,
    file: Option<String>,
    /// Implicit nullable deprecations raised in this file, queued in source order at the end.
    deprecations: Vec<crate::errors::CompileWarning>,
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

    /// Returns `__METHOD__` for the current scope: the closure name inside a closure, otherwise
    /// `Class::method` or the namespace-qualified function name.
    fn method_name(&self) -> String {
        let s = &self.scope;
        if let Some(closure_name) = s.closure_names.last() {
            return closure_name.clone();
        }
        match (self.fqn_class().or_else(|| self.fqn_trait()), &s.function) {
            (Some(c), Some(f)) => format!("{}::{}", c, f),
            (None, Some(f)) => qualify(s.namespace.as_deref(), f),
            _ => String::new(),
        }
    }

    /// Returns the function name php-src prints in a compile-time deprecation.
    ///
    /// That is `__METHOD__`, except inside an anonymous class: php-src's runtime name for one is
    /// `class@anonymous` followed by a NUL byte and the declaring file and position, and the
    /// message is formatted with `%s`, so it stops at the NUL. `m` in `new class { ... }` is
    /// therefore `class@anonymous`, and a closure inside it `{closure:class@anonymous`.
    fn deprecation_owner_name(&self) -> String {
        let name = self.method_name();
        let Some(label) = self.scope.class.as_ref().and_then(|class| {
            let label = self.scope.anonymous_class_labels.get(class)?;
            Some((qualify(self.scope.namespace.as_deref(), class), label))
        }) else {
            return name;
        };
        let (synthetic, label) = label;
        match name.find(&synthetic) {
            Some(at) => format!("{}{}", &name[..at], label),
            None => name,
        }
    }

    /// Resolves a class name written in this file the way php-src names it at compile time:
    /// a fully qualified name as written, otherwise through the class imports and then the
    /// current namespace.
    fn resolve_class_name(&self, name: &Name) -> String {
        if name.is_fully_qualified() {
            return name.parts.join("\\");
        }
        let Some((first, rest)) = name.parts.split_first() else {
            return name.as_canonical();
        };
        match self.scope.class_imports.get(&php_symbol_key(first)) {
            Some(import) => import.parts.iter().chain(rest).cloned().collect::<Vec<_>>().join("\\"),
            None => qualify(self.scope.namespace.as_deref(), name.as_str()),
        }
    }
}

impl Pass for ScopePass {
    /// Widens `T $p = null` to the nullable type PHP gives it and, under a PHP 8.4 or newer
    /// profile, queues PHP 8.4's compile-time deprecation, named the way php-src names the
    /// function (`f`, `C::m`, `{closure:...}`).
    ///
    /// Done per physical file, where the lexical owner is still known; every later pass then
    /// sees an ordinary `?T` parameter, so a passed or defaulted null arrives as null.
    ///
    /// A by-reference parameter keeps its declared type and only gets the deprecation: a `?T`
    /// reference parameter needs nullable storage in every caller variable, so widening it
    /// would refuse `int &$r = null` called with an `int` local, which compiles today.
    ///
    /// An `#[Export]` function also keeps its declared type: its signature is the host-visible
    /// contract, so `--emit phpext` still rejects the implicit form and asks for an explicit `?T`.
    fn transform_param_type(
        &mut self,
        name: &str,
        ty: TypeExpr,
        default: Option<&Expr>,
        by_ref: bool,
        exported: bool,
        span: Span,
    ) -> TypeExpr {
        if !default.is_some_and(|default| matches!(default.kind, ExprKind::Null)) {
            return ty;
        }
        let Some(widened) = ty.implicitly_nullable() else {
            return ty;
        };
        // PHP 8.4 deprecated the implicit form; older `--php-version` profiles accept it silently.
        if crate::codegen_support::compile_php_version().version_id() >= 80400 {
            // Reported at the parameter, as php-src does, not at its function.
            let span = default.map_or(span, |default| default.span);
            self.deprecations.push(crate::errors::CompileWarning::deprecation(
                span,
                &format!(
                    "{}(): Implicitly marking parameter ${} as nullable is deprecated, the \
                     explicit nullable type must be used instead",
                    self.deprecation_owner_name(),
                    name
                ),
            ));
        }
        if by_ref || exported {
            ty
        } else {
            widened
        }
    }

    /// Transforms a magic constant into a string literal based on current lexical scope.
    ///
    /// - `__NAMESPACE__`: current namespace or empty string.
    /// - `__CLASS__`: FQN of current class; uses `TRAIT_CLASS_PLACEHOLDER` when in a trait but outside a class.
    /// - `__TRAIT__`: FQN of current trait or empty string.
    /// - `__FUNCTION__`: closure name if inside a closure, otherwise FQN (unqualified inside class/trait).
    /// - `__METHOD__`: closure name if inside a closure, otherwise `ClassName::methodName`.
    /// - `__FILE__` / `__DIR__`: empty string (handled by the file pass; reaching here means it was skipped).
    fn transform_magic(&self, span: Span, mc: MagicConstant) -> ExprKind {
        let s = &self.scope;
        match mc {
            MagicConstant::Line => ExprKind::IntLiteral(i64::from(span.line)),
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
            MagicConstant::Method => ExprKind::StringLiteral(self.method_name()),
            // File/Dir are handled by the file pass; if they reach here it
            // means the file pass was skipped — substitute to empty rather
            // than panic, since this is best-effort.
            MagicConstant::File | MagicConstant::Dir => ExprKind::StringLiteral(String::new()),
        }
    }

    /// Pushes the current namespace onto the stack and sets the new namespace declaration.
    fn enter_namespace_decl(&mut self, name: &Option<Name>) {
        self.scope.namespace = Some(namespace_string(name));
        self.scope.class_imports.clear();
    }

    /// Saves the current namespace on the stack and enters a new namespace block.
    fn enter_namespace_block(&mut self, name: &Option<Name>) {
        self.scope.namespace_stack.push(self.scope.namespace.clone());
        self.scope.namespace = Some(namespace_string(name));
        let outer = std::mem::take(&mut self.scope.class_imports);
        self.scope.class_imports_stack.push(outer);
    }

    /// Restores the previous namespace from the stack after leaving a namespace block.
    fn leave_namespace_block(&mut self) {
        if let Some(prev) = self.scope.namespace_stack.pop() {
            self.scope.namespace = prev;
        }
        if let Some(outer) = self.scope.class_imports_stack.pop() {
            self.scope.class_imports = outer;
        }
    }

    /// Records class imports, so [`Pass::marks_export`] sees an attribute's imported identity.
    fn observe_use_decl(&mut self, imports: &[UseItem]) {
        for item in imports.iter().filter(|item| matches!(item.kind, UseKind::Class)) {
            self.scope.class_imports.insert(php_symbol_key(&item.alias), item.name.clone());
        }
    }

    /// Recognizes `#[Export]` the way `exports` does after name resolution: by the last segment
    /// of the resolved name. Only an unqualified name can change its last segment while
    /// resolving, through a class import (`use Foo\Marker as Export;` is not the attribute, and
    /// `use Elephc\Export as Ex;` makes `#[Ex]` it); a qualified or fully qualified name keeps
    /// its last segment.
    fn marks_export(&self, attributes: &[AttributeGroup]) -> bool {
        attributes.iter().flat_map(|group| &group.attributes).any(|attr| {
            let name = match attr.name.last_segment() {
                Some(alias) if attr.name.is_unqualified() => self
                    .scope
                    .class_imports
                    .get(&php_symbol_key(alias))
                    .unwrap_or(&attr.name),
                _ => &attr.name,
            };
            name.parts.last().is_some_and(|seg| crate::exports::is_export_attribute_segment(seg))
        })
    }

    /// Saves the current function on the stack and enters a new function scope.
    ///
    /// A function or method declared inside a closure body is named on its own (`inner`, not
    /// `{closure:...}`), so the enclosing closures are set aside until it ends.
    fn enter_function(&mut self, name: &str) {
        self.scope.function_stack.push(self.scope.function.clone());
        self.scope.function = Some(name.to_string());
        let closures = std::mem::take(&mut self.scope.closure_names);
        self.scope.closure_names_stack.push(closures);
    }

    /// Restores the previous function and its enclosing closures after leaving a function scope.
    fn leave_function(&mut self) {
        if let Some(prev) = self.scope.function_stack.pop() {
            self.scope.function = prev;
        }
        if let Some(closures) = self.scope.closure_names_stack.pop() {
            self.scope.closure_names = closures;
        }
    }

    /// Saves the current class on the stack and enters a new class scope.
    fn enter_class(&mut self, name: &str) {
        self.scope.class_stack.push(self.scope.class.clone());
        self.scope.class = Some(name.to_string());
    }

    /// Records php-src's display name for an anonymous class: its parent's name, else its first
    /// interface's, else `class`, followed by `@anonymous`.
    fn observe_class_heritage(&mut self, extends: Option<&Name>, implements: &[Name]) {
        let Some(class) = self.scope.class.clone() else {
            return;
        };
        if !crate::parser::is_anonymous_class_name(&class) {
            return;
        }
        let prefix = extends
            .or(implements.first())
            .map_or_else(|| "class".to_string(), |name| self.resolve_class_name(name));
        self.scope.anonymous_class_labels.insert(class, format!("{}@anonymous", prefix));
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
    fn enter_method(&mut self, name: &str, _type_params: &[crate::parser::ast::TypeParam]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses `source` and returns the declared parameter types of its first top-level function.
    fn widened_param_types(source: &str) -> Vec<Option<TypeExpr>> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let program = substitute_scope_constants_with_file(program, Some("t.php".to_string()));
        program
            .iter()
            .find_map(|stmt| match &stmt.kind {
                StmtKind::FunctionDecl { params, .. } => {
                    Some(params.iter().map(|(_, ty, _, _)| ty.clone()).collect())
                }
                _ => None,
            })
            .expect("a function declaration")
    }

    /// Verifies only a literal-null default on a type that rejects null is widened, a
    /// by-reference parameter keeps its type, and each declaration deprecates exactly once.
    #[test]
    fn widens_implicitly_nullable_params_and_queues_one_deprecation_each() {
        let _ = crate::errors::take_deferred_warnings();
        let types = widened_param_types(
            "<?php function f(string $a = null, ?int $b = null, mixed $c = null, int $d = 0, \
             int|string $e = null, int &$g = null, $h = null) {}",
        );
        assert_eq!(
            types,
            vec![
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Int))),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                Some(TypeExpr::Int),
                Some(TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::Str, TypeExpr::Void])),
                Some(TypeExpr::Int),
                None,
            ]
        );
        let messages: Vec<String> = crate::errors::take_deferred_warnings()
            .into_iter()
            .map(|warning| warning.message)
            .collect();
        let suffix = "as nullable is deprecated, the explicit nullable type must be used instead";
        assert_eq!(
            messages,
            vec![
                format!("f(): Implicitly marking parameter $a {suffix}"),
                format!("f(): Implicitly marking parameter $e {suffix}"),
                format!("f(): Implicitly marking parameter $g {suffix}"),
            ]
        );
    }

    /// Verifies a function or method declared inside a closure body names itself in the
    /// deprecation, as php 8.5 does (`inner()`, `K::m()`), not the enclosing closure, and that
    /// the closure's own name comes back after the declaration ends.
    #[test]
    fn deprecation_names_declarations_nested_in_a_closure() {
        let _ = crate::errors::take_deferred_warnings();
        let tokens = crate::lexer::tokenize(
            "<?php $f = function () {\n\
             function inner(int $x = null) {}\n\
             class K { public function m(int $y = null) {} }\n\
             $g = function (int $z = null) {};\n\
             };",
        )
        .expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let _ = substitute_scope_constants_with_file(program, Some("t.php".to_string()));
        let owners: Vec<String> = crate::errors::take_deferred_warnings()
            .into_iter()
            .map(|warning| warning.message.split("(): ").next().unwrap_or_default().to_string())
            .collect();
        assert_eq!(owners, vec!["inner", "K::m", "{closure:{closure:t.php:1}:4}"]);
    }

    /// Verifies an `#[Export]` function keeps its declared type, so the PHP export validator
    /// still rejects the implicit form, while the deprecation is still queued.
    #[test]
    fn keeps_declared_type_on_exported_function_params() {
        let _ = crate::errors::take_deferred_warnings();
        let types = widened_param_types("<?php #[Export] function f(int $a = null, ?int $b = null) {}");
        assert_eq!(
            types,
            vec![Some(TypeExpr::Int), Some(TypeExpr::Nullable(Box::new(TypeExpr::Int)))]
        );
        assert_eq!(crate::errors::take_deferred_warnings().len(), 1);
    }

    /// Verifies the export check reads an attribute through the file's class imports, as name
    /// resolution will: an alias of another class named `Export` is not the export attribute and
    /// widens, while an alias of the export attribute keeps its declared type.
    #[test]
    fn export_check_resolves_attribute_class_imports() {
        let _ = crate::errors::take_deferred_warnings();
        let widened = Some(TypeExpr::Nullable(Box::new(TypeExpr::Str)));
        let kept = Some(TypeExpr::Str);
        for (source, expected) in [
            ("<?php use Foo\\Marker as Export; #[Export] function f(string $x = null) {}", &widened),
            ("<?php use Foo\\Marker as export; #[EXPORT] function f(string $x = null) {}", &widened),
            ("<?php use Elephc\\Export as Ex; #[Ex] function f(string $x = null) {}", &kept),
            ("<?php use Elephc\\Export; #[Export] function f(string $x = null) {}", &kept),
            ("<?php use Foo\\Marker as Export; #[\\Export] function f(string $x = null) {}", &kept),
            ("<?php use Foo\\Marker as Export; #[Elephc\\Export] function f(string $x = null) {}", &kept),
            ("<?php use function Foo\\marker as Export; #[Export] function f(string $x = null) {}", &kept),
        ] {
            assert_eq!(&widened_param_types(source)[0], expected, "{source}");
        }
        let _ = crate::errors::take_deferred_warnings();
    }

    /// Verifies class imports end with their namespace: a later `namespace` statement or the end
    /// of a namespace block forgets them, so `#[Export]` there is the attribute again.
    #[test]
    fn export_check_forgets_class_imports_with_their_namespace() {
        let _ = crate::errors::take_deferred_warnings();
        let function_param_types = |source: &str| -> Vec<Option<TypeExpr>> {
            let tokens = crate::lexer::tokenize(source).expect("tokenize");
            let program = crate::parser::parse(&tokens).expect("parse");
            let mut types = Vec::new();
            collect_function_param_types(
                &substitute_scope_constants_with_file(program, Some("t.php".to_string())),
                &mut types,
            );
            types
        };
        let widened = Some(TypeExpr::Nullable(Box::new(TypeExpr::Str)));
        let kept = Some(TypeExpr::Str);
        assert_eq!(
            function_param_types(
                "<?php namespace A; use Foo\\Marker as Export; #[Export] function f(string $x = null) {}\n\
                 namespace B; #[Export] function g(string $x = null) {}"
            ),
            vec![widened.clone(), kept.clone()]
        );
        assert_eq!(
            function_param_types(
                "<?php namespace A { use Foo\\Marker as Export; #[Export] function f(string $x = null) {} }\n\
                 namespace B { #[Export] function g(string $x = null) {} }"
            ),
            vec![widened, kept]
        );
        let _ = crate::errors::take_deferred_warnings();
    }

    /// Appends the first parameter type of every function declared in `program`, in order.
    fn collect_function_param_types(program: &Program, types: &mut Vec<Option<TypeExpr>>) {
        for stmt in program {
            match &stmt.kind {
                StmtKind::FunctionDecl { params, .. } => types.push(params[0].1.clone()),
                StmtKind::NamespaceBlock { body, .. } => collect_function_param_types(body, types),
                _ => {}
            }
        }
    }
}
