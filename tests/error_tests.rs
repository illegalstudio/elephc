//! Purpose:
//! Diagnostic test root wiring and shared helpers for compile-time error reporting suites.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Helpers run frontend checks over inline or multi-file PHP fixtures and assert reported diagnostics.

use elephc::lexer::tokenize;
use elephc::parser::parse;
use elephc::parser::parse_with_recovery;
use elephc::types;
use elephc::types::PhpType;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_PROJECT_ID: AtomicUsize = AtomicUsize::new(0);

fn check_source(src: &str) -> Result<(), String> {
    check_source_with_defines(src, &[])
}

fn check_source_with_defines(src: &str, defines: &[&str]) -> Result<(), String> {
    let tokens = tokenize(src).map_err(|e| e.message.clone())?;
    let ast = parse(&tokens).map_err(|e| e.message.clone())?;
    let define_set: HashSet<String> = defines.iter().map(|define| (*define).to_string()).collect();
    let ast = elephc::conditional::apply(ast, &define_set);
    let ast = elephc::name_resolver::resolve(ast).map_err(|e| e.message.clone())?;
    let ast = elephc::optimize::fold_constants(ast);
    types::check(&ast).map_err(|e| e.message.clone())?;
    Ok(())
}

fn check_source_full(src: &str) -> Result<elephc::types::CheckResult, elephc::errors::CompileError> {
    let tokens = tokenize(src).map_err(|e| elephc::errors::CompileError::new(e.span, &e.message))?;
    let ast = parse(&tokens)?;
    let ast = elephc::name_resolver::resolve(ast)?;
    let ast = elephc::optimize::fold_constants(ast);
    types::check(&ast)
}

fn resolve_files_error(
    files: &[(&str, &str)],
    main_file: &str,
) -> elephc::errors::CompileError {
    let id = TEST_PROJECT_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("elephc_error_test_{}_{}", std::process::id(), id));
    fs::create_dir_all(&dir).unwrap();

    for (path, content) in files {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    let php_path = dir.join(main_file);
    let source = fs::read_to_string(&php_path).unwrap();
    let base_dir = php_path.parent().unwrap();

    let result = (|| -> Result<(), elephc::errors::CompileError> {
        let tokens = tokenize(&source)?;
        let ast = parse(&tokens)?;
        let _ = elephc::resolver::resolve(ast, base_dir)?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&dir);
    result.expect_err("expected resolve to fail")
}

fn expect_error(src: &str, expected_substr: &str) {
    match check_source(src) {
        Ok(_) => panic!(
            "Expected error containing '{}', but got Ok",
            expected_substr
        ),
        Err(msg) => {
            assert!(
                msg.contains(expected_substr),
                "Error '{}' doesn't contain '{}'",
                msg,
                expected_substr,
            );
        }
    }
}

fn expect_warning(src: &str, expected_substr: &str) {
    let result = check_source_full(src).expect("expected source to type-check");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.message.contains(expected_substr)),
        "Warnings {:?} do not contain '{}'",
        result
            .warnings
            .iter()
            .map(|warning| warning.message.clone())
            .collect::<Vec<_>>(),
        expected_substr,
    );
}

fn expect_no_warning(src: &str, unexpected_substr: &str) {
    let result = check_source_full(src).expect("expected source to type-check");
    assert!(
        !result
            .warnings
            .iter()
            .any(|warning| warning.message.contains(unexpected_substr)),
        "Warnings {:?} unexpectedly contain '{}'",
        result
            .warnings
            .iter()
            .map(|warning| warning.message.clone())
            .collect::<Vec<_>>(),
        unexpected_substr,
    );
}

macro_rules! expect_builtin_arity_error {
    ($test_name:ident, $src:expr, $expected:expr) => {
        #[test]
        fn $test_name() {
            expect_error($src, $expected);
        }
    };
}

