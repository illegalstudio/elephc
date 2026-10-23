//! Purpose:
//! End-to-end tests for the pre-check fold of `function_exists('name')` on USER functions, which
//! decides the `if (! function_exists('f')) { function f() {} }` polyfill idiom at compile time
//! in top-level program order and flattens the guard's declaration.
//!
//! Called from:
//! - `cargo test` through Rust's integration-test harness (`tests/codegen/optimizer.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.
//! - Callable bodies, include-discovered functions, non-clean conditional declarations and
//!   programs whose top level calls `eval()` keep the runtime probe; controls cover those.

use super::*;

/// Verifies a guarded user function is declared, and that `function_exists` follows program order.
#[test]
fn test_function_exists_guard_declares_user_function() {
    let out = compile_and_run(r#"<?php
var_dump(function_exists('g_late'));
if (!function_exists('g_late')) { function g_late() { return 'late'; } }
var_dump(function_exists('g_late'));
echo g_late(), "\n";
function probe() { return function_exists('g_late') ? 'y' : 'n'; }
echo probe(), "\n";
"#);
    assert_eq!(out, "bool(false)\nbool(true)\nlate\ny\n");
}

/// Verifies an existing builtin or unconditional user function keeps the branch from declaring.
#[test]
fn test_function_exists_guard_skips_existing_functions() {
    let out = compile_and_run(r#"<?php
if (!function_exists('strlen')) { echo "no\n"; }
function plain() { return 'plain'; }
if (!function_exists('plain')) { function plain2() { return 'x'; } }
var_dump(function_exists('plain2'));
echo plain(), "\n";
if (function_exists('nowhere')) { echo "no\n"; } else { echo "absent\n"; }
"#);
    assert_eq!(out, "bool(false)\nplain\nabsent\n");
}

/// Verifies the leading backslash, case-insensitive names and an invalid `\\` needle.
#[test]
fn test_function_exists_guard_name_normalization() {
    let out = compile_and_run(r#"<?php
if (!function_exists('\Mixed_Case')) { function mixed_case() { return 'mc'; } }
echo MIXED_CASE(), "\n";
var_dump(function_exists('\\\\mixed_case'), function_exists('\MIXED_case'));
"#);
    assert_eq!(out, "mc\nbool(false)\nbool(true)\n");
}

/// stays a clear error.
#[test]
fn test_function_exists_guard_inside_loop_is_not_hoisted() {
    let err = compile_cli_file_with_flags_expect_failure(
        "<?php\nforeach ([1] as $_) { if (!function_exists('lp')) { function lp() { return 1; } } }\necho lp();\n",
        &[],
    );
    assert!(err.contains("Undefined function: lp"), "{err}");
}

/// Verifies an include-discovered function keeps its runtime load-order answer next to a polyfill.
#[test]
fn test_function_exists_guard_leaves_include_variants_to_runtime() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() { include 'lib.php'; }
if (!function_exists('poly_cd')) { function poly_cd() { return 'poly'; } }
echo function_exists('late_cd') ? 'yes' : 'no', '|', poly_cd(), '|';
load_lib();
echo function_exists('late_cd') ? late_cd() : 'no';
"#,
            ),
            ("lib.php", "<?php function late_cd() { return 'late'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "no|poly|late");
}

/// Control (also passes on main): top-level `eval()` keeps literal probes at runtime, where eval-declared names exist.
#[test]
fn test_function_exists_probe_after_top_level_eval_stays_runtime() {
    let out = compile_and_run(r#"<?php
echo function_exists('ev_cd') ? 'y' : 'n';
eval('function ev_cd() { return "e"; }');
echo function_exists('ev_cd') ? ev_cd() : 'n', "\n";
"#);
    assert_eq!(out, "ne\n");
}

/// A probe inside a polyfill's guard body answers PHP's program order: `false` before the
/// guarded `function f() {}` statement and `true` after it. The inventory used to be updated
/// only after the whole guard was folded, so the probe after the declaration folded to `false`.
#[test]
fn test_function_exists_probe_after_declaration_inside_its_guard_is_true() {
    let out = compile_and_run(r#"<?php
if (!function_exists('in_guard')) {
    var_dump(function_exists('in_guard'));
    function in_guard() { return 1; }
    var_dump(function_exists('in_guard'));
}
var_dump(function_exists('in_guard'));
if (!function_exists('in_guard')) { function in_guard() { return 2; } }
echo in_guard(), "\n";
"#);
    assert_eq!(out, "bool(false)\nbool(true)\nbool(true)\n1\n");
}
