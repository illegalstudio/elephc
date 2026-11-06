//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins, includes include-loaded function variants, including conditional include function variants dispatch false branch, conditional include function variants dispatch true branch, and conditional include single function variant marks loaded branch.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.
//! - Recursive include-loaded functions pin the group's provisional signature (issue #635): a
//!   self-call inside a variant names the group, so its placeholder must carry the declared return.

use super::*;

/// Verifies conditional include function variants: false branch is dispatched when `$pick = 0`.
#[test]
fn test_conditional_include_function_variants_dispatch_false_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = 0;
if ($pick) {
    include 'left.php';
} else {
    include 'right.php';
}
echo selected();
"#,
            ),
            ("left.php", "<?php function selected() { return 'left'; }"),
            ("right.php", "<?php function selected() { return 'right'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "right");
}

/// Verifies conditional include function variants: true branch is dispatched when `$pick = 1`.
#[test]
fn test_conditional_include_function_variants_dispatch_true_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = 1;
if ($pick) {
    include 'left.php';
} else {
    include 'right.php';
}
echo selected_true();
"#,
            ),
            ("left.php", "<?php function selected_true() { return 'left'; }"),
            ("right.php", "<?php function selected_true() { return 'right'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "left");
}

/// Verifies a single conditional include marks the branch as loaded; function is callable after the branch.
#[test]
fn test_conditional_include_single_function_variant_marks_loaded_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = 1;
if ($pick) {
    include 'lib.php';
}
echo optional_selected();
"#,
            ),
            ("lib.php", "<?php function optional_selected() { return 'loaded'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
}

/// Verifies `function_exists()` returns false when a conditional include branch is not taken.
#[test]
fn test_conditional_include_function_exists_tracks_unloaded_variant() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if ($argc > 1) {
    include 'lib.php';
}
if (function_exists('optional_exists')) {
    echo optional_exists();
} else {
    echo 'missing';
}
"#,
            ),
            ("lib.php", "<?php function optional_exists() { return 'loaded'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "missing");
}

/// Verifies `function_exists()` returns true after a conditional include branch is taken.
#[test]
fn test_conditional_include_function_exists_tracks_loaded_variant() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if ($argc >= 1) {
    include 'lib.php';
}
if (function_exists('optional_exists_loaded')) {
    echo optional_exists_loaded();
} else {
    echo 'missing';
}
"#,
            ),
            (
                "lib.php",
                "<?php function optional_exists_loaded() { return 'loaded'; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
}

/// Verifies `function_exists()` tracks runtime load order: before the include the function does not exist, after it does.
#[test]
fn test_include_discovered_function_exists_tracks_runtime_load_order() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() {
    include 'lib.php';
}
echo function_exists('runtime_loaded') ? 'yes-before' : 'no-before';
load_lib();
echo '|';
echo function_exists('runtime_loaded') ? runtime_loaded() : 'no-after';
"#,
            ),
            ("lib.php", "<?php function runtime_loaded() { return 'yes-after'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "no-before|yes-after");
}

/// Verifies calling a function before its runtime load via include produces "undefined function" error.
#[test]
fn test_include_discovered_function_call_before_runtime_load_fails() {
    let err = compile_and_run_files_expect_failure(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() {
    include 'lib.php';
}
echo runtime_loaded_late();
load_lib();
"#,
            ),
            (
                "lib.php",
                "<?php function runtime_loaded_late() { return 'loaded'; }",
            ),
        ],
        "main.php",
    );
    assert!(err.contains("Call to undefined function runtime_loaded_late()"));
}

/// Verifies `function_exists()` tracks runtime load order for `include_once`.
#[test]
fn test_include_once_discovered_function_exists_tracks_runtime_load_order() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_once_lib() {
    include_once 'lib.php';
}
echo function_exists('runtime_loaded_once') ? 'yes-before' : 'no-before';
load_once_lib();
load_once_lib();
echo '|';
echo function_exists('runtime_loaded_once') ? runtime_loaded_once() : 'no-after';
"#,
            ),
            (
                "lib.php",
                "<?php function runtime_loaded_once() { return 'yes-after'; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "no-before|yes-after");
}

