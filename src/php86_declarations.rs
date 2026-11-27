//! Purpose:
//! Emits PHP 8.6's declaration-time deprecations: returning a value from (or making a
//! generator of) a constructor/destructor, a nullable `__debugInfo()` return type, a class
//! constant named `namespace`, a function named `readonly`, and the reserved `let`/`is`
//! identifiers plus `_` as a constant name.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before name resolution.
//!
//! Key details:
//! - PHP reports these when the declaration is compiled, so elephc's startup helper calls
//!   `trigger_error(..., E_USER_DEPRECATED)` once per offending declaration. The whole pass is
//!   gated on `--php-version=8.6`: every message below is new in that profile, and earlier
//!   profiles must stay silent.
//! - Only declarations whose execution is unconditional (top level, namespace block,
//!   `Synthetic`, or an include-once guard) are scanned, matching the point at which the
//!   helper call runs. Nested closures and nested declarations are not entered.
//! - `define('name', ...)` is scanned too: the deprecation is about the constant NAME, which
//!   the call introduces at the same top-level point.

use crate::parser::ast::{
    ClassConst, Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr, UseKind, Visibility,
};
use crate::synthetic_class::*;

/// Internal helper that emits every collected declaration deprecation.
const HELPER: &str = "__elephc_php86_declaration_deprecations";

/// Injects the deprecation helper and its startup call when the program needs them.
///
/// Returns the program unchanged on profiles older than PHP 8.6 and when no declaration is
/// affected.
pub fn inject(program: Program) -> Program {
    if crate::codegen_support::compile_php_version() < crate::php_version::PhpVersion::Php86 {
        return program;
    }
    let mut messages = Vec::new();
    collect_statements(&program, &mut messages);
    let mut warnings = Vec::new();
    collect_magic_visibility(&program, &mut warnings);
    if messages.is_empty() && warnings.is_empty() {
        return program;
    }
    let helper = internal_declarations(|| {
        let mut body = Vec::with_capacity(messages.len() + warnings.len());
        for message in &messages {
            body.push(s_expr(e_call(
                "\\trigger_error",
                vec![e_str(message.as_str()), e_const("E_USER_DEPRECATED")],
            )));
        }
        for warning in &warnings {
            body.push(s_expr(e_call(
                "\\trigger_error",
                vec![e_str(warning.as_str()), e_const("E_USER_WARNING")],
            )));
        }
        vec![function(HELPER).body(body).build()]
    });
    let mut output = helper;
    output.push(Stmt::new(
        StmtKind::ExprStmt(e_call(&format!("\\{HELPER}"), vec![])),
        crate::span::Span::dummy(),
    ));
    output.extend(program);
    output
}

/// Collects deprecations from declarations that execute at the top level.
fn collect_statements(statements: &[Stmt], messages: &mut Vec<String>) {
    for statement in statements {
        match &statement.kind {
            StmtKind::ClassDecl {
                name,
                methods,
                constants,
                ..
            } => {
                collect_reserved_name(name, "class", messages);
                collect_methods(name, methods, messages);
                collect_class_constant_names(constants, messages);
            }
            StmtKind::InterfaceDecl {
                name,
                methods,
                constants,
                ..
            } => {
                collect_reserved_name(name, "interface", messages);
                collect_methods(name, methods, messages);
                collect_class_constant_names(constants, messages);
            }
            StmtKind::TraitDecl {
                name,
                methods,
                constants,
                ..
            } => {
                collect_reserved_name(name, "trait", messages);
                collect_methods(name, methods, messages);
                collect_class_constant_names(constants, messages);
            }
            StmtKind::EnumDecl {
                name,
                methods,
                constants,
                ..
            } => {
                collect_reserved_name(name, "enum", messages);
                collect_methods(name, methods, messages);
                collect_class_constant_names(constants, messages);
            }
            StmtKind::FunctionDecl { name, .. } => collect_function_name(name, messages),
            StmtKind::ConstDecl { name, .. } => collect_constant_name(name, messages),
            StmtKind::UseDecl { imports } => {
                for item in imports {
                    let kind = match item.kind {
                        UseKind::Class => "class",
                        UseKind::Function => "function",
                        UseKind::Const => "constant",
                    };
                    collect_reserved_name(&item.alias, kind, messages);
                    if item.alias == "_" && !matches!(item.kind, UseKind::Function) {
                        messages.push(format!("Using _ as a {kind} name is deprecated"));
                    }
                }
            }
            StmtKind::ExprStmt(expr) => collect_call_declared_name(expr, messages),
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_statements(body, messages),
            _ => {}
        }
    }
}

