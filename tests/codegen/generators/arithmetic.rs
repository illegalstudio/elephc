//! Purpose:
//! Generators that yield computed values: arithmetic on parameters and locals, post-increment, division, constant folding, and user-function calls in yield expressions.
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Focuses on values produced inside the generated resume state machine,
//!    including preserved locals and helper-call results.

use crate::support::*;

/// Verifies that integer division in a yield expression is evaluated correctly.
///
/// `intdiv($i, 2) * 2 == $i` is true exactly when `$i` is even, so the generator
/// emits only even numbers. (Plain `/` is float division in PHP — `$i / 2 * 2`
/// always equals `$i` — so `intdiv` is used to exercise truncating division.)
#[test]
fn test_generator_int_division_in_yield_expr() {
    let out = compile_and_run(
        r#"<?php
function gen(int $n) {
    for ($i = 0; $i < $n; $i++) {
        if ($i == intdiv($i, 2) * 2) {
            yield $i;
        }
    }
}
foreach (gen(10) as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 2 4 6 8 ");
}

/// Verifies that function parameters are preserved across multiple yields.
#[test]
fn test_generator_yields_int_parameters() {
    let out = compile_and_run(
        r#"<?php
function gen(int $a, int $b) {
    yield $a;
    yield $b;
    yield $a;
}
foreach (gen(7, 9) as $v) {
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "7 9 7 ");
}

/// Verifies that constant-folded arithmetic expressions are inlined at compile time.
#[test]
fn test_generator_yields_const_folded_arithmetic() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    yield 1 + 2;
    yield 3 * 4;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "3 12 ");
}

/// Verifies that arithmetic on parameters is evaluated at yield time, not at resume time.
#[test]
fn test_generator_yields_param_arithmetic() {
    let out = compile_and_run(
        r#"<?php
function gen(int $a) {
    yield $a + 1;
    yield $a * 2;
}
foreach (gen(10) as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "11 20 ");
}

/// Verifies that local variables are preserved across yields and can be mutated between yields.
#[test]
fn test_generator_local_variable_across_yields() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $x = 5;
    yield $x;
    $x = 10;
    yield $x;
    $x = 99;
    yield $x;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "5 10 99 ");
}

/// Verifies that a counter using `$i = $i + 1` arithmetic assignment survives across yields.
#[test]
fn test_generator_counter_with_arithmetic_assignment() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $i = 0;
    yield $i;
    $i = $i + 1;
    yield $i;
    $i = $i + 1;
    yield $i;
    $i = $i + 1;
    yield $i;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 3 ");
}

/// Verifies that post-increment on a local variable works correctly across yields.
#[test]
fn test_generator_post_increment_local() {
    let out = compile_and_run(
        r#"<?php
function gen() {
    $i = 10;
    yield $i;
    $i++;
    yield $i;
    $i++;
    yield $i;
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "10 11 12 ");
}

/// Verifies that a user-defined function can be called in a yield expression.
///
/// The call is evaluated into a register (x0 for int return), then the result is
/// boxed into the generator's Mixed yield cell. Parameters are passed in registers.
#[test]
fn test_generator_calls_user_function() {
    let out = compile_and_run(
        r#"<?php
function helper(int $x): int { return $x + 100; }
function gen() {
    yield helper(1);
    yield helper(2);
    yield helper(3);
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "101 102 103 ");
}

/// Verifies that a user function with 7 stack-passed arguments can be called in a yield expression.
///
/// Verifies stack argument materialization for user functions with parameters beyond
/// the 8-register ABI limit.
#[test]
fn test_generator_calls_user_function_with_stack_passed_arg() {
    let out = compile_and_run(
        r#"<?php
function sum7(int $a, int $b, int $c, int $d, int $e, int $f, int $g): int {
    return $a + $b + $c + $d + $e + $f + $g;
}
function gen() {
    yield sum7(1, 2, 3, 4, 5, 6, 7);
}
foreach (gen() as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "28");
}

/// Verifies that a stack-passed integer parameter is preserved across the generator's lazy start.
///
/// On x86_64 SysV (6 integer arg registers) the 7th parameter `$g` is passed on the caller stack;
/// the generator constructor must spill it from the caller stack into the boxed `start_args`, and
/// the entry wrapper must forward it back through the call stack to the body. Mixing `$g` with a
/// register-passed `$a` checks both paths line up.
#[test]
fn test_generator_stack_passed_parameter_survives_in_frame() {
    let out = compile_and_run(
        r#"<?php
function gen(int $a, int $b, int $c, int $d, int $e, int $f, int $g) {
    yield $g;
    yield $a + $g;
}
foreach (gen(1, 2, 3, 4, 5, 6, 7) as $v) {
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "7 8 ");
}

/// Verifies that a stack-passed string parameter keeps its pointer and length across the lazy start.
///
/// A string consumes two integer argument registers, so after five integer parameters fill
/// rdi–r8 on x86_64 SysV the `string $s` no longer fits and is passed on the caller stack as a
/// pointer/length pair. The constructor must spill both words and the entry wrapper must forward
/// the pair back, so concatenating `$s` proves the secondary (length) word is not lost.
#[test]
fn test_generator_stack_passed_string_parameter() {
    let out = compile_and_run(
        r#"<?php
function gen(int $a, int $b, int $c, int $d, int $e, string $s) {
    yield $s . "!";
    yield $a . $s;
}
foreach (gen(1, 2, 3, 4, 5, "hi") as $v) {
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "hi! 1hi ");
}

/// Verifies that a generator with more parameters than the coroutine's start-argument
/// capacity is rejected with a clear diagnostic rather than silently corrupting adjacent
/// fiber fields. Parameters are boxed into the fixed `FIBER_START_ARGS_MAX` (7) slots, so
/// an 8th parameter has nowhere to live.
#[test]
#[should_panic(expected = "generators support at most 7")]
fn test_generator_too_many_parameters_is_rejected() {
    compile_and_run(
        r#"<?php
function gen(int $a, int $b, int $c, int $d, int $e, int $f, int $g, int $h) {
    yield $h;
}
foreach (gen(1, 2, 3, 4, 5, 6, 7, 8) as $v) { echo $v; }
"#,
    );
}

/// Verifies that a user function call embedded in arithmetic is correctly evaluated within a yield expression.
#[test]
fn test_generator_calls_user_function_in_arithmetic() {
    let out = compile_and_run(
        r#"<?php
function dbl(int $x): int { return $x * 2; }
function gen() {
    $i = 1;
    while ($i < 5) {
        yield dbl($i) + 10;
        $i++;
    }
}
foreach (gen() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "12 14 16 18 ");
}

/// Verifies that both explicit keys and parameter-derived values are correctly placed in yielded pairs.
#[test]
fn test_generator_combined_param_key_and_value() {
    let out = compile_and_run(
        r#"<?php
function gen(int $start, int $end) {
    yield $start => 1;
    yield $end => 2;
    yield 99 => $start;
}
foreach (gen(10, 20) as $k => $v) {
    echo $k;
    echo "->";
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "10->1 20->2 99->10 ");
}