/// Verifies `function_exists()` is case-insensitive for fully-qualified include-discovered functions.
#[test]
fn test_conditional_include_function_exists_is_case_insensitive_in_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace App;
if ($argc >= 1) {
    include 'lib.php';
}
echo function_exists('App\\OPTIONAL_CASE') ? optional_case() : 'missing';
"#,
            ),
            (
                "lib.php",
                "<?php namespace App; function optional_case() { return 'loaded'; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
}

/// Verifies include discovery, `use function` lookup, and `function_exists()`
/// stay aligned with PHP's literal string-name semantics in a namespaced file.
#[test]
fn test_include_namespace_fallback_function_exists_stress() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace App;
use function App\Lib\helper;

require __DIR__ . "/lib.php";

echo function_exists("helper") ? "y\n" : "n\n";
echo function_exists("\\App\\Lib\\helper") ? "y\n" : "n\n";
echo helper() . "\n";
"#,
            ),
            (
                "lib.php",
                r#"<?php
namespace App\Lib;

function helper() {
    return "ok";
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "n\ny\nok\n");
}

/// Verifies namespace declarations inside included files are preserved in the correct namespace scope.
#[test]
fn test_conditional_include_function_variants_preserve_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace App;
$pick = 0;
if ($pick) {
    include 'left.php';
} else {
    include 'right.php';
}
echo selected_ns();
"#,
            ),
            (
                "left.php",
                "<?php namespace App; function selected_ns() { return 'left'; }",
            ),
            (
                "right.php",
                "<?php namespace App; function selected_ns() { return 'right'; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "right");
}

/// Verifies `include_once` in a conditional picks the loaded branch and the function is callable.
#[test]
fn test_conditional_include_once_function_variants_dispatch_loaded_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = 1;
if ($pick) {
    include_once 'left.php';
} else {
    include_once 'right.php';
}
echo selected_once();
"#,
            ),
            ("left.php", "<?php function selected_once() { return 'left'; }"),
            ("right.php", "<?php function selected_once() { return 'right'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "left");
}

/// Verifies conditional includes with mismatched function signatures (different return types) produce a compile error.
#[test]
fn test_conditional_include_function_variants_require_matching_signatures() {
    let error = compile_files_error_message(
        &[
            (
                "main.php",
                r#"<?php
$pick = 0;
if ($pick) {
    include 'int.php';
} else {
    include 'string.php';
}
echo selected_mismatch();
"#,
            ),
            ("int.php", "<?php function selected_mismatch(): int { return 1; }"),
            (
                "string.php",
                "<?php function selected_mismatch(): string { return 'one'; }",
            ),
        ],
        "main.php",
    ).expect("conflicting declared signatures must fail compilation");
    assert_eq!(error.matches("Function variants for 'selected_mismatch' must have identical signatures").count(), 1, "{error}");
}

/// Conflicting recursive return hints report the group contract before a borrowed placeholder.
#[test]
fn test_conditional_include_recursive_variant_signature_mismatch_diagnostic() {
    let error = compile_files_error_message(
        &[
            (
                "main.php",
                "<?php\nif ($argc > 1) { include 'left.php'; } else { include 'right.php'; }\necho selected_recursive(1);",
            ),
            (
                "left.php",
                "<?php\nfunction selected_recursive(int $n): string { if ($n == 0) { return 'left'; } return selected_recursive($n - 1); }",
            ),
            (
                "right.php",
                "<?php\nfunction selected_recursive(int $n): int { if ($n == 0) { return 1; } return selected_recursive($n - 1); }",
            ),
        ],
        "main.php",
    ).expect("conflicting recursive signatures must fail compilation");
    assert_eq!(error.matches("Function variants for 'selected_recursive' must have identical signatures").count(), 1, "{error}");
}

