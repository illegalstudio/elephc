//! Purpose:
//! Serves the compiled entry script's returned closure to the runtime's self-include without
//! re-executing the entry through the interpreter.
//!
//! Called from:
//! - `crate::pipeline::compile`, after include resolution and before any compiler prelude.
//!
//! Key details:
//! - Symfony Runtime's `autoload_runtime.php` does `$app = require $_SERVER['SCRIPT_FILENAME'];`.
//!   The compiled entry already executed its top level, but that nested include is value-producing,
//!   so the compiler-included skip does not apply and the interpreter re-runs `public/index.php`.
//! - The entry's shape is a compiled `require_once autoload_runtime.php;` (a no-op on the second
//!   pass) followed by `return static function (...) { ... };`. This module appends a synthetic
//!   function that returns a clone of that capture-less closure. The pipeline registers it as the
//!   include unit for the canonical entry path, so the runtime's existing include-unit dispatch
//!   calls it and receives the closure directly.
//! - Only a trailing top-level `return` of a closure with no captures is eligible, after a prefix
//!   whose second run does nothing (`use`, `require_once`, the resolver's declaration block, a
//!   read-only guard that throws): evaluating that twice has no observable effect, and a closure
//!   that captured entry locals would not be reconstructible at include time. Web and CLI entries
//!   alike: `public/index.php` and `bin/console` are both this shape.

use crate::parser::ast::{ExprKind, Program, StmtKind, TypeExpr};
use crate::names::Name;

/// Name of the synthetic function that returns the entry's closure.
pub(crate) const ENTRY_RETURN_FUNCTION: &str = "__elephc_entry_return_closure";

/// Appends the synthetic entry-return function when the entry ends with a capture-less closure.
///
/// Returns the program and whether the function was injected. The original `return` statement is
/// left in place: the entry's own execution keeps its shape, and only the runtime self-include is
/// served by the synthetic unit.
pub(crate) fn inject(program: Program) -> (Program, bool) {
    if std::env::var_os("ELEPHC_ENTRY_RETURN_DEBUG").is_some() {
        for (index, statement) in program.iter().enumerate() {
            let kind = match &statement.kind {
                StmtKind::UseDecl { .. } => "use",
                StmtKind::Include { once: true, .. } => "include_once",
                StmtKind::IncludeOnceGuard { .. } => "guard",
                StmtKind::Return(_) => "return",
                StmtKind::NamespaceBlock { .. } => "namespace_block",
                StmtKind::Synthetic(_) => "synthetic",
                _ => "other",
            };
            eprintln!("entry-return debug {index}: {kind}");
        }
    }
    if program.iter().any(|statement| {
        matches!(
            &statement.kind,
            StmtKind::FunctionDecl { name, .. }
                if crate::names::php_symbol_key(name.trim_start_matches('\\'))
                    == crate::names::php_symbol_key(ENTRY_RETURN_FUNCTION)
        )
    }) {
        return (program, false);
    }
    let Some((return_index, return_statement)) = program.iter().enumerate().rev().find(|(_, statement)| {
        matches!(
            &statement.kind,
            StmtKind::Return(Some(expr))
                if matches!(
                    &expr.kind,
                    ExprKind::Closure {
                        captures,
                        capture_refs,
                        ..
                    } if captures.is_empty() && capture_refs.is_empty()
                )
        )
    }) else {
        return (program, false);
    };
    let ExprKind::Closure { .. } = &match &return_statement.kind {
        StmtKind::Return(Some(expr)) => expr,
        _ => return (program, false),
    }
    .kind
    else {
        return (program, false);
    };
    // Soundness: the nested include re-executes the entry's top-level statements in PHP. Only
    // skip that re-execution when every preceding statement is idempotent on a second pass: a
    // `require_once`/`include_once` guard is a no-op once the file ran, and `use` declarations
    // bind nothing at run time. Anything else (assignments, output, plain includes, namespace
    // scopes) keeps PHP's re-execution.
    let idempotent_prefix = program[..return_index].iter().all(|statement| {
        matches!(
            &statement.kind,
            StmtKind::UseDecl { .. }
                | StmtKind::Include { once: true, .. }
                // The resolver has already inlined a resolved `require_once` into its guard by
                // the time this runs; a second execution of the guard skips the body.
                | StmtKind::IncludeOnceGuard { .. }
        ) || declares_only(statement)
            || is_passed_guard(statement)
    });
    if !idempotent_prefix {
        return (program, false);
    }
    let mut function = return_statement.clone();
    function.kind = StmtKind::FunctionDecl {
        type_params: Vec::new(),
        name: ENTRY_RETURN_FUNCTION.to_string(),
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        // Untyped would infer `callable`; the include-unit caller consumes the include's value as
        // a boxed Mixed cell, so force the boxing at this function's return.
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body: vec![return_statement.clone()],
    };
    let mut program = program;
    // BEFORE the top-level return: the optimizer treats statements after a top-level `return` as
    // unreachable and dropped the synthetic function, even though php declares a function after
    // that return at compile time.
    program.insert(return_index, function);
    (program, true)
}

