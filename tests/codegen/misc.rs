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

/// Verifies a top-level `return <expr>;` halts the script and discards the value
/// (PHP only uses a script's return value for includes). Previously this errored
/// as "return values on the EIR backend entry function".
#[test]
fn test_top_level_return_value_halts_and_is_discarded() {
    let out = compile_and_run("<?php echo \"a\"; return 5; echo \"b\";");
    assert_eq!(out, "a");
}

/// Verifies `declare(strict_types=1);` is accepted and compiles as a no-op,
/// so the rest of the program runs normally (elephc is always strict).
#[test]
fn test_declare_strict_types_is_accepted_and_noop() {
    let out = compile_and_run("<?php declare(strict_types=1); echo 1 + 2;");
    assert_eq!(out, "3");
}

/// Verifies the `declare(ticks=1) { ... }` block form runs its body in the
/// enclosing scope.
#[test]
fn test_declare_block_runs_body() {
    let out = compile_and_run("<?php declare(ticks=1) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

/// Verifies PHP's alternative declare syntax compiles and executes its body.
#[test]
fn test_declare_alternative_syntax_runs_body() {
    let out = compile_and_run("<?php declare(ticks=1): echo \"alternative\"; enddeclare;");
    assert_eq!(out, "alternative");
}

/// Verifies PHP's single-statement declare form compiles and executes that statement.
#[test]
fn test_declare_single_statement_runs_body() {
    let out = compile_and_run("<?php declare(ticks=1) echo \"single\";");
    assert_eq!(out, "single");
}

/// Verifies empty and nested declare bodies preserve normal enclosing-scope execution.
#[test]
fn test_declare_empty_and_nested_bodies() {
    let out = compile_and_run(
        "<?php declare(ticks=1) {} declare(ticks=1): declare(ticks=1) echo \"nested\"; enddeclare;",
    );
    assert_eq!(out, "nested");
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

// --- Superglobals ---

/// Verifies `$_GET` is recognized as a superglobal (no undefined-variable error) and that
/// reading it as an assoc array via null-coalescing works end-to-end.
///
/// Off-web, `$_GET` is seeded as an empty assoc array. `$_GET['x']` on the missing key
/// returns null, and `?? 'none'` yields "none". The primary assertion is that it COMPILES
/// without "Undefined variable: $_GET" — i.e. the superglobal is type-recognized at
/// top level.
#[test]
fn superglobal_get_is_recognized() {
    let out = compile_and_run("<?php $v = $_GET['x'] ?? 'none'; echo $v;");
    assert_eq!(out, "none");
}

/// Regression test: assigning an empty array literal `[]` to a superglobal
/// inside a function body must contextualize as a hash (`AssocArray{Str,
/// Mixed}`), not a scalar `Array(Never)`. Without superglobal type seeding
/// in the lowering env, `local_type("_GET")` fell back to `Mixed` and the
/// store emitted a scalar array into the shared global hash slot, crashing
/// the runtime. The fixture writes `[]` then a string key inside a function
/// and echoes the key back to prove the hash store round-trips.
#[test]
fn superglobal_assign_empty_array_in_function() {
    let out = compile_and_run(
        r#"<?php
function reset_and_set() {
    $_GET = [];
    $_GET['k'] = 'v';
    return $_GET['k'];
}
echo reset_and_set();
"#,
    );
    assert_eq!(out, "v");
}

/// BUG-0 regression: a non-`--web` program reading a superglobal before any
/// fresh assignment must not crash. `env_from_signature`
/// (`src/ir_lower/function.rs`) previously seeded every request superglobal
/// (`$_SERVER`/`$_SESSION`/…) with the fixed `AssocArray{Str, Mixed}` type in
/// EVERY function/main env unconditionally, while `.comm` storage for that
/// shared global slot is reserved only under `--web`
/// (`codegen_ir::block_emit::emit_module`). Off-web, `count($_SERVER)`
/// therefore read a never-initialized (zeroed) global as if it were a live
/// Hash pointer and dereferenced a null pointer (`exit 139`). The seeding
/// (and the `global_alias_type` fallback in `ir_lower::context`) is now
/// gated on the `web` flag threaded from `Module::web`, so a CLI build
/// leaves these names untyped (`Mixed`) instead of assuming initialized
/// storage. `compile_and_run` itself asserts the binary exits successfully,
/// so a regression here fails via that assertion, not just the output check.
#[test]
fn bug0_cli_read_of_server_superglobal_before_assignment_does_not_crash() {
    let out = compile_and_run("<?php echo count($_SERVER);");
    assert_eq!(out, "0");
}

/// BUG-0 regression (companion): a non-`--web` program checking `isset()` on
/// `$_SESSION` before any assignment must not crash either. Exercises the
/// same seeded-Hash-vs-uninitialized-storage mismatch as
/// `bug0_cli_read_of_server_superglobal_before_assignment_does_not_crash`,
/// but through `isset()` rather than a builtin call, and for `$_SESSION`
/// (the superglobal the v2 session-support commit was specifically seeding).
#[test]
fn bug0_cli_isset_on_session_superglobal_before_assignment_does_not_crash() {
    let out = compile_and_run(
        r#"<?php
if (isset($_SESSION)) {
    echo "set";
} else {
    echo "unset";
}
"#,
    );
    assert_eq!(out, "unset");
}

/// BUG-7 / A1 regression: `PHP_SESSION_DISABLED`/`PHP_SESSION_NONE`/`PHP_SESSION_ACTIVE`
/// are predefined `ext/session` integer constants (`src/types/session_constants.rs`,
/// `SESSION_INT_CONSTANTS`), registered the same way as `JSON_INT_CONSTANTS` at the
/// name-resolver, checker, and prescan sites. A bare reference folds to the literal
/// with zero runtime `define()` calls.
#[test]
fn session_status_int_constants_resolve_to_php_values() {
    let out = compile_and_run(
        "<?php echo PHP_SESSION_DISABLED . PHP_SESSION_NONE . PHP_SESSION_ACTIVE;",
    );
    assert_eq!(out, "012");
}

/// BUG-7 / A1 companion: `defined('PHP_SESSION_NONE')` must fold to `true` at compile
/// time (matching the JSON_INT_CONSTANTS registry mechanism), and the constants must
/// coexist without error alongside the web prelude's existing
/// `if (!defined('PHP_SESSION_NONE')) { define(...); }` guard (removed later by the
/// prelude owner once every consumer relies on the predefined constants). Because
/// `defined()` on a registry-backed name now folds true unconditionally, the guarded
/// `define()` calls become unreachable and never run, so no "Constant already defined"
/// warning can occur across repeated (request-like) executions of the guard.
#[test]
fn session_status_constants_coexist_with_guarded_define() {
    let out = compile_and_run(
        r#"<?php
if (!defined('PHP_SESSION_NONE')) {
    define('PHP_SESSION_DISABLED', 0);
    define('PHP_SESSION_NONE', 1);
    define('PHP_SESSION_ACTIVE', 2);
}
echo PHP_SESSION_DISABLED . PHP_SESSION_NONE . PHP_SESSION_ACTIVE;
echo "|";
var_dump(defined('PHP_SESSION_NONE'));
"#,
    );
    assert_eq!(out, "012|bool(true)\n");
}

/// A2 regression: `SID` is a predefined string constant (deprecated in PHP 8.4+).
/// elephc's session support is cookie-only, matching PHP's own cookie-mode `SID`,
/// so it always resolves to the empty string. Wired via the same ad-hoc
/// single-string-constant pattern as `PHP_OS` (`src/name_resolver/names.rs`
/// early-return + `is_builtin_global_constant`, `src/types/checker/driver/init.rs`,
/// `src/codegen/prescan.rs`).
#[test]
fn sid_constant_resolves_to_empty_string() {
    let out = compile_and_run(r#"<?php echo "[" . SID . "]"; echo "|"; var_dump(defined('SID'));"#);
    assert_eq!(out, "[]|bool(true)\n");
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