/// Verifies two regular includes of the same file in the same branch report a duplicate function error.
#[test]
fn test_same_branch_conditional_includes_still_report_duplicate_function() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
$pick = 1;
if ($pick) {
    include 'a.php';
    include 'b.php';
}
"#,
            ),
            ("a.php", "<?php function same_branch_duplicate() { return 1; }"),
            ("b.php", "<?php function same_branch_duplicate() { return 2; }"),
        ],
        "main.php",
    ));
}

/// Verifies a regular `include` inside a constant-false branch does not claim the file; later `include` still succeeds.
#[test]
fn test_regular_include_in_constant_false_branch_does_not_duplicate_later_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'lib.php';
}
include 'lib.php';
echo false_branch_value();
"#,
            ),
            ("lib.php", "<?php function false_branch_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a regular `include` inside a constant-false `elseif` does not claim the file; later `include` still succeeds.
#[test]
fn test_regular_include_in_constant_false_elseif_chain_does_not_duplicate_later_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'lib.php';
} elseif (false) {
    include 'lib.php';
}
include 'lib.php';
echo false_elseif_value();
"#,
            ),
            ("lib.php", "<?php function false_elseif_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a runtime-possible include branch followed by a regular include still reports duplicate when both execute.
#[test]
fn test_regular_include_possible_branch_then_later_include_still_reports_duplicate() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
if (time() > 0) {
    include 'lib.php';
}
include 'lib.php';
"#,
            ),
            ("lib.php", "<?php function maybe_duplicated() { return 1; }"),
        ],
        "main.php",
    ));
}

/// Verifies regular `include` inside a loop reports duplicate declaration error.
#[test]
fn test_regular_include_declaration_in_loop_reports_duplicate() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 2) {
    include 'lib.php';
    $i = $i + 1;
}
"#,
            ),
            ("lib.php", "<?php function loop_duplicated() { return 1; }"),
        ],
        "main.php",
    ));
}

/// Verifies `include_once` in a loop with a nested regular `include` discovers declarations exactly once.
#[test]
fn test_include_once_in_loop_with_nested_regular_include_discovers_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 2) {
    include_once 'outer.php';
    $i = $i + 1;
}
echo nested_once_value();
"#,
            ),
            ("outer.php", "<?php include 'inner.php';"),
            ("inner.php", "<?php function nested_once_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies `include_once` in a runtime-possible branch still discovers declarations when the branch is not taken.
#[test]
fn test_include_once_possible_branch_then_later_include_once_discovers_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (time() < 0) {
    include_once 'lib.php';
}
include_once 'lib.php';
echo once_later_value();
"#,
            ),
            ("lib.php", "<?php function once_later_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies `include_once` in mutually exclusive branches scans context-sensitive nested includes (dynamic path via define).
#[test]
fn test_include_once_exclusive_branches_scan_context_sensitive_nested_includes() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = time() < 0;
if ($pick) {
    define('TARGET_FILE', 'a.php');
    include_once 'outer.php';
    echo branch_a_value();
} else {
    define('TARGET_FILE', 'b.php');
    include_once 'outer.php';
    echo branch_b_value();
}
"#,
            ),
            ("outer.php", "<?php include TARGET_FILE;"),
            ("a.php", "<?php function branch_a_value() { return 'a'; }"),
            ("b.php", "<?php function branch_b_value() { return 'b'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "b");
}

