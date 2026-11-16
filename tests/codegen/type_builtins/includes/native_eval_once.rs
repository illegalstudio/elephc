//! Purpose:
//! Integration tests for `include_once`/`require_once` state shared between compiled (native)
//! code and code run by `eval()`: a file loaded by one engine must count as loaded for the other.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (via `type_builtins/includes/mod.rs`).
//!
//! Key details:
//! - Each fixture pairs `main.php` with a `module.php` that prints `load|`, so a second load
//!   (or a redeclared interface fatal) shows up in stdout.
//! - Expected output matches php for the same files.

use super::*;

/// Compiles `main` as `main.php` beside `included` as `module.php`, runs the CLI binary from
/// `main.php`, and returns its stdout.
fn run_cross_engine_once(main: &str, included: &str) -> String {
    compile_cli_files_and_run(&[("main.php", main), ("module.php", included)], "main.php")
}

/// Verifies a module loaded by compiled `require_once` is not loaded again by an
/// `include_once` inside `eval()`: "load|" prints once, then "done".
#[test]
fn test_native_include_once_is_visible_to_eval() {
    let out = run_cross_engine_once(r#"<?php
require_once __DIR__.'/module.php';
$path = __DIR__.'/module.php';
eval('include_once $path;');
echo 'done';
"#, "<?php echo 'load|';");
    assert_eq!(out, "load|done");
}

/// Verifies the reverse direction: a module loaded by `include_once` inside `eval()` is not
/// loaded again by a later compiled `require_once`, so "load|" prints once.
#[test]
fn test_eval_include_once_is_visible_to_native() {
    let out = run_cross_engine_once(r#"<?php
$path = __DIR__.'/module.php';
eval('include_once $path;');
require_once __DIR__.'/module.php';
echo 'done';
"#, "<?php echo 'load|';");
    assert_eq!(out, "load|done");
}

/// Verifies a module declaring an interface, loaded natively, is skipped by eval's
/// `include_once` instead of being re-executed and fataling on "Cannot declare interface".
/// PHP outputs "load|done".
#[test]
fn test_native_interface_include_once_is_not_redeclared_by_eval() {
    let out = run_cross_engine_once(r#"<?php
require_once __DIR__.'/module.php';
$path = __DIR__.'/module.php';
eval('include_once $path;');
echo 'done';
"#, "<?php interface CrossEngineProtocol {} echo 'load|';");
    assert_eq!(out, "load|done");
}

/// Verifies a module the compiler discovered through an unreachable `require_once` is not
/// marked as loaded: eval's `include_once` executes it for the first time and declares its
/// interface. PHP outputs "load|done".
#[test]
fn test_skipped_native_interface_include_can_first_execute_in_eval() {
    let out = run_cross_engine_once(r#"<?php
if ($argc < 0) { require_once __DIR__.'/module.php'; }
$path = __DIR__.'/module.php';
eval('include_once $path;');
echo 'done';
"#, "<?php interface InitiallyUnloadedProtocol {} echo 'load|';");
    assert_eq!(out, "load|done");
}
