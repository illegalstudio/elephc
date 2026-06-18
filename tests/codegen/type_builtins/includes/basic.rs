//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins, includes basic, including include basic, require basic, and include with parens.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use super::*;

/// Compiles main.php that includes helper.php and calls the exported function.
#[test]
fn test_include_basic() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'helper.php'; echo greet();"),
            ("helper.php", "<?php function greet() { return \"hello\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "hello");
}

/// Compiles main.php that requires math.php and calls the exported function.
#[test]
fn test_require_basic() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php require 'math.php'; echo add(3, 4);"),
            ("math.php", "<?php function add($a, $b) { return $a + $b; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

/// Verifies `include` with parentheses (functional syntax) works correctly.
#[test]
fn test_include_with_parens() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include('helper.php'); echo greet();"),
            ("helper.php", "<?php function greet() { return \"hi\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "hi");
}

/// Verifies top-level code in an included file executes at the include point, interleaving with main file output.
#[test]
fn test_include_top_level_code() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php echo \"before\"; include 'mid.php'; echo \"after\";",
            ),
            ("mid.php", "<?php echo \"middle\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "beforemiddleafter");
}

/// Verifies `include_once` only executes the file the first time; subsequent calls in the same runtime are no-ops.
#[test]
fn test_include_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include_once 'counter.php';
include_once 'counter.php';
echo $x;
"#,
            ),
            ("counter.php", "<?php $x = 42;"),
        ],
        "main.php",
    );
    assert_eq!(out, "42");
}

/// Verifies `require_once` only executes the file once; function is callable after first load.
#[test]
fn test_require_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require_once 'lib.php';
require_once 'lib.php';
echo double(5);
"#,
            ),
            ("lib.php", "<?php function double($n) { return $n * 2; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "10");
}

/// Verifies constants and functions declared in a `require_once` file are accessible after loading.
#[test]
fn test_require_once_const_visible_inside_included_function() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require_once 'lib.php';
echo LIB_CONST;
echo from_func();
"#,
            ),
            (
                "lib.php",
                r#"<?php
const LIB_CONST = 42;
function from_func() { return LIB_CONST; }
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "4242");
}

/// Verifies `include_once` in a constant-false branch does not claim the file; later `include_once` still executes it.
#[test]
fn test_include_once_skipped_branch_does_not_claim_file() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include_once 'piece.php';
}
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

/// Verifies `include_once` in a loop only executes the file once across all iterations.
#[test]
fn test_include_once_in_loop_executes_file_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 3) {
    include_once 'tick.php';
    $i = $i + 1;
}
"#,
            ),
            ("tick.php", "<?php echo \"tick\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "tick");
}

/// Verifies `require_once` inside a function has globalOnce semantics; subsequent calls do not re-execute.
#[test]
fn test_require_once_in_function_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_piece() {
    require_once 'piece.php';
}
load_piece();
load_piece();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

/// Verifies `require_once` inside a class method has globalOnce semantics across calls on the same instance.
#[test]
fn test_require_once_in_method_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
class Loader {
    public function load() {
        require_once 'piece.php';
    }
}
$loader = new Loader();
$loader->load();
$loader->load();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

/// Verifies `require_once` inside a closure has globalOnce semantics across closure invocations.
#[test]
fn test_require_once_in_closure_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$load = function() {
    require_once 'piece.php';
};
$load();
$load();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

/// Verifies a regular `include` inside a closure marks the file as loaded, causing a later `include_once` to skip it.
#[test]
fn test_regular_include_in_closure_marks_later_include_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$load = function() {
    include 'piece.php';
};
$load();
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

/// Verifies declarations from a regular `include` are visible to a subsequent `include_once` (no duplicate error).
#[test]
fn test_regular_include_marks_later_include_once_declarations() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include 'lib.php';
include_once 'lib.php';
echo seven();
"#,
            ),
            ("lib.php", "<?php function seven() { return 7; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

/// Verifies `include_once` in a constant-false branch does not claim the file; later `include_once` still executes and finds the declaration.
#[test]
fn test_skipped_regular_include_does_not_make_include_once_skip() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'piece.php';
}
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

/// Verifies `return require X;` includes the file (its declarations become available) and the
/// expression yields `1`, the value PHP returns for an include with no explicit `return`.
#[test]
fn test_require_as_return_value() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function boot(): int { return require 'helper.php'; } echo boot(); echo ':'; echo greet();",
            ),
            ("helper.php", "<?php function greet() { return \"hi\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "1:hi");
}

/// Verifies `$x = require X;` includes the file and assigns the include's value `1`.
#[test]
fn test_require_as_assignment_value() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $loaded = require 'math.php'; echo $loaded; echo ':'; echo add(2, 5);",
            ),
            ("math.php", "<?php function add($a, $b) { return $a + $b; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "1:7");
}

/// Verifies `$x = require_once X;` works as a value-position include with the once semantics.
#[test]
fn test_require_once_as_assignment_value() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $a = require_once 'lib.php'; echo $a; echo ':'; echo val();",
            ),
            ("lib.php", "<?php function val() { return 9; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "1:9");
}

/// Verifies that `$x = require X;` captures the included file's top-level `return` value (an
/// integer here), matching PHP's "include returns a value" semantics.
#[test]
fn test_require_value_captures_returned_int() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php $n = require 'num.php'; echo $n + 1;"),
            ("num.php", "<?php return 41;"),
        ],
        "main.php",
    );
    assert_eq!(out, "42");
}

