//! Purpose:
//! Tests pre-check propagation of target-availability guard names and its safety boundaries.
//!
//! Called from:
//! - `cargo test -p elephc --lib target_guard` through Rust's test harness.
//!
//! Key details:
//! - Parser fixtures exercise the same target fold for all five supported output targets.

use super::*;

/// Parses and resolves a fixture before running only the pre-check target-folding pass.
fn fold_source(source: &str, target: &str) -> Program {
    let tokens = crate::lexer::tokenize(source).unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let program = crate::name_resolver::resolve(program).unwrap();
    fold_constants_for_target(program, Target::parse(target).unwrap())
}

/// Keeps fallback provenance from enlarging AST nodes and overflowing synthetic prelude builders.
#[test]
fn test_namespace_polyfill_metadata_preserves_name_layout() {
    assert_eq!(
        std::mem::size_of::<Name>(),
        std::mem::size_of::<(crate::names::NameKind, Vec<String>, String)>(),
    );
}

/// Rebinds available names and retains unavailable PCNTL polyfills independently of the host.
#[test]
fn test_namespace_polyfill_fallback_uses_selected_target() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for (function, expected) in [
            ("str_contains", "str_contains"),
            ("pcntl_getcpu", if target.starts_with("linux-") { "pcntl_getcpu" } else { "App\\pcntl_getcpu" }),
        ] {
            let source = format!("<?php namespace App;
                if (!function_exists('{function}')) {{ function {function}() {{ return 7; }} }}
                echo {function}();");
            let program = fold_source(&source, target);
            let StmtKind::Echo(Expr { kind: ExprKind::FunctionCall { name, .. }, .. }) =
                &program.last().unwrap().kind else { panic!("{program:?}") };
            assert_eq!(name.as_str(), expected, "{target}: {function}");
        }
    }
}

