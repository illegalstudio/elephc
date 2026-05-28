//! Purpose:
//! Integration tests for optimizer-sensitive codegen coverage of constant folding
//! through the PHP 8.5 pipe operator. Cover whitelisted pure builtins (`strlen`,
//! `intval`, `floatval`, `abs`, `strtoupper`, `strtolower`, `strrev`) plus the
//! regressions where folding must stay disabled (user functions, non-ASCII strings).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions both
//!   compare stdout and (when feasible) probe the emitted assembly for the
//!   absence of the runtime helper that the fold should have eliminated.

use super::*;

/// Verifies that a pipe with `strlen` on a literal string is folded so the
/// `__rt_strlen` call does not appear in user assembly.
#[test]
fn test_constant_folding_pipe_strlen_eliminates_runtime_call() {
    let dir = make_cli_test_dir("elephc_constant_folding_pipe_strlen");
    let (user_asm, _runtime_asm, libs) = compile_source_to_asm_with_options(
        r#"<?php echo "hello" |> strlen(...);"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        !user_asm.contains("__rt_strlen"),
        "pipe-folded strlen should leave no runtime strlen call:\n{}",
        user_asm
    );
    assert_eq!(
        assemble_and_run(&user_asm, get_runtime_obj(), &dir, &libs, &default_link_paths(), &[]),
        "5"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that `strtoupper` on a literal ASCII string is folded via pipe.
#[test]
fn test_constant_folding_pipe_strtoupper() {
    let out = compile_and_run(r#"<?php echo "hello" |> strtoupper(...);"#);
    assert_eq!(out, "HELLO");
}

/// Verifies constant folding through a two-link pipe chain: `strtoupper` then `strrev`,
/// both operating on literal strings.
#[test]
fn test_constant_folding_pipe_chain_strtoupper_strrev() {
    let out = compile_and_run(r#"<?php echo "hello" |> strtoupper(...) |> strrev(...);"#);
    assert_eq!(out, "OLLEH");
}

/// Verifies that `strtolower` on a literal ASCII string is folded via pipe.
#[test]
fn test_constant_folding_pipe_strtolower() {
    let out = compile_and_run(r#"<?php echo "HELLO" |> strtolower(...);"#);
    assert_eq!(out, "hello");
}

/// Verifies that `intval` on a literal float is folded via pipe.
#[test]
fn test_constant_folding_pipe_intval_from_float() {
    let out = compile_and_run("<?php echo 3.7 |> intval(...);");
    assert_eq!(out, "3");
}

/// Verifies that `floatval` on a literal integer is folded via pipe.
#[test]
fn test_constant_folding_pipe_floatval_from_int() {
    let out = compile_and_run("<?php echo 5 |> floatval(...);");
    assert_eq!(out, "5");
}

/// Verifies that `abs` on a literal negative integer is folded via pipe.
#[test]
fn test_constant_folding_pipe_abs_negative_int() {
    let out = compile_and_run("<?php echo -7 |> abs(...);");
    assert_eq!(out, "7");
}

/// Verifies that a pipe whose target is a user-defined function is NOT folded.
/// User functions may have observable side effects or depend on global state.
#[test]
fn test_constant_folding_pipe_user_function_not_folded() {
    // User-defined targets must NOT be folded — the function might rely on
    // global state, observable side effects, or future refinements.
    let out = compile_and_run(
        r#"<?php
function double(int $n): int { return $n * 2; }
echo 5 |> double(...);
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies that a pipe with `strtoupper` on a non-ASCII literal string is NOT folded.
/// Rust's `to_uppercase` diverges from PHP for non-ASCII input; the compiler must
/// fall back to the runtime path so output matches PHP (e.g., "café" → "CAFé").
#[test]
fn test_constant_folding_pipe_non_ascii_strtoupper_not_folded() {
    // PHP's `strtoupper` only uppercases ASCII a-z; non-ASCII bytes pass through
    // unchanged. Rust's `to_uppercase` would expand many Unicode lowercase chars
    // and diverge. The fold rejects non-ASCII input and falls back to the
    // runtime path, which matches PHP.
    let out = compile_and_run(r#"<?php echo "café" |> strtoupper(...);"#);
    assert_eq!(out, "CAFé");
}

/// Verifies that a pipe with a non-literal LHS (variable) is NOT folded. The runtime
/// call must still execute and produce the correct result.
#[test]
fn test_constant_folding_pipe_runtime_value_not_folded() {
    // No literal LHS → no fold; runtime call still produces the right answer.
    let out = compile_and_run(
        r#"<?php
$s = "world";
echo $s |> strlen(...);
"#,
    );
    assert_eq!(out, "5");
}

// --- Extended whitelist (commit 11): type predicates, gettype, math, string ASCII transforms ---

/// Verifies that `is_int` on a literal integer is folded via pipe and returns true.
#[test]
fn test_constant_folding_pipe_is_int() {
    let out = compile_and_run("<?php echo (int)(5 |> is_int(...));");
    assert_eq!(out, "1");
}

/// Verifies that `is_string` on a literal integer is folded via pipe and returns false.
#[test]
fn test_constant_folding_pipe_is_string_on_int() {
    let out = compile_and_run("<?php echo (int)(5 |> is_string(...));");
    assert_eq!(out, "0");
}

/// Verifies that `is_numeric` on a literal integer, float, and boolean is folded via
/// pipe with correct results (int→true, float→true, bool→false).
#[test]
fn test_constant_folding_pipe_is_numeric_int_yes_float_yes_bool_no() {
    let out = compile_and_run(
        r#"<?php
echo (int)(5 |> is_numeric(...));
echo (int)(3.14 |> is_numeric(...));
echo (int)(true |> is_numeric(...));
"#,
    );
    assert_eq!(out, "110");
}

/// Verifies that `gettype` on a literal integer is folded via pipe and returns "integer".
#[test]
fn test_constant_folding_pipe_gettype_int() {
    let out = compile_and_run(r#"<?php echo 5 |> gettype(...);"#);
    assert_eq!(out, "integer");
}

/// Verifies that `gettype` on a literal string is folded via pipe and returns "string".
#[test]
fn test_constant_folding_pipe_gettype_string() {
    let out = compile_and_run(r#"<?php echo "hi" |> gettype(...);"#);
    assert_eq!(out, "string");
}

/// Verifies that `floor` on a literal float is folded via pipe.
#[test]
fn test_constant_folding_pipe_floor_on_float() {
    let out = compile_and_run("<?php echo 3.7 |> floor(...);");
    assert_eq!(out, "3");
}

/// Verifies that `ceil` on a literal float is folded via pipe.
#[test]
fn test_constant_folding_pipe_ceil_on_float() {
    let out = compile_and_run("<?php echo 3.2 |> ceil(...);");
    assert_eq!(out, "4");
}

/// Verifies that `round` on a literal float is folded via pipe.
#[test]
fn test_constant_folding_pipe_round_on_float() {
    let out = compile_and_run("<?php echo 3.6 |> round(...);");
    assert_eq!(out, "4");
}

/// Verifies that `ucfirst` on a literal ASCII string is folded via pipe.
#[test]
fn test_constant_folding_pipe_ucfirst() {
    let out = compile_and_run(r#"<?php echo "hello" |> ucfirst(...);"#);
    assert_eq!(out, "Hello");
}

/// Verifies that `lcfirst` on a literal ASCII string is folded via pipe.
#[test]
fn test_constant_folding_pipe_lcfirst() {
    let out = compile_and_run(r#"<?php echo "HELLO" |> lcfirst(...);"#);
    assert_eq!(out, "hELLO");
}

/// Verifies that `trim` on a literal string with default whitespace padding is folded via pipe.
#[test]
fn test_constant_folding_pipe_trim_default_whitespace() {
    let out = compile_and_run(r#"<?php echo "  hello  " |> trim(...);"#);
    assert_eq!(out, "hello");
}

/// Verifies that `trim` on a literal string containing tab and newline characters
/// is folded via pipe (escapes produce actual tab/newline bytes).
#[test]
fn test_constant_folding_pipe_trim_with_tabs_and_newlines() {
    let out = compile_and_run("<?php echo \"\\t\\nhi\\n\\t\" |> trim(...);");
    assert_eq!(out, "hi");
}

// --- Inline closure literal in pipe (commit 14) ---

/// Verifies that an arrow closure is inlined into a literal pipe LHS so no `_fcc_`
/// deferred wrapper appears in user assembly, and the arithmetic result is correct.
#[test]
fn test_inline_pipe_arrow_closure_arithmetic() {
    // `5 |> (fn($v) => $v * 2 + 1)` inlines to `5 * 2 + 1` which then folds to `11`.
    let dir = make_cli_test_dir("elephc_inline_pipe_arrow_arithmetic");
    let (user_asm, _, libs) = compile_source_to_asm_with_options(
        "<?php echo 5 |> (fn($v) => $v * 2 + 1);",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        !user_asm.contains("_fcc_"),
        "inlined arrow closure should not generate a deferred FCC wrapper:\n{}",
        user_asm
    );
    assert_eq!(
        assemble_and_run(&user_asm, get_runtime_obj(), &dir, &libs, &default_link_paths(), &[]),
        "11"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that a plain (non-arrow) closure with a single return statement is
/// also eligible for inlining via pipe.
#[test]
fn test_inline_pipe_full_closure_single_return() {
    // A plain `function($v) { return ... }` is eligible too as long as the body
    // is a single return statement.
    let out = compile_and_run(
        r#"<?php echo 3 |> (function($v) { return $v * $v; });"#,
    );
    assert_eq!(out, "9");
}

/// Verifies that a pipe whose closure body never references the pipe parameter
/// replaces the entire pipe with the body literal. Safe when the LHS is a trivial literal
/// so its evaluation cannot have observable side effects.
#[test]
fn test_inline_pipe_unused_parameter_drops_value() {
    // The body never uses `$v`; the inline replaces the entire pipe with the
    // body expression. Only safe when `$v` is a literal so its evaluation
    // cannot have observable effects.
    let out = compile_and_run("<?php echo 5 |> (fn($v) => 42);");
    assert_eq!(out, "42");
}

/// Verifies that a pipe whose closure uses the parameter twice (after LHS folds to
/// a trivial literal) folds correctly to 14. Regression: ensures the fold path does
/// not incorrectly substitute when the same variable is used multiple times.
#[test]
fn test_inline_pipe_skipped_for_non_trivial_value_used_twice() {
    // `5 + 2 |> (fn($v) => $v + $v)` — the value would fold to `7` first, then
    // the body uses `$v` twice. With trivial-literal substitution allowed, the
    // result becomes `7 + 7 = 14`. Non-regression on this fold path.
    let out = compile_and_run("<?php echo 5 + 2 |> (fn($v) => $v + $v);");
    assert_eq!(out, "14");
}

/// Verifies that a closure with a `use` capture clause is NOT inlined, because
/// evaluating the captured variable at the call site would be incorrect.
#[test]
fn test_inline_pipe_skipped_for_closure_with_capture() {
    // A closure with a `use ($w)` clause may evaluate `$w` at the call site;
    // inlining would be incorrect. The pipe must keep its closure wrapper.
    let out = compile_and_run(
        r#"<?php
$w = 100;
$cb = function($v) use ($w) { return $v + $w; };
echo 5 |> $cb;
"#,
    );
    assert_eq!(out, "105");
}

/// Verifies that a closure whose body is not a single return statement is NOT
/// inlined. The pipe must keep its closure wrapper.
#[test]
fn test_inline_pipe_skipped_for_multi_statement_closure() {
    // The closure body is not a single return — inlining must back off.
    let out = compile_and_run(
        r#"<?php
$cb = function($v) {
    $w = $v + 1;
    return $w * 2;
};
echo 5 |> $cb;
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that a closure whose body calls a by-ref function is NOT inlined.
/// Aliasing semantics must be preserved; the pipe must use the closure wrapper.
#[test]
fn test_inline_pipe_skipped_for_closure_body_call_by_ref_aliasing() {
    let out = compile_and_run(
        r#"<?php
function setv(&$x): int { $x = 9; return $x; }
$x = 1;
$r = $x |> (fn($v) => setv($v));
echo $x;
echo "|";
echo $r;
"#,
    );
    assert_eq!(out, "1|9");
}
