//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, named arguments variadics, including named arguments unknown variadic named args keep string keys, named arguments variadic mixes positional and named extra args, and named arguments variadic after long spread keeps tail and named args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_named_arguments_unknown_variadic_named_args_keep_string_keys() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(head: 1, extra: 2);
"#,
    );
    assert_eq!(out, "extra=2;");
}

#[test]
fn test_named_arguments_variadic_mixes_positional_and_named_extra_args() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(1, 2, extra: 3);
"#,
    );
    assert_eq!(out, "0=2;extra=3;");
}

#[test]
fn test_named_arguments_variadic_after_long_spread_keeps_tail_and_named_args() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    echo "head=" . $head . ";";
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(...[1, 2, 3], extra: 4);
"#,
    );
    assert_eq!(out, "head=1;0=2;1=3;extra=4;");
}

#[test]
fn test_named_arguments_variadic_after_exact_spread_keeps_named_arg() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    echo "head=" . $head . ";";
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
show(...[1], extra: 4);
"#,
    );
    assert_eq!(out, "head=1;extra=4;");
}