/// Whether `statement` is `if (<read-only condition>) { throw ...; }` with no other branch.
///
/// `bin/console` opens with two of them (`if (!is_dir(dirname(__DIR__).'/vendor')) { throw ... }`).
/// The runtime's self-include happens AFTER the first pass went through them, so their condition
/// was false; evaluated again over the same paths it is false again, and the statement does
/// nothing. The body must be unable to complete normally, so that a condition that WAS true could
/// not have let the first pass continue either.
fn is_passed_guard(statement: &crate::parser::ast::Stmt) -> bool {
    let StmtKind::If {
        condition,
        then_body,
        elseif_clauses,
        else_body,
    } = &statement.kind
    else {
        return false;
    };
    elseif_clauses.is_empty()
        && else_body.is_none()
        && !then_body.is_empty()
        && then_body.iter().all(|statement| matches!(statement.kind, StmtKind::Throw(_)))
        && is_read_only(condition)
}

/// Whether evaluating `expr` twice gives the same value and changes nothing: literals, constants
/// and operators over them, and calls to builtins that only inspect the filesystem or the engine.
/// No builtin that can autoload (`class_exists`) or write is admitted.
fn is_read_only(expr: &crate::parser::ast::Expr) -> bool {
    const READ_ONLY_FUNCTIONS: &[&str] = &[
        "is_dir",
        "is_file",
        "is_link",
        "is_readable",
        "file_exists",
        "dirname",
        "basename",
        "defined",
        "extension_loaded",
        "function_exists",
        "version_compare",
    ];
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_) => true,
        ExprKind::Not(inner) => is_read_only(inner),
        ExprKind::BinaryOp { left, right, .. } => is_read_only(left) && is_read_only(right),
        ExprKind::FunctionCall { name, args } => {
            let key = crate::names::php_symbol_key(name.as_str().trim_start_matches('\\'));
            READ_ONLY_FUNCTIONS.contains(&key.as_str()) && args.iter().all(is_read_only)
        }
        _ => false,
    }
}

