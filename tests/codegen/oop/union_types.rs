//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP union types, including union typed local gettype and reassignment, nullable typed local null coalesce, and union typed local truthiness dispatch.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that gettype() reflects the narrowest runtime type of a union-typed local,
/// and that reassignment to an alternate union member updates the reported type.
/// Fixture: `int|string $value = 1` → gettype returns "integer", then `$value = "two"` → gettype returns "string".
#[test]
fn test_union_typed_local_gettype_and_reassignment() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = 1;
    echo gettype($value);
    echo ":";
    $value = "two";
    echo gettype($value);
    echo ":";
    echo $value;
}

demo();
"#,
    );
    assert_eq!(out, "integer:string:two");
}

/// Verifies null coalesce on a nullable-typed local: fallback is used when the value is null,
/// and the actual value is returned after assignment to a non-null int.
/// Fixture: `?int $value = null` → `?? 41` yields 41; then `$value = 1` → `?? 41` yields 1.
#[test]
fn test_nullable_typed_local_null_coalesce() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    ?int $value = null;
    echo $value ?? 41;
    $value = 1;
    echo $value ?? 41;
}

demo();
"#,
    );
    assert_eq!(out, "411");
}

/// Verifies truthiness dispatch for a union-typed local: string "0" is falsy, int 7 is truthy.
/// Regression: ensures the codegen emits correct branch logic for both string and int payloads.
#[test]
fn test_union_typed_local_truthiness_dispatch() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = "0";
    if ($value) {
        echo 1;
    } else {
        echo 0;
    }
    $value = 7;
    if ($value) {
        echo 1;
    } else {
        echo 0;
    }
}

demo();
"#,
    );
    assert_eq!(out, "01");
}

/// Verifies empty() dispatch for a union-typed local: string "0" is empty, string "7" is not.
/// Regression: ensures empty() correctly distinguishes falsy-but-non-empty strings from empty strings.
#[test]
fn test_union_typed_local_empty_dispatch() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = "0";
    echo empty($value) ? 1 : 0;
    $value = "7";
    echo empty($value) ? 1 : 0;
}

demo();
"#,
    );
    assert_eq!(out, "10");
}
