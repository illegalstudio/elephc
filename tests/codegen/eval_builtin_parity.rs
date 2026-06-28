//! Purpose:
//! End-to-end regressions for builtin lookup parity across AOT code and eval.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_builtin_parity` through Rust's test harness.
//!
//! Key details:
//! - Fixtures verify `function_exists()` and namespaced builtin fallback before
//!   and after eval has introduced dynamic symbols.

use crate::support::compile_and_run;

/// Verifies AOT builtin lookup stays case-insensitive without eval being present.
#[test]
fn test_aot_function_exists_builtin_case_insensitive_without_eval() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("strlen") ? "S" : "s";
echo function_exists("STRLEN") ? "C" : "c";
echo function_exists("StRlEn") ? "M" : "m";
"#,
    );

    assert_eq!(out, "SCM");
}

/// Verifies eval declarations extend function lookup without hiding existing AOT builtins.
#[test]
fn test_function_exists_sees_builtins_and_eval_declared_functions_after_eval() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("eval_declared_lookup") ? "b" : "B";
eval('function eval_declared_lookup() { return "D"; }');
echo function_exists("strlen") ? "S" : "s";
echo function_exists("STRLEN") ? "C" : "c";
echo function_exists("eval_declared_lookup") ? eval_declared_lookup() : "d";
"#,
    );

    assert_eq!(out, "BSCD");
}

/// Verifies eval builtin lookup remains case-insensitive after eval is active.
#[test]
fn test_eval_function_exists_builtin_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
eval('echo function_exists("strlen") ? "S" : "s";
echo function_exists("STRLEN") ? "C" : "c";
echo function_exists("StRlEn") ? "M" : "m";');
"#,
    );

    assert_eq!(out, "SCM");
}

/// Verifies namespaced function calls fall back to builtins in AOT and eval code.
#[test]
fn test_namespaced_calls_fall_back_to_builtin_before_and_after_eval() {
    let out = compile_and_run(
        r#"<?php
namespace EvalBuiltinParity;
echo strlen("abc");
eval('namespace EvalBuiltinParity;
echo strlen("de");
echo STRLEN("fghi");');
"#,
    );

    assert_eq!(out, "324");
}
