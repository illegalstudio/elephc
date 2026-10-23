//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin includes, including require once chain preserves included file error location, include missing path, and include non string path.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use super::*;

/// Verifies that a require_once chain preserves the original error location
/// in the deepest nested file rather than pointing back to the require_once line.
#[test]
fn test_require_once_chain_preserves_included_file_error_location() {
    let err = resolve_files_error(
        &[
            ("main.php", "<?php\nrequire_once 'a.php';\n"),
            ("a.php", "<?php\nrequire_once 'nested/b.php';\n"),
            ("nested/b.php", "<?php\nfunction broken() {\n    echo 1\n}\n"),
        ],
        "main.php",
    );

    assert_eq!(err.span.line, 4, "expected parser error to point into nested/b.php");
    assert_ne!(err.span.line, 2, "error should not point back to the require_once line");
    assert!(
        Path::new(err.file.as_deref().expect("expected included file path")).ends_with("nested/b.php"),
        "expected file path to reference nested/b.php, got {:?}",
        err.file,
    );
    assert!(
        err.message.contains("Expected ';'"),
        "unexpected error message: {}",
        err.message,
    );
    assert!(
        err.to_string().contains("nested/b.php:4"),
        "expected display output to include nested/b.php:4, got {}",
        err,
    );
}

// --- Float/math function errors ---

/// Verifies that `include ;` (empty expression) produces an "Unexpected token" parse error.
#[test]
fn test_error_include_missing_path() {
    // Empty `include ;` — parse_expr immediately sees `;` and errors out.
    expect_error("<?php include ;", "Unexpected token");
}

/// Verifies that a non-compile-time-constant integer path in include produces a resolver
/// error about "compile-time-constant string", not a runtime-dynamic message.
#[test]
fn test_error_include_non_string_path() {
    // Non-foldable path — parses fine but the resolver rejects it because
    // an integer literal is not a compile-time-constant *string*.
    let err = resolver_error("<?php include 42;");
    assert!(
        err.message.contains("compile-time-constant string"),
        "message did not mention compile-time-constant string: {}",
        err.message
    );
    assert!(
        !err.message.contains("Runtime-dynamic"),
        "static non-string path should not be reported as runtime-dynamic: {}",
        err.message
    );
}

/// Helper: asserts that a resolver error message rejects a runtime-dynamic include/require path
/// and mentions the specific runtime-resolved part.
fn expect_runtime_dynamic_include_path_error(src: &str, expected_detail: &str) {
    let err = resolver_error(src);
    assert!(
        err.message
            .contains("Runtime-dynamic include/require path expressions are not supported"),
        "message did not reject runtime-dynamic include paths: {}",
        err.message
    );
    assert!(
        err.message.contains(expected_detail),
        "message '{}' did not contain '{}'",
        err.message,
        expected_detail
    );
}

/// Verifies that a variable used directly as require path is rejected with a note
/// that variable `$path` is resolved at runtime.
#[test]
fn test_error_include_variable_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php function load($path) { require $path; }",
        "variable `$path` is resolved at runtime",
    );
}

/// Verifies that a concatenated variable path `'lib/' . $path` reports the variable
/// `$path` as the runtime-resolved part.
#[test]
fn test_error_include_concat_variable_path_reports_runtime_part() {
    expect_runtime_dynamic_include_path_error(
        "<?php function load($path) { require 'lib/' . $path; }",
        "variable `$path` is resolved at runtime",
    );
}

/// Verifies that `include_once $path` with a variable path is rejected with a note
/// that variable `$path` is resolved at runtime.
#[test]
fn test_error_include_once_variable_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php function load($path) { include_once $path; }",
        "variable `$path` is resolved at runtime",
    );
}

/// Verifies that a function call used as require path `require pick()` is rejected with a note
/// that the function call is resolved at runtime.
#[test]
fn test_error_include_function_call_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php function pick() { return 'helper.php'; } require pick();",
        "function call `pick()` is resolved at runtime",
    );
}

/// Verifies that a ternary expression used as require path is rejected with a note
/// that ternary path selection is resolved at runtime.
#[test]
fn test_error_include_ternary_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php require true ? 'debug.php' : 'prod.php';",
        "ternary path selection is resolved at runtime",
    );
}

