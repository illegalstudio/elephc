//! Purpose:
//! Integration tests for the EIR peephole pass driven by the fixed-point pass
//! driver. Fixtures use the runtime `$argc` value and side-effecting calls so
//! the targeted IR constructs survive AST-level folding and reach EIR, where the
//! peephole patterns must rewrite them while preserving observable behavior.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `$argc` is 1 when the compiled binary runs with no CLI arguments.
//! - Scalar load/store forwarding is the pattern that fires most directly from
//!   source (`$x = $expr; ... $x`); the other patterns are covered exhaustively
//!   by the `src/ir_passes/tests/peephole_test.rs` unit tests. These end-to-end
//!   fixtures prove the rewrites never miscompile real programs.

use super::*;

/// `$x = $argc; echo $x;` forwards the load of `$x` to the stored value at the
/// IR level and still prints the value.
#[test]
fn test_peephole_load_after_store_forwards() {
    let out = compile_and_run("<?php $x = $argc; echo $x;");
    assert_eq!(out, "1");
}

/// `$x = $x;` storing back the just-loaded value is a dead store; the program
/// still prints the original value.
#[test]
fn test_peephole_self_store_preserves_value() {
    let out = compile_and_run("<?php $x = $argc; $x = $x; echo $x;");
    assert_eq!(out, "1");
}

/// Forwarding the load of a value assigned from a side-effecting call must keep
/// the call: `f()` still runs (printing `x`) and its return flows through.
#[test]
fn test_peephole_forwarding_preserves_call_side_effects() {
    let out = compile_and_run(
        "<?php function f() { echo \"x\"; return 5; } $r = f(); echo $r;",
    );
    assert_eq!(out, "x5");
}

/// A scalar reused through two locals still reads the same value after
/// forwarding: `$n + $m` is `$argc + $argc`.
#[test]
fn test_peephole_scalar_reuse_through_locals() {
    let out = compile_and_run("<?php $n = $argc; $m = $n; echo $n + $m;");
    assert_eq!(out, "2");
}

/// Load forwarding into a string concat must keep the concat and its cleanup
/// balanced: the runtime value is still interpolated correctly.
#[test]
fn test_peephole_forwarding_into_concat_is_balanced() {
    let out = compile_and_run("<?php $n = $argc; echo \"v\" . $n . \"!\";");
    assert_eq!(out, "v1!");
}

/// A local passed by reference must not be forwarded: the load after the call
/// has to read the value the callee mutated, not the pre-call stored value.
#[test]
fn test_peephole_does_not_forward_across_by_ref_mutation() {
    let out = compile_and_run(
        "<?php function inc(&$x) { $x = $x + 1; } $n = $argc; inc($n); echo $n;",
    );
    assert_eq!(out, "2");
}

/// A local whose load feeds a by-reference argument must stay a real load so the
/// backend can take its address; forwarding it to a constant would not compile.
#[test]
fn test_peephole_by_ref_arg_from_constant_local_compiles() {
    let out = compile_and_run(
        "<?php function inc(&$x) { $x = $x + 1; } $n = $argc; inc($n); inc($n); echo $n;",
    );
    assert_eq!(out, "3");
}
