//! Purpose:
//! Regression coverage for PHP namespace fallback after availability guards remove polyfills.
//!
//! Called from:
//! - `cargo test --test codegen_tests_oop namespace_polyfill` through Rust's test harness.
//!
//! Key details:
//! - Unqualified calls may fall back globally; retained declarations and explicit imports win.

use crate::support::*;

/// Applies the fallback to polyfills loaded from a separate namespaced include file.
#[test]
fn test_namespace_polyfill_from_included_file() {
    let out = compile_and_run_files(&[
        ("main.php", r#"<?php namespace App;
            require 'polyfill.php';
            var_dump(str_contains('abc', 'b'));
        "#),
        ("polyfill.php", r#"<?php namespace App;
            if (!function_exists('str_contains')) {
                function str_contains(string $haystack, string $needle): bool { return false; }
            }
        "#),
    ], "main.php");
    assert_eq!(out, "bool(true)\n");
}

/// Uses the same fallback for other available string and array builtins.
#[test]
fn test_namespace_polyfill_other_available_builtins() {
    let out = compile_and_run(r#"<?php
namespace App;
if (!function_exists('str_starts_with')) {
    function str_starts_with(string $haystack, string $needle): bool { return false; }
}
if (!function_exists('array_is_list')) {
    function array_is_list(array $values): bool { return false; }
}
var_dump(str_starts_with('abc', 'a'), array_is_list([1, 2]));
"#);
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// Restores a global user-function fallback without assuming every global target is a builtin.
#[test]
fn test_namespace_polyfill_global_user_function_fallback() {
    let out = compile_and_run(r#"<?php
namespace { function fallback(): string { return 'global'; } }
namespace App {
    if (!function_exists('str_contains')) {
        function fallback(): string { return 'local'; }
    }
    echo fallback();
}
"#);
    assert_eq!(out, "global");
}

/// Falls back to an available builtin after discarding a namespaced conditional declaration.
#[test]
fn test_namespace_polyfill_available_builtin() {
    let out = compile_and_run(r#"<?php
namespace App;
if (!function_exists('str_contains')) {
    function str_contains(string $haystack, string $needle): bool { return false; }
}
var_dump(str_contains("abc", "b"));
var_dump(StR_CoNtAiNs("abc", "b"));
var_dump(\str_contains("abc", "b"));
var_dump(str_contains(needle: 'b', haystack: 'abc'));
var_dump(function_exists('App\\str_contains'));
"#);
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\n");
}

/// Reuses resolver desugaring when the pruned polyfill shadows a procedural date alias.
#[test]
fn test_namespace_polyfill_builtin_alias_desugaring() {
    let out = compile_and_run(r#"<?php
namespace App;
if (!function_exists('date_create')) {
    function date_create(string $datetime) { return false; }
}
echo get_class(date_create('2020-01-01'));
"#);
    assert_eq!(out, "DateTime");
}

/// Resolves nonliteral guards and first-class callable targets through the same fallback.
#[test]
fn test_namespace_polyfill_nonliteral_guard_and_callable() {
    let out = compile_and_run(r#"<?php
namespace App {
    const PREFIX = 'str_';
    $name = PREFIX . 'contains';
    if (!function_exists($name)) {
        function str_contains(string $haystack, string $needle): bool { return false; }
    }
    $contains = str_contains(...);
    var_dump($contains('abc', 'b'));
    function check(): bool { return str_contains('abc', 'b'); }
    class Probe {
        public function check(): bool { return str_contains('abc', 'b'); }
    }
    $check = fn(): bool => str_contains('abc', 'b');
    var_dump(check(), (new Probe())->check(), $check());
}
"#);
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\nbool(true)\n");
}

/// Keeps a retained namespace declaration and explicit function imports ahead of global builtins.
#[test]
fn test_namespace_polyfill_preserves_shadows_and_imports() {
    let out = compile_and_run(r#"<?php
namespace Local {
    if (function_exists('str_contains')) {
        function str_contains(string $haystack, string $needle): bool { return false; }
    }
    var_dump(str_contains('abc', 'b'));
    var_dump(\str_contains('abc', 'b'));
}
namespace App {
    use function Local\str_contains;
    var_dump(str_contains('abc', 'b'));
}
"#);
    assert_eq!(out, "bool(false)\nbool(true)\nbool(false)\n");
}

/// Does not confuse identically named declarations in separate namespace blocks.
#[test]
fn test_namespace_polyfill_independent_namespace_blocks() {
    let out = compile_and_run(r#"<?php
namespace First {
    if (!function_exists('str_contains')) {
        function str_contains(string $haystack, string $needle): bool { return false; }
    }
    var_dump(str_contains('abc', 'b'));
}
namespace Second {
    function str_contains(string $haystack, string $needle): bool { return false; }
    var_dump(str_contains('abc', 'b'));
}
"#);
    assert_eq!(out, "bool(true)\nbool(false)\n");
}
