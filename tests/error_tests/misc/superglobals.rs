//! Purpose:
//! Diagnostic tests for superglobal slot reassignment and adjacent forms. The
//! superglobal slots (`_eir_global_*`) must always hold a Hash pointer; a
//! non-literal indexed array cannot be stored into them because there is no
//! static indexed→hash conversion for arbitrary values.
//!
//! Called from:
//! - `cargo test` through the Rust test harness (wired via the `misc` module).
//!
//! Key details:
//! - Fixtures run the full frontend check pipeline and assert reported
//!   diagnostics via `expect_error`, or acceptance via `check_source`.
//! - Some assertions below are SPECULATIVE (flagged inline and in NOTES); the
//!   applier runs them and records the actual behavior.

use super::*;

/// Top-level reassignment of a non-literal indexed array to a superglobal must
/// be rejected.
#[test]
fn test_error_superglobal_reassign_non_literal_top_level() {
    expect_error(
        r#"<?php
$arr = explode(',', 'a,b');
$_GET = $arr;
"#,
        "cannot reassign $_GET",
    );
}

/// In-function reassignment of a non-literal indexed array to a superglobal
/// must also be rejected: the slot contract is scope-independent.
#[test]
fn test_error_superglobal_reassign_non_literal_function_scope() {
    expect_error(
        r#"<?php
function f(): void {
    $arr = explode(',', 'a,b');
    $_GET = $arr;
}
"#,
        "cannot reassign $_GET",
    );
}

/// Coalesce-assign `??=` against a superglobal is rejected: the RHS array
/// literal type (`array<string, string>`) does not match the superglobal's
/// fixed slot type (`array<string, mixed>`), so the null-coalescing-assignment
/// type check rejects it. Observed message (flipped from the original
/// SPECULATIVE guess of "cannot reassign $_GET", which does not appear for
/// this form): "Type error: null coalescing assignment for $_GET must keep
/// array<string, mixed>, got array<string, string>".
#[test]
fn test_superglobal_coalesce_assign() {
    expect_error(
        r#"<?php
$_GET ??= ['a' => 'x'];
"#,
        "null coalescing assignment for $_GET must keep array<string, mixed>",
    );
}

/// Positional destructuring READ from a superglobal is accepted: list-unpack
/// reads integer key 0 via the hash path (PHP semantics: `null` plus an
/// undefined-key notice when absent), so `[$x] = $_GET` type-checks with
/// `$x` taking the superglobal's value type (`Mixed`).
#[test]
fn test_superglobal_destructure_read_accepted() {
    assert!(check_source(
        r#"<?php
[$x] = $_GET;
"#,
    )
    .is_ok());
}

/// SPECULATIVE: by-reference binding to a superglobal. Best guess is
/// acceptance (it creates an alias rather than overwriting the slot); the
/// applier must verify and report the actual behavior.
#[test]
fn test_superglobal_byref_binding() {
    let res = check_source(
        r#"<?php
$r = &$_GET;
"#,
    );
    assert!(res.is_ok(), "by-ref binding to $_GET should be accepted, got: {:?}", res);
}

/// Moved from `tests/codegen/regressions/superglobal_function_scope.rs`
/// (`test_superglobal_fn_packed_literal`) per the CONTINGENCY in that file's
/// origin notes: a packed (integer-keyed) non-empty array literal assigned to
/// a superglobal from inside a function is rejected by the checker, since the
/// superglobal slot's fixed type is `array<string, mixed>` and there is no
/// static indexed→hash conversion path for a non-empty packed literal target.
/// Observed message: "Type error: cannot reassign $_GET from
/// array<string, mixed> to array<int>".
#[test]
fn test_error_superglobal_fn_packed_literal_rejected() {
    expect_error(
        r#"<?php
function fill(): void { $_GET = [1, 2, 3]; }
"#,
        "cannot reassign $_GET from array<string, mixed> to array<int>",
    );
}