/// Verifies that `return require X;` returns the included file's returned array, readable by key.
#[test]
fn test_require_value_captures_returned_array() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function cfg(): array { return require 'config.php'; } $c = cfg(); echo $c['port'];",
            ),
            ("config.php", "<?php return ['host' => 'localhost', 'port' => 5432];"),
        ],
        "main.php",
    );
    assert_eq!(out, "5432");
}

/// Verifies that an expression-position `require` shares the caller's scope: the included file
/// can READ a variable defined in the caller (PHP runs includes in the calling scope).
#[test]
fn test_require_value_reads_caller_scope() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $base = 10; $v = require 'inc.php'; echo $v;",
            ),
            ("inc.php", "<?php return $base * 2;"),
        ],
        "main.php",
    );
    assert_eq!(out, "20");
}

/// Verifies that an expression-position `require` shares the caller's scope for WRITES: a value
/// assigned to an existing caller variable inside the included file is visible after the include.
#[test]
fn test_require_value_writes_caller_scope() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $acc = 1; $r = require 'inc.php'; echo $acc; echo ':'; echo $r;",
            ),
            ("inc.php", "<?php $acc = $acc + 41; return 7;"),
        ],
        "main.php",
    );
    assert_eq!(out, "42:7");
}

/// Verifies that a variable first assigned inside an expression-position `require` leaks into the
/// caller's scope afterward, matching PHP's shared-scope include semantics.
#[test]
fn test_require_value_new_var_leaks_to_caller() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $r = require 'inc.php'; echo $created; echo ':'; echo $r;",
            ),
            ("inc.php", "<?php $created = 99; return 1;"),
        ],
        "main.php",
    );
    assert_eq!(out, "99:1");
}

/// Verifies that an included file with no top-level `return` yields `1` while still hoisting its
/// declarations globally.
#[test]
fn test_require_value_without_return_yields_one() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php $r = require 'lib.php'; echo $r; echo ':'; echo helper();"),
            ("lib.php", "<?php function helper() { return 'H'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "1:H");
}

/// Verifies `require_once` as a parenthesized comparison operand inside `||` (the Symfony Runtime
/// `public/index.php` pattern: `if (true === (require_once X) || false)`). The autoloader returns
/// a non-`int` value, so `true === <value>` is false and the block is skipped, matching PHP.
#[test]
fn test_require_once_as_comparison_operand_string_return() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php if (true === (require_once 'cfg.php') || false) { echo \"reached \"; } echo \"done\";",
            ),
            ("cfg.php", "<?php return 'prod';"),
        ],
        "main.php",
    );
    assert_eq!(out, "done");
}

/// Verifies the `require_once` comparison-operand pattern enters the block when the include
/// returns `true` (`true === true`), so the deep-hoisted temporary flows into the condition.
#[test]
fn test_require_once_as_comparison_operand_true_return() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php if (true === (require_once 'flag.php') || false) { echo \"reached \"; } echo \"done\";",
            ),
            ("flag.php", "<?php return true;"),
        ],
        "main.php",
    );
    assert_eq!(out, "reached done");
}

/// Verifies a non-`_once` `require` as a comparison operand: no pre-seed is emitted, so the
/// temporary carries the returned value directly and the strict comparison succeeds.
#[test]
fn test_require_as_comparison_operand_int_return() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php if ((require 'num.php') === 5) { echo 'yes'; } else { echo 'no'; }",
            ),
            ("num.php", "<?php return 5;"),
        ],
        "main.php",
    );
    assert_eq!(out, "yes");
}

/// Verifies `$x = require_once X;` captures a non-`int` (string) return value. This was a
/// pre-existing pre-seed type conflict (`int 1` then `string`) that the `mixed`-typed temporary
/// now resolves, so the assignment yields the file's returned string.
#[test]
fn test_require_once_assignment_captures_string_return() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php $x = require_once 'cfg.php'; echo $x;"),
            ("cfg.php", "<?php return 'prod';"),
        ],
        "main.php",
    );
    assert_eq!(out, "prod");
}

/// Verifies `$x = require_once X;` captures an object return (the Composer autoloader case:
/// `return $loader;`), and the object is usable after the assignment.
#[test]
fn test_require_once_assignment_captures_object_return() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php $l = require_once 'loader.php'; echo $l->v;"),
            ("loader.php", "<?php class Loader { public int $v = 7; } return new Loader();"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

/// Verifies `echo require X;` evaluates the include in the current scope and echoes its returned
/// value (single-argument echo, not the multi-argument synthetic form).
#[test]
fn test_echo_require_value() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php echo require 'val.php';"),
            ("val.php", "<?php return 5;"),
        ],
        "main.php",
    );
    assert_eq!(out, "5");
}

/// Verifies `require` used as a function-call argument is evaluated before the call and its value
/// is passed positionally.
#[test]
fn test_require_as_call_argument() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function add10(int $n): int { return $n + 10; } echo add10(require 'val.php');",
            ),
            ("val.php", "<?php return 20;"),
        ],
        "main.php",
    );
    assert_eq!(out, "30");
}

/// Verifies a deep `require` whose path is a `__DIR__`-concatenated expression (the Symfony
/// `__DIR__.'/autoload.php'` form) resolves and runs in the caller's scope.
#[test]
fn test_require_value_with_dir_concat_path() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php $v = require __DIR__ . '/cfg.php'; echo $v;",
            ),
            ("cfg.php", "<?php return 42;"),
        ],
        "main.php",
    );
    assert_eq!(out, "42");
}