/// Collects PHP's warning for a magic method declared without public visibility.
fn collect_magic_visibility(statements: &[Stmt], warnings: &mut Vec<String>) {
    for statement in statements {
        match &statement.kind {
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. }
            | StmtKind::EnumDecl { name, methods, .. } => {
                for method in methods {
                    if method.name.eq_ignore_ascii_case("__debugInfo")
                        && method.visibility != Visibility::Public
                    {
                        warnings.push(format!(
                            "The magic method {name}::__debugInfo() must have public visibility"
                        ));
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_magic_visibility(body, warnings),
            _ => {}
        }
    }
}

/// Reports `let`/`is` used as a class, interface, trait, enum, function, or constant name.
fn collect_reserved_name(name: &str, kind: &str, messages: &mut Vec<String>) {
    let lower = name.to_ascii_lowercase();
    if matches!(lower.as_str(), "let" | "is") {
        messages.push(format!("Using {lower} as a {kind} name is deprecated"));
    }
}

/// Reports `readonly` and the reserved identifiers declared as a function name.
fn collect_function_name(name: &str, messages: &mut Vec<String>) {
    if name.eq_ignore_ascii_case("readonly") {
        messages.push("Declaring a function named readonly is deprecated".to_string());
    }
    collect_reserved_name(name, "function", messages);
}

/// Reports the reserved identifiers declared as a global constant name.
fn collect_constant_name(name: &str, messages: &mut Vec<String>) {
    if name == "_" {
        messages.push("Using _ as a constant name is deprecated".to_string());
    }
    collect_reserved_name(name, "constant", messages);
}

/// Reports the reserved identifiers declared as a class constant name.
fn collect_class_constant_names(constants: &[ClassConst], messages: &mut Vec<String>) {
    for constant in constants {
        if constant.name.eq_ignore_ascii_case("namespace") {
            messages.push("Declaring a class constant named namespace is deprecated".to_string());
        }
        collect_constant_name(&constant.name, messages);
    }
}

/// Reports a literal `define('name', ...)` or `class_alias(..., 'name')` call that
/// introduces a reserved declaration name.
fn collect_call_declared_name(expr: &Expr, messages: &mut Vec<String>) {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return;
    };
    let literal_arg = |index: usize| -> Option<&str> {
        match args.get(index).map(|arg| &arg.kind) {
            Some(ExprKind::StringLiteral(value)) => Some(value.as_str()),
            _ => None,
        }
    };
    if name.as_str().eq_ignore_ascii_case("define") {
        if let Some(value) = literal_arg(0) {
            collect_constant_name(value, messages);
        }
    } else if name.as_str().eq_ignore_ascii_case("class_alias") {
        if let Some(alias) = literal_arg(1) {
            collect_reserved_name(alias, "class", messages);
            if alias == "_" {
                messages.push("Using _ as a class name is deprecated".to_string());
            }
        }
    }
}

/// Collects the constructor/destructor and `__debugInfo()` deprecations of one class-like
/// declaration.
fn collect_methods(
    class_name: &str,
    methods: &[crate::parser::ast::ClassMethod],
    messages: &mut Vec<String>,
) {
    for method in methods {
        let key = method.name.to_ascii_lowercase();
        if key == "__debuginfo" {
            if let Some(TypeExpr::Nullable(inner)) = &method.return_type {
                if type_expr_is_array(inner) {
                    messages.push(format!(
                        "Declaring a nullable return type for {class_name}::__debugInfo() is deprecated"
                    ));
                }
            }
            continue;
        }
        let (generator, value) = match key.as_str() {
            "__construct" => (
                "Making a constructor a Generator is deprecated",
                "Returning a value from a constructor is deprecated",
            ),
            "__destruct" => (
                "Making a destructor a Generator is deprecated",
                "Returning a value from a destructor is deprecated",
            ),
            _ => continue,
        };
        if crate::types::checker::yield_validation::body_contains_yield(&method.body) {
            messages.push(generator.to_string());
        } else if body_returns_value(&method.body) {
            messages.push(value.to_string());
        }
    }
}

/// Returns whether a type expression spells PHP's `array` family.
fn type_expr_is_array(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Array(_) | TypeExpr::AssocArray { .. } => true,
        TypeExpr::Named(name) => name.as_str().eq_ignore_ascii_case("array"),
        _ => false,
    }
}