fn resolver_error(src: &str) -> elephc::errors::CompileError {
    let id = TEST_PROJECT_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "elephc_resolver_err_{}_{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).unwrap();
    let main_path = dir.join("main.php");
    fs::write(&main_path, src).unwrap();

    let result = (|| -> Result<(), elephc::errors::CompileError> {
        let tokens = tokenize(src)?;
        let ast = parse(&tokens)?;
        let ast = elephc::magic_constants::substitute_file_and_scope_constants(ast, &main_path);
        let _ = elephc::resolver::resolve(ast, &dir)?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&dir);
    result.expect_err("expected resolver to fail")
}

#[path = "error_tests/syntax.rs"]
mod syntax;
#[path = "error_tests/recovery.rs"]
mod recovery;
#[path = "error_tests/warnings.rs"]
mod warnings;
#[path = "error_tests/type_system.rs"]
mod type_system;
#[path = "error_tests/exceptions_enums_magic.rs"]
mod exceptions_enums_magic;
#[path = "error_tests/classes_traits.rs"]
mod classes_traits;
#[path = "error_tests/extensions.rs"]
mod extensions;
#[path = "error_tests/math_builtins.rs"]
mod math_builtins;
#[path = "error_tests/string_builtins.rs"]
mod string_builtins;
#[path = "error_tests/io_builtins/mod.rs"]
mod io_builtins;
#[path = "error_tests/array_builtins.rs"]
mod array_builtins;
#[path = "error_tests/callables.rs"]
mod callables;
#[path = "error_tests/never.rs"]
mod never;
#[path = "error_tests/misc.rs"]
mod misc;

// --- Iterator-related errors ---

#[test]
fn test_error_foreach_over_object_not_implementing_iterator() {
    expect_error(
        "<?php class Plain { public int $x = 1; } foreach (new Plain() as $v) { echo $v; }",
        "foreach over object requires Plain to implement Iterator",
    );
}

#[test]
fn test_error_iterator_cannot_be_redeclared() {
    expect_error(
        "<?php interface Iterator { public function current(): mixed; }",
        "Cannot redeclare built-in interface: Iterator",
    );
}

#[test]
fn test_error_iterator_aggregate_cannot_be_redeclared() {
    expect_error(
        "<?php interface IteratorAggregate { public function getIterator(): Iterator; }",
        "Cannot redeclare built-in interface: IteratorAggregate",
    );
}

#[test]
fn test_error_iterator_method_requires_declared_return_type() {
    expect_error(
        "<?php
class Bad implements Iterator {
    public function current() { return 1; }
    public function key(): mixed { return 0; }
    public function next(): void {}
    public function valid(): bool { return true; }
    public function rewind(): void {}
}",
        "Cannot implement interface method Bad::current without declaring a compatible return type",
    );
}

#[test]
fn test_error_iterator_method_rejects_incompatible_return_type() {
    expect_error(
        "<?php
class Bad implements Iterator {
    public function current(): mixed { return 1; }
    public function key(): mixed { return 0; }
    public function next(): void {}
    public function valid(): int { return 1; }
    public function rewind(): void {}
}",
        "Cannot implement interface method Bad::valid with incompatible return type int",
    );
}

// --- Generator-related errors ---

#[test]
fn test_error_generator_cannot_be_redeclared() {
    expect_error(
        "<?php class Generator { public function current(): mixed { return null; } }",
        "Cannot redeclare built-in interface: Generator",
    );
}

#[test]
fn test_error_yield_outside_function() {
    expect_error(
        "<?php yield 1;",
        "yield can only be used inside a function or method body",
    );
}

#[test]
fn test_error_yield_in_try_block() {
    expect_error(
        "<?php function gen() { try { yield 1; } catch (\\Throwable $e) {} }",
        "yield inside try/catch/finally is not yet supported",
    );
}

#[test]
fn test_error_yield_in_catch_block() {
    expect_error(
        "<?php function gen() { try {} catch (\\Throwable $e) { yield 1; } }",
        "yield inside try/catch/finally is not yet supported",
    );
}

#[test]
fn test_error_yield_from_outside_function() {
    expect_error(
        "<?php yield from [1, 2, 3];",
        "yield can only be used inside a function or method body",
    );
}

#[test]
fn test_error_yield_from_rejects_non_generator_call() {
    expect_error(
        "<?php
function not_gen(): int { return 1; }
function gen() { yield from not_gen(); }
",
        "yield from expects an array literal or Generator, got Int",
    );
}