/// A conditional variant group's collector takes a named tail, in every variant.
///
/// A variant group is TWO signatures: the group signature every call site sees, and the per-variant
/// signature that carries the body actually compiled. Both have to sit on the descriptor container
/// or the descriptor publishes one collector shape while the selected variant's frame reads the
/// other, which is how a named tail's hash header gets read as an indexed length. Both variants
/// declare the same collector here, and the fixture routes through `call_user_func_array` with a
/// variable container so the invoker receives the named entry untouched.
#[test]
fn test_conditional_include_function_variant_collector_takes_a_named_tail() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = time() < 0;
if ($pick) {
    include 'left_tail.php';
} else {
    include 'right_tail.php';
}
function bindTail(callable $callback, array $arguments): string {
    return call_user_func_array($callback, $arguments);
}
$positional = ['lead', 'first tail'];
$named = ['head' => 'lead', 'alpha' => 'named alpha'];
$mixed = ['lead', 'first tail', 'beta' => 'named beta'];
echo bindTail(variant_tail(...), $positional), '|', bindTail(variant_tail(...), $named);
echo '|', bindTail(variant_tail(...), $mixed);
"#,
            ),
            (
                "left_tail.php",
                r#"<?php
function variant_tail(string $head, ...$rest): string {
    $out = 'left/' . $head . '#' . count($rest);
    foreach ($rest as $key => $value) {
        $out .= ';' . (is_string($key) ? 's' : 'i') . $key . '=' . $value;
    }
    return $out;
}
"#,
            ),
            (
                "right_tail.php",
                r#"<?php
function variant_tail(string $head, ...$rest): string {
    $out = 'right/' . $head . '#' . count($rest);
    foreach ($rest as $key => $value) {
        $out .= ';' . (is_string($key) ? 's' : 'i') . $key . '=' . $value;
    }
    return $out;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "right/lead#1;i0=first tail|right/lead#1;salpha=named alpha|right/lead#2;i0=first tail;sbeta=named beta",
    );
}

/// A recursive `: string` function declared in two mutually exclusive included files.
///
/// Each variant's self-call names the group, so it is typed from the group's provisional
/// signature. That placeholder hardcoded `Int`, and both variants were rejected with `return type
/// expects Str, got Int` although the same function compiles when declared in the main file
/// (issue #635). Reference PHP prints `Rzrr` and `RQRR|4`.
#[test]
fn test_conditional_include_recursive_string_variant_keeps_declared_return() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = time() < 0;
if ($pick) {
    include 'left.php';
} else {
    include 'right.php';
}
echo pad_to("z"), "\n";
echo strtoupper(pad_to("q")), "|", strlen(pad_to("")), "\n";
"#,
            ),
            (
                "left.php",
                r#"<?php
function pad_to(string $x): string {
    if (strlen($x) > 2) { return "L" . $x; }
    return pad_to($x . "l");
}
"#,
            ),
            (
                "right.php",
                r#"<?php
function pad_to(string $x): string {
    if (strlen($x) > 2) { return "R" . $x; }
    return pad_to($x . "r");
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Rzrr\nRQRR|4\n");
}

/// Recursive include-loaded functions returning an `array` and an object, read by outside callers.
///
/// An included function is a one-variant group, so it hit the same `Int` placeholder: the array
/// function failed its return check, and the placeholder leaked to the top-level callers as
/// `count() argument must be array` and `Property access requires an object` (issue #635).
/// Reference PHP prints `2|3|4` and `30`.
#[test]
fn test_include_loaded_recursive_array_and_object_returns_reach_callers() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require 'lib.php';
$a = count_up(0);
echo count($a), "|", $a[0], "|", $a[1], "\n";
$o = boxed(0);
echo $o->v, "\n";
"#,
            ),
            (
                "lib.php",
                r#"<?php
class Holder { public int $v = 0; }
function count_up(int $x): array {
    if ($x > 2) { return [$x, $x + 1]; }
    $next = count_up($x + 1);
    return $next;
}
function boxed(int $x): Holder {
    if ($x > 2) { $h = new Holder(); $h->v = $x * 10; return $h; }
    return boxed($x + 1);
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "2|3|4\n30\n");
}