/// Verifies that a property access used as require path `require $config->path` is rejected
/// with a note that property access `->path` is resolved at runtime.
#[test]
fn test_error_include_property_path_is_explicitly_rejected() {
    expect_runtime_dynamic_include_path_error(
        "<?php require $config->path;",
        "property access `->path` is resolved at runtime",
    );
}

// --- INF/NAN function errors ---


/// Verifies a runtime-dynamic path keeps its diagnostic when the include is an EXPRESSION in
/// any position (Laravel's `(fn () => require $path)()`, `tap(require $x, ...)`,
/// `$cache[$p] ??= require $p`): the parser accepts them, the resolver refuses the path.
#[test]
fn test_error_include_expression_dynamic_path_is_rejected() {
    for src in [
        "<?php function load($path) { return (fn () => require $path)(); }",
        "<?php function load($path) { return array_merge(require $path, []); }",
        "<?php function load($path) { static $c = []; return $c[$path] ??= require $path; }",
        "<?php function load($f) { return file_exists($f) ? (include $f) : false; }",
    ] {
        expect_runtime_dynamic_include_path_error(src, "is resolved at runtime");
    }
}

/// Verifies a variable or property passed to a call before an include of the same call is a
/// compile error: PHP binds it before the file runs, so a file that writes it
/// must not change it, but the parameter may be by reference, so it cannot be copied either.
/// A dimension whose index is computed (`$a[index()]`) is such a place too.
#[test]
fn test_error_place_argument_before_include_is_rejected() {
    for src in [
        "<?php function f($a, $b) { echo $a, $b; } $x = 1; f($x, require 'a.php');",
        "<?php class K { public static $p = 1; } function f($a, $b) {} f(K::$p, require 'a.php');",
        "<?php function f($a, $b) {} $x = 1; $c = true; f($x, $c ? require 'a.php' : 0);",
        // A dimension with a computed index is as writable as one with a literal index.
        "<?php function index() { return 0; } function f(&$v, $b) { $v = 9; } $a = [1]; f($a[index()], require 'a.php');",
        "<?php function k() { return 'x'; } function f(&$v, $b) {} $o = new stdClass; $o->p = []; f($o->p[k()][0], require 'a.php');",
    ] {
        let err = resolve_files_error(&[("main.php", src), ("a.php", "<?php $x = 2; return 10;")], "main.php");
        assert!(
            err.message.contains("may be by reference"),
            "{src}: unexpected error message: {}",
            err.message
        );
    }
}

/// Verifies includes in positions whose statements cannot be expanded (a loop condition, an
/// `elseif`, a nested `for` clause, a `match` arm) fail with a clear diagnostic instead of
/// reaching the checker.
#[test]
fn test_error_include_expression_in_unsupported_position() {
    for (src, expected) in [
        ("<?php while (include 'a.php') { break; }", "not supported in this position yet"),
        (
            "<?php $c = 0; if ($c) {} elseif (require 'a.php') { echo 1; }",
            "not supported in this position yet",
        ),
        ("<?php for ($i = 1 + include 'a.php'; false; ) {}", "not supported in this position yet"),
        ("<?php $v = match (1) { 1 => require 'a.php', default => 0 };", "`match` arm"),
    ] {
        let err = resolve_files_error(&[("main.php", src), ("a.php", "<?php return 1;")], "main.php");
        assert!(err.message.contains(expected), "{src}: unexpected error message: {}", err.message);
    }
}

/// Verifies `static $x; $x ??= require F;` is refused: the conditional hoist leaves a `mixed`
/// value, which elephc does not persist into an untyped `static` local.
#[test]
fn test_error_include_coalesce_into_static_local_is_rejected() {
    let err = resolve_files_error(
        &[
            ("main.php", "<?php function p() { static $q; $q ??= require 'a.php'; return $q; } p();"),
            ("a.php", "<?php return [1];"),
        ],
        "main.php",
    );
    assert!(
        err.message.contains("assigned to `static $q`"),
        "unexpected error message: {}",
        err.message
    );
}