/// Returns whether a method body contains a `return <value>` statement.
fn body_returns_value(body: &[Stmt]) -> bool {
    body.iter().any(statement_returns_value)
}

/// Recursively checks one statement for a value return without entering nested scopes.
fn statement_returns_value(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Return(Some(_)) => true,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            then_body.iter().any(statement_returns_value)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body.iter().any(statement_returns_value))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(statement_returns_value))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(statement_returns_value),
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body.iter().any(statement_returns_value))
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(statement_returns_value))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(statement_returns_value)
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(statement_returns_value))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(statement_returns_value))
        }
        // A nested declaration or function has its own return scope.
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::IfDef { .. } => false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses one probe source.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }

    /// Collects the deprecation messages of one source.
    fn messages_of(source: &str) -> Vec<String> {
        let mut messages = Vec::new();
        collect_statements(&parse(source), &mut messages);
        messages
    }

    /// A value-returning constructor produces one deprecation message.
    #[test]
    fn value_returning_constructor_is_collected() {
        assert_eq!(
            messages_of("<?php class A { public function __construct() { return 1; } }"),
            vec!["Returning a value from a constructor is deprecated"]
        );
    }

    /// A generator constructor produces the generator message instead of the value one.
    #[test]
    fn generator_constructor_is_collected() {
        assert_eq!(
            messages_of("<?php class A { public function __construct() { yield 1; } }"),
            vec!["Making a constructor a Generator is deprecated"]
        );
    }

    /// A plain constructor in a class with other methods emits nothing.
    #[test]
    fn ordinary_constructor_is_ignored() {
        assert!(messages_of(
            "<?php class A { public function __construct() { } public function f(): int { return 1; } }"
        )
        .is_empty());
    }

    /// A non-public `__debugInfo()` produces PHP's magic-method visibility warning.
    #[test]
    fn non_public_debug_info_collects_warning() {
        let mut warnings = Vec::new();
        collect_magic_visibility(
            &parse(
                "<?php enum Foo { case Bar; private function __debugInfo(): array { return []; } }",
            ),
            &mut warnings,
        );
        assert_eq!(
            warnings,
            vec!["The magic method Foo::__debugInfo() must have public visibility"]
        );
    }

    /// A nullable `__debugInfo()` return is deprecated with the declaring class named.
    #[test]
    fn nullable_debug_info_return_is_collected() {
        assert_eq!(
            messages_of(
                "<?php class A { public function __debugInfo(): ?array { return null; } }"
            ),
            vec!["Declaring a nullable return type for A::__debugInfo() is deprecated"]
        );
    }

    /// A class constant named `namespace` is deprecated, methods of that name are not.
    #[test]
    fn namespace_class_constant_is_collected() {
        assert_eq!(
            messages_of("<?php class A { const namespace = 1; }"),
            vec!["Declaring a class constant named namespace is deprecated"]
        );
        assert!(messages_of("<?php class A { public function namespace() {} }").is_empty());
    }

    /// `readonly` as a function name and `let`/`is`/`_` as declaration names are deprecated.
    #[test]
    fn reserved_identifier_declarations_are_collected() {
        assert_eq!(
            messages_of("<?php function readonly() {}"),
            vec!["Declaring a function named readonly is deprecated"]
        );
        assert_eq!(
            messages_of("<?php function is() {}"),
            vec!["Using is as a function name is deprecated"]
        );
        assert_eq!(
            messages_of("<?php class let {}"),
            vec!["Using let as a class name is deprecated"]
        );
        assert_eq!(
            messages_of("<?php const _ = 1;"),
            vec!["Using _ as a constant name is deprecated"]
        );
        assert_eq!(
            messages_of("<?php define('let', 1);"),
            vec!["Using let as a constant name is deprecated"]
        );
        assert_eq!(
            messages_of("<?php use Foo as is;"),
            vec!["Using is as a class name is deprecated"]
        );
        assert_eq!(
            messages_of("<?php class_alias('Foo', '_');"),
            vec!["Using _ as a class name is deprecated"]
        );
    }

    /// The whole pass is gated on the 8.6 profile.
    #[test]
    fn declaration_deprecations_require_php_86() {
        let profile = crate::codegen_support::compile_php_version();
        crate::codegen_support::set_compile_profile(crate::php_version::PhpVersion::Php85, false);
        let program = parse("<?php class A { public function __construct() { return 1; } }");
        let injected = inject(program);
        assert_eq!(injected.len(), 1, "PHP 8.5 must not inject the helper");
        crate::codegen_support::set_compile_profile(profile, false);
    }
}