/// Mutual recursion between two include-loaded `: string` functions.
///
/// Resolving `ping` re-enters the `pong` group, whose body calls back into `ping` while its
/// provisional signature is still published; before the fix the second leg reported `return type
/// expects Str, got Int` (issue #635). Reference PHP prints `zioi`.
#[test]
fn test_include_loaded_mutual_recursion_keeps_declared_returns() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php\nrequire 'lib.php';\necho ping(\"z\"), \"\\n\";\n"),
            (
                "lib.php",
                r#"<?php
function ping(string $x): string { if (strlen($x) > 3) { return $x; } return pong($x . "i"); }
function pong(string $x): string { if (strlen($x) > 3) { return $x; } return ping($x . "o"); }
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "zioi\n");
}

/// Unhinted recursive include-loaded functions still infer their return from the base case.
///
/// The unhinted placeholder stays `Int`, exactly as for free functions: the base case's real type
/// absorbs it in the `wider_type` merge. Pinned so seeding hinted groups cannot disturb inference.
/// Reference PHP prints `ZAA|3`, `bool(true)` and `NULL`.
#[test]
fn test_include_loaded_unhinted_recursion_infers_from_base_case() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require 'lib.php';
$s = grow("z");
echo strtoupper($s), "|", strlen($s), "\n";
var_dump(reach_true(0));
var_dump(reach_null(0));
"#,
            ),
            (
                "lib.php",
                r#"<?php
function grow(string $x) { if (strlen($x) > 2) { return $x; } return grow($x . "a"); }
function reach_true(int $x) { if ($x > 2) { return true; } return reach_true($x + 1); }
function reach_null(int $x) { if ($x > 2) { return null; } return reach_null($x + 1); }
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ZAA|3\nbool(true)\nNULL\n");
}

/// Recursive include-loaded generators iterate their self-call without a bogus warning.
///
/// A body containing `yield` returns a `Generator`, so that is what the group placeholder must
/// hold. With the old `Int` placeholder the checker warned `foreach() argument must be of type
/// array|object, int given; the loop body will never run` on each recursive `foreach`, although
/// the loop does run (issue #635). Reference PHP prints `3,2,1,0,` and `2,1,0,`.
#[test]
fn test_include_loaded_recursive_generators_resolve_to_generator() {
    let files: &[(&str, &str)] = &[
        (
            "main.php",
            r#"<?php
require 'lib.php';
foreach (countdown(3) as $v) { echo $v, ","; }
echo "\n";
foreach (countdown_unhinted(2) as $v) { echo $v, ","; }
echo "\n";
"#,
        ),
        (
            "lib.php",
            r#"<?php
function countdown(int $n): Generator {
    yield $n;
    if ($n > 0) { foreach (countdown($n - 1) as $v) { yield $v; } }
}
function countdown_unhinted(int $n) {
    yield $n;
    if ($n > 0) { foreach (countdown_unhinted($n - 1) as $v) { yield $v; } }
}
"#,
        ),
    ];
    let warnings = check_files_diagnostics(files, "main.php", false)
        .expect("recursive include-loaded generators should type-check");
    assert!(
        warnings.iter().all(|warning| !warning.contains("foreach()")),
        "unexpected foreach warning: {warnings:?}"
    );
    assert_eq!(compile_and_run_files(files, "main.php"), "3,2,1,0,\n2,1,0,\n");
}

/// A recursive include-loaded `: string` function whose base case returns an int is still rejected.
///
/// The negative control for the group placeholder: trusting the declared hint for the self-call
/// must not turn into trusting it for the real returns.
#[test]
fn test_include_loaded_recursive_wrong_base_case_is_still_rejected() {
    let error = compile_files_error_message(
        &[
            ("main.php", "<?php\nrequire 'lib.php';\necho bad_base(0), \"\\n\";\n"),
            (
                "lib.php",
                r#"<?php
function bad_base(int $x): string {
    if ($x > 2) { return $x; }
    return bad_base($x + 1);
}
"#,
            ),
        ],
        "main.php",
    )
    .expect("a string function returning an int base case must not type-check");
    assert!(
        error.contains("return type expects string, got int"),
        "unexpected diagnostic: {error}"
    );
}
