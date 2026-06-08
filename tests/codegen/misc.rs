//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of misc, including iife returns string, iife returns integer, and empty php file.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Compiles a program whose source begins with a UTF-8 BOM (U+FEFF) before `<?php` and
/// verifies it builds and runs end-to-end, matching editors that emit BOM-prefixed UTF-8.
#[test]
fn test_utf8_bom_prefixed_source_compiles_and_runs() {
    let out = compile_and_run("\u{feff}<?php echo \"hi\";");
    assert_eq!(out, "hi");
}

// --- IIFE (Immediately Invoked Function Expression) ---

/// Compiles an IIFE that returns a string literal and verifies the value is echoed correctly.
#[test]
fn test_iife_returns_string() {
    let out = compile_and_run(
        r#"<?php
$result = (function() { return "hello"; })();
echo $result;
"#,
    );
    assert_eq!(out, "hello");
}

/// Compiles an IIFE with a parameter that doubles its argument and verifies the result is 42.
#[test]
fn test_iife_returns_int() {
    let out = compile_and_run(
        r#"<?php
echo (function($x) { return $x * 2; })(21);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies parenthesized expressions can appear as standalone statements, including object and closure calls.
#[test]
fn test_parenthesized_expression_statements() {
    let out = compile_and_run(
        r#"<?php
class C { function m() { echo "C"; } }
(new C())->m();
(function () { echo "|F"; })();
(1 + 2);
echo "|3";
"#,
    );
    assert_eq!(out, "C|F|3");
}

// --- Empty input / EOF handling ---

/// Compiles a PHP file containing only `<?php\n` and verifies no output is produced.
#[test]
fn test_empty_php_file() {
    let out = compile_and_run("<?php\n");
    assert_eq!(out, "");
}

/// Compiles a PHP file containing only `<?php ` with no code and verifies no output.
#[test]
fn test_only_open_tag() {
    let out = compile_and_run("<?php ");
    assert_eq!(out, "");
}

// --- Syntactic return type inference ---

/// Verifies return type inference for a function that returns mid-do-while loop with an early exit.
/// The fixpoint return type must account for the mid-loop return, not just the post-loop return.
#[test]
fn test_callback_return_from_dowhile() {
    let out = compile_and_run(
        r#"<?php
function find_first($arr) {
    $i = 0;
    do {
        if ($arr[$i] > 5) { return $arr[$i]; }
        $i = $i + 1;
    } while ($i < count($arr));
    return 0;
}
echo find_first([1, 3, 7, 2]);
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies type widening for a function with conditional string/int returns; the declared return
/// type must be wide enough to hold both branches and the string "big" must be returned.
#[test]
fn test_mixed_return_types_widened() {
    let out = compile_and_run(
        r#"<?php
function describe($n) {
    if ($n > 100) { return "big"; }
    if ($n < 0) { return "negative"; }
    return $n;
}
echo describe(200);
"#,
    );
    assert_eq!(out, "big");
}

/// Verifies null-coalescing a null variable with a string literal default allocates the string
/// and does not evaluate the default eagerly.
#[test]
fn test_null_coalesce_allocates_for_string_default() {
    let out = compile_and_run(
        r#"<?php
function test() {
    $x = null;
    $result = $x ?? "fallback";
    echo $result;
}
test();
"#,
    );
    assert_eq!(out, "fallback");
}

/// Verifies null-coalescing when the left-hand side evaluates to null at runtime (ternary
/// produces null) uses the string default and outputs "fallback".
#[test]
fn test_null_coalesce_runtime_null_to_string_default() {
    let out = compile_and_run(
        r#"<?php
$x = false ? 1 : null;
$result = $x ?? "fallback";
echo $result;
"#,
    );
    assert_eq!(out, "fallback");
}

/// Verifies null-coalescing assignment (`??=`) assigns the right-hand side when the variable
/// is null.
#[test]
fn test_null_coalesce_assignment_assigns_when_null() {
    let out = compile_and_run(
        r#"<?php
$x = null;
$x ??= 7;
echo $x;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies null-coalescing assignment (`??=`) skips the right-hand side when the variable is
/// non-null; the fallback function must not be called.
#[test]
fn test_null_coalesce_assignment_skips_rhs_when_non_null() {
    let out = compile_and_run(
        r#"<?php
function fallback() {
    echo "bad";
    return 99;
}
$x = 5;
$x ??= fallback();
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies null-coalescing assignment with a typed function return keeps the int type when
/// assigned null; the null is discarded and the original value is preserved.
#[test]
fn test_null_coalesce_assignment_literal_null_keeps_non_null_type() {
    let out = compile_and_run(
        r#"<?php
function value(): int {
    return 5;
}
$x = value();
$x ??= null;
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies null-coalescing assignment updates a variable that is null at runtime (ternary
/// produces null) and assigns 9.
#[test]
fn test_null_coalesce_assignment_updates_runtime_null() {
    let out = compile_and_run(
        r#"<?php
$x = false ? 1 : null;
$x ??= 9;
echo $x;
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies null-coalescing assignment leaves a non-null string unchanged.
#[test]
fn test_null_coalesce_assignment_keeps_non_null_string() {
    let out = compile_and_run(
        r#"<?php
$x = "keep";
$x ??= "fallback";
echo $x;
"#,
    );
    assert_eq!(out, "keep");
}

/// Verifies null-coalescing assignment in a for-loop initializer: the ??= runs on the first
/// iteration and the loop then iterates 0, 1, 2.
#[test]
fn test_null_coalesce_assignment_in_for_init() {
    let out = compile_and_run(
        r#"<?php
$i = null;
for ($i ??= 0; $i < 3; $i++) {
    echo $i;
}
"#,
    );
    assert_eq!(out, "012");
}

/// Verifies return type inference for a closure with branches that return different types;
/// the fixpoint return type must account for the branch return.
#[test]
fn test_closure_return_type_from_nested_branch() {
    let out = compile_and_run(
        r#"<?php
$describe = function($n) {
    if ($n > 0) {
        return "positive";
    }
    return 0;
};
$result = $describe(3);
echo $result;
"#,
    );
    assert_eq!(out, "positive");
}

/// Verifies a function call whose result is assigned to a local variable and then echoed
/// produces the correct concatenated string output.
#[test]
fn test_assigned_user_function_call_string_result() {
    let out = compile_and_run(
        r#"<?php
function greet($name) {
    return "Hello, " . $name;
}
function run() {
    $message = greet("World");
    echo $message;
}
run();
"#,
    );
    assert_eq!(out, "Hello, World");
}

/// Verifies a ternary with int/string branches allocates the wider type (string) at runtime
/// when the condition is false.
#[test]
fn test_ternary_allocates_for_wider_type() {
    let out = compile_and_run(
        r#"<?php
function test($flag) {
    $val = $flag ? 42 : "none";
    echo $val;
}
test(false);
"#,
    );
    assert_eq!(out, "none");
}

/// Verifies a ternary in a function where both branches return strings produces correct output
/// for both positive and non-positive inputs.
#[test]
fn test_ternary_both_branches_in_function() {
    let out = compile_and_run(
        r#"<?php
function label($n) {
    $result = $n > 0 ? "positive" : "zero or negative";
    return $result;
}
echo label(5) . "|" . label(-1);
"#,
    );
    assert_eq!(out, "positive|zero or negative");
}