/// Whether `statement` is the block the resolver prepends with the declarations of the files the
/// entry includes (`resolver::resolve`), an `opcache.preload` graph's among them.
///
/// Those declarations belong to other files: php binds them when THEY are compiled, so a second
/// pass over the entry does not run them. A declaration written in the entry itself is not in
/// this block and still disqualifies the shape (php's second pass would redeclare it).
fn declares_only(statement: &crate::parser::ast::Stmt) -> bool {
    fn declaration(statement: &crate::parser::ast::Stmt) -> bool {
        match &statement.kind {
            StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::FunctionDecl { .. }
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::UseDecl { .. } => true,
            StmtKind::NamespaceBlock { body, .. } => body.iter().all(declaration),
            _ => false,
        }
    }
    matches!(&statement.kind, StmtKind::NamespaceBlock { name: None, body } if body.iter().all(declaration))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a small entry fixture.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// A `require_once` + trailing capture-less closure is the Symfony Runtime entry shape; the
    /// synthetic function is inserted before the return (the optimizer drops statements after a
    /// top-level return).
    #[test]
    fn injects_before_the_return_for_an_idempotent_prefix() {
        let program = parse(
            "<?php\nrequire_once __DIR__.'/vendor/runtime.php';\nreturn static function (array $context) { return $context; };",
        );
        let (program, injected) = inject(program);
        assert!(injected);
        let function_index = program
            .iter()
            .position(|statement| {
                matches!(
                    &statement.kind,
                    StmtKind::FunctionDecl { name, .. } if name == ENTRY_RETURN_FUNCTION
                )
            })
            .expect("synthetic function");
        let return_index = program
            .iter()
            .rposition(|statement| matches!(&statement.kind, StmtKind::Return(Some(_))))
            .expect("entry return");
        assert!(function_index < return_index, "function must precede the return");
        let StmtKind::FunctionDecl { body, return_type, .. } = &program[function_index].kind else {
            unreachable!()
        };
        assert!(matches!(return_type, Some(TypeExpr::Named(name)) if name.as_str() == "mixed"));
        assert!(matches!(body.as_slice(), [statement] if matches!(&statement.kind, StmtKind::Return(Some(expr)) if matches!(expr.kind, ExprKind::Closure { .. }))));
    }

    /// The resolver prepends the declarations of included files (an `opcache.preload` graph) as
    /// one unnamed block; php does not re-run them on the entry's second pass, so the shape stays
    /// eligible. Without this, every `--web` build with a preload lost the unit, and a
    /// closed-world binary refused the runtime's self-include of the entry on every request.
    #[test]
    fn injects_after_the_resolver_declaration_prelude() {
        let mut program = parse(
            "<?php\nrequire_once __DIR__.'/vendor/runtime.php';\nreturn static function (array $context) { return $context; };",
        );
        let prelude = parse("<?php\nnamespace { class PreloadedHelper {} function preloaded_fn() {} }");
        program.splice(0..0, prelude);
        let (_, injected) = inject(program);
        assert!(injected);
    }

    /// A declaration written in the entry itself would be redeclared by php's second pass.
    #[test]
    fn skips_an_entry_that_declares_a_class_itself() {
        let program = parse("<?php\nclass Own {}\nreturn static function () { return 1; };");
        let (_, injected) = inject(program);
        assert!(!injected);
    }

    /// `bin/console`'s shape: read-only guards that throw sit before the runtime require. The
    /// first pass went through them, so the self-include's second pass does nothing there either.
    #[test]
    fn injects_after_read_only_throwing_guards() {
        let program = parse(
            "<?php\nif (!is_dir(dirname('/app/bin').'/vendor')) {\n    throw new LogicException('missing');\n}\nrequire_once '/app/vendor/autoload_runtime.php';\nreturn function (array $context) { return $context; };",
        );
        let (_, injected) = inject(program);
        assert!(injected);
    }

    /// A guard whose body can complete, or whose condition can do something, re-runs in php.
    #[test]
    fn skips_guards_that_are_not_read_only_throws() {
        for source in [
            "<?php\nif (!is_dir('/app/vendor')) {\n    echo 'warn';\n}\nreturn function () { return 1; };",
            "<?php\nif (!class_exists('Foo')) {\n    throw new LogicException('missing');\n}\nreturn function () { return 1; };",
            "<?php\nif (!is_dir('/app/vendor')) {\n    throw new LogicException('a');\n} else {\n    echo 'b';\n}\nreturn function () { return 1; };",
        ] {
            let (_, injected) = inject(parse(source));
            assert!(!injected, "{source}");
        }
    }

    /// Any other top-level statement re-runs on PHP's nested include and must keep it.
    #[test]
    fn skips_an_entry_with_a_run_time_side_effect_before_the_return() {
        let program = parse("<?php\necho 'x';\nreturn static function () { return 1; };");
        let (_, injected) = inject(program);
        assert!(!injected);
    }

    /// A closure capturing entry locals cannot be reconstructed from a unit function.
    #[test]
    fn skips_a_capturing_closure() {
        let program = parse("<?php\nreturn static function () use ($undefined) { return 1; };");
        let (_, injected) = inject(program);
        assert!(!injected);
    }

    /// A user function with the synthetic name keeps its meaning; the optimization is skipped.
    #[test]
    fn skips_when_the_synthetic_name_is_taken() {
        let program = parse(
            "<?php\nfunction __elephc_entry_return_closure() { return 1; }\nrequire_once __DIR__.'/r.php';\nreturn static function () { return 1; };",
        );
        let (_, injected) = inject(program);
        assert!(!injected);
    }
}

#[cfg(test)]
mod shape_debug {
    use super::*;

    #[test]
    fn injects_for_the_symfony_entry_shape() {
        let program = parse_entry();
        for (index, statement) in program.iter().enumerate() {
            eprintln!("{index}: {:?}", statement.kind);
        }
        let (_, injected) = inject(program);
        assert!(injected, "the Symfony entry shape must be eligible");
    }

    fn parse_entry() -> Program {
        let source = "<?php\n\nuse App\\Kernel;\n\nrequire_once dirname(__DIR__).'/vendor/autoload_runtime.php';\n\nreturn static function (array $context) {\n    return new Kernel($context['APP_ENV'], (bool) $context['APP_DEBUG']);\n};\n";
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }
}