/// Reapplies the resolver's date alias rewrite after the local polyfill disappears.
#[test]
fn test_namespace_polyfill_date_alias_is_desugared() {
    let program = fold_source("<?php namespace App;
        if (!function_exists('date_create')) { function date_create(string $datetime) { return false; } }
        echo date_create('2020-01-01');", "linux-x86_64");
    assert!(matches!(&program.last().unwrap().kind,
        StmtKind::Echo(Expr { kind: ExprKind::NewObject { class_name, .. }, .. })
            if class_name.as_str() == "DateTime"), "{program:#?}");
}

/// Explicit qualification and function imports must not become calls to the global builtin.
#[test]
fn test_namespace_polyfill_explicit_references_do_not_fall_back() {
    for (imports, call) in [
        ("", "\\App\\str_contains('abc', 'b')"),
        ("use App as Alias;", "Alias\\str_contains('abc', 'b')"),
        ("use function App\\str_contains as contains;", "contains('abc', 'b')"),
        ("", "\\App\\str_contains(...)"),
    ] {
        let source = format!("<?php namespace App;
            {imports}
            if (!function_exists('str_contains')) {{
                function str_contains(string $haystack, string $needle): bool {{ return false; }}
            }}
            $result = {call};");
        let program = fold_source(&source, "linux-x86_64");
        let error = crate::types::check_with_target(&program, Target::parse("linux-x86_64").unwrap())
            .expect_err("an explicit missing function must not silently call a builtin");
        assert!(error.message.contains("Undefined function") && error.message.contains("App\\str_contains"),
            "{call}: {}", error.message);
    }
}

/// A declaration under an unknown guard must still own its namespace binding after target folding.
#[test]
fn test_namespace_polyfill_unknown_guard_retains_local_binding() {
    let program = fold_source("<?php namespace App;
        if ($argc > 1) {
            function str_contains(string $haystack, string $needle): bool { return false; }
        }
        echo str_contains('abc', 'b');", "linux-x86_64");
    let StmtKind::Echo(Expr { kind: ExprKind::FunctionCall { name, .. }, .. }) =
        &program.last().unwrap().kind else { panic!("{program:?}") };
    assert_eq!(name.as_str(), "App\\str_contains");
}

/// Resolves variable guards against the output target, independently of the host machine.
#[test]
fn test_target_guard_variable_uses_selected_target() {
    for (target, expected) in [
        ("linux-x86_64", "yes"),
        ("linux-aarch64", "yes"),
        ("macos-aarch64", "no"),
        ("ios-arm64", "no"),
        ("ios-sim-arm64", "no"),
    ] {
        let program = fold_source("<?php $name = 'pcntl_getcpu';
            if (function_exists($name)) { echo 'yes'; } else { echo 'no'; }", target);
        assert!(matches!(&program.last().unwrap().kind,
            StmtKind::Echo(Expr { kind: ExprKind::StringLiteral(value), .. }) if value == expected),
            "{target}: {program:?}");
    }
}

/// Keeps ordinary reads intact while resolving nested guards from namespace constants and aliases.
#[test]
fn test_target_guard_string_facts_preserve_checker_shape() {
    let program = fold_source("<?php namespace Demo;
        const PREFIX = 'pcntl_'; $name = PREFIX . 'getqos_class'; echo $name;
        if ($argc > 0) {
            if (!function_exists($name)) { echo 'fallback'; }
        }", "linux-x86_64");
    assert!(matches!(&program[2].kind, StmtKind::Echo(Expr { kind: ExprKind::Variable(_), .. })));
    let StmtKind::If { then_body, .. } = &program[3].kind else { panic!("{program:?}") };
    assert!(matches!(&then_body[0].kind,
        StmtKind::Echo(Expr { kind: ExprKind::StringLiteral(value), .. }) if value == "fallback"));
}

/// Never substitutes a stale value across calls, branches, references, or condition-side writes.
#[test]
fn test_target_guard_invalidates_unknown_writes_and_aliases() {
    for source in [
        "<?php $name = 'pcntl_getqos_class'; change($name);
            if (function_exists($name)) { echo 'yes'; }",
        "<?php function change() { global $name; $name = 'strtoupper'; }
            $name = 'pcntl_getqos_class'; change();
            if (function_exists($name)) { echo 'yes'; }",
        "<?php $name = 'pcntl_getqos_class'; if ($argc > 1) { $name = 'strtoupper'; }
            if (function_exists($name)) { echo 'yes'; }",
        "<?php $alias = &$name; $name = 'pcntl_getqos_class'; $alias = 'strtoupper';
            if (function_exists($name)) { echo 'yes'; }",
        "<?php $name = 'pcntl_getqos_class';
            if (($name = 'strtoupper') && function_exists($name)) { echo 'yes'; }",
        "<?php $name = 'pcntl_getqos_class'; $callback = function () use (&$name) {};
            $name = 'pcntl_getqos_class'; $callback();
            if (function_exists($name)) { echo 'yes'; }",
        "<?php $name = 'pcntl_getqos_class'; unset($name);
            if (function_exists($name)) { echo 'yes'; }",
    ] {
        let program = fold_source(source, "linux-x86_64");
        let StmtKind::If { condition, .. } = &program.last().unwrap().kind
            else { panic!("Guard was incorrectly removed: {source}: {program:?}") };
        assert!(!matches!(condition.kind, ExprKind::BoolLiteral(_)), "{source}");
    }
}

/// Excludes reference parameters inside a callable without poisoning same-named outer locals.
#[test]
fn test_target_guard_callable_reference_scope() {
    let program = fold_source("<?php
        function probe(&$name, &$alias) {
            $name = 'pcntl_getqos_class'; $alias = 'strtoupper';
            if (function_exists($name)) { echo 'yes'; }
        }
        $name = 'pcntl_getqos_class';
        if (function_exists($name)) { echo 'yes'; } else { echo 'no'; }", "linux-x86_64");
    let StmtKind::FunctionDecl { body, .. } = &program[0].kind else { panic!("{program:?}") };
    assert!(matches!(&body.last().unwrap().kind, StmtKind::If { .. }));
    assert!(matches!(&program.last().unwrap().kind,
        StmtKind::Echo(Expr { kind: ExprKind::StringLiteral(value), .. }) if value == "no"));
}


/// Target-dependent pre-check folding must retain a callable's last syntactic yield.
#[test]
fn test_target_fold_preserves_a_dead_yield_for_generator_classification() {
    let program = fold_source(
        "<?php\nfunction inner(): Generator { yield 1; }\nfunction outer(): Generator {\n    if (PHP_OS_FAMILY === \"Windows\") { yield 2; }\n    return inner();\n}",
        "linux-x86_64",
    );
    let body = program
        .iter()
        .find_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } if name.as_str() == "outer" => Some(body),
            _ => None,
        })
        .expect("outer generator exists");
    assert!(
        crate::types::checker::yield_validation::body_contains_yield(body),
        "the selected target removed the last yield before type checking"
    );
}
