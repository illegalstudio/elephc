//! Purpose:
//! Checks global symbol-table identity across opaque eval and declared functions.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Runtime-selected source keeps these cases on the interpreter bridge.

use crate::support::*;

/// Shares names created by main-scope eval with eval-declared function globals.
#[test]
fn test_eval_main_global_scope_dynamic_names() {
    let source = r#"<?php
$source = $argc > 0 ? '$number = 7; function increment_dynamic_global() { global $number; $number++; } increment_dynamic_global(); echo $number, "\n";' : '';
eval($source);
$next = $argc > 0 ? 'increment_dynamic_global(); echo $number, "\n";' : '';
eval($next);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "8\n9\n");
}

/// Keeps function-local eval variables separate from a declared global binding.
#[test]
fn test_eval_function_global_scope_keeps_locals_separate() {
    let source = r#"<?php
$number = 7;
function read_number(): int { global $number; return $number; }
function invoke_local_eval(string $source): void {
    $number = 20;
    eval($source);
    echo $number, "\n";
}
$source = $argc > 0 ? 'function increment_local_global() { global $number; $number++; } increment_local_global(); echo $number, "\n";' : '';
invoke_local_eval($source);
echo read_number(), "\n";
"#;
    assert_eq!(compile_and_run(source), "20\n20\n8\n");
}

/// Gives eval code PHP's CLI superglobals when the compiled program names none of them (#935).
///
/// Eval found `$_ENV` and `$_SERVER` undefined unless the program spelled them. The fragment
/// runs from the top level and inside a function; each sees the environment in `$_ENV`, the
/// CLI keys in `$_SERVER`, and empty request arrays. Heap debugging proves the created arrays
/// are released with their eval scopes.
#[test]
fn test_eval_creates_cli_superglobals_the_program_never_names() {
    let source = r#"<?php
$code = $argc > 0 ? 'echo array_key_exists("PATH", $_ENV) ? "env" : "no-env", "|", $_SERVER["argc"], "|", $_SERVER["PHP_SELF"] === $_SERVER["argv"][0] ? "self" : "bad", "|", $_SERVER["REQUEST_TIME"] > 0 ? "time" : "no-time", "|", count($_GET) + count($_REQUEST), "\n";' : '';
function probe(string $code): void { eval($code); }
eval($code);
probe($code);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "env|1|self|time|0\nenv|1|self|time|0\n");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "eval-created superglobals leaked: {}",
        output.stderr
    );
}

/// Lets a function-scope eval read and write the superglobals the compiled program seeded,
/// without a `global` statement (#935).
///
/// Superglobals resolved in the eval's local scope, so a function-scope eval saw neither the
/// program's `$_SERVER` nor its `$_ENV`. They now resolve through the synchronized global
/// scope: the native write is visible inside eval, and eval's write reaches native code.
#[test]
fn test_eval_function_scope_shares_program_superglobals() {
    let source = r#"<?php
$_SERVER["FROM_NATIVE"] = "n";
$code = $argc > 0 ? 'echo $_SERVER["FROM_NATIVE"], "|", array_key_exists("PATH", $_ENV) ? "env" : "no-env", "\n"; $_ENV["FROM_EVAL"] = "e";' : '';
function probe(string $code): void { eval($code); }
probe($code);
echo array_key_exists("FROM_EVAL", $_ENV) ? $_ENV["FROM_EVAL"] : "lost", "\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "n|env\ne\n");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "shared superglobals leaked: {}",
        output.stderr
    );
}

/// Unsets a superglobal from a function-scope eval the way PHP does: globally, for good (#935).
///
/// Reads and writes of a superglobal inside eval resolve through the global scope, but
/// `unset()` used to touch only the eval's local scope, so the next read found the global
/// value again, and a superglobal eval had created (`$_SERVER` here, which the compiled
/// program never names) was created again by the next fragment naming it. After the unset,
/// every read (in the same fragment, in a later eval of the same call, in the compiled
/// function, at top level, and in a top-level eval) now sees it undefined, as in PHP 8.5.10.
#[test]
fn test_eval_function_scope_superglobal_unset_is_global() {
    let source = r#"<?php
$dyn = $argc > 0;
$unsetGet = $dyn ? 'unset($_GET); var_dump(isset($_GET));' : '';
$checkGet = $dyn ? 'var_dump(isset($_GET));' : '';
$unsetServer = $dyn ? 'unset($_SERVER); var_dump(isset($_SERVER));' : '';
$checkServer = $dyn ? 'var_dump(isset($_SERVER));' : '';
$_GET["a"] = "native";
function f(string $unset, string $check): void {
    eval($unset);
    eval($check);
    var_dump(isset($_GET));
}
f($unsetGet, $checkGet);
var_dump(isset($_GET));
eval($checkGet);
function g(string $unset, string $check): void {
    eval($unset);
    eval($check);
}
g($unsetServer, $checkServer);
eval($checkServer);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "bool(false)\n".repeat(8));
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "unset superglobals leaked: {}",
        output.stderr
    );
}
