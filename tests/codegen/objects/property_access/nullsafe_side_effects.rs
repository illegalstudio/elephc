//! Purpose:
//! Regression tests for nullsafe side-effect ordering around chained calls and coalescing.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures assert that nullsafe short-circuiting skips later argument effects
//!   while live branches still evaluate those effects exactly once.

use super::*;

/// Verifies chained nullsafe access with `??` skips argument side effects after
/// a null receiver, then evaluates the live branch argument exactly once.
#[test]
fn test_nullsafe_chain_coalesce_orders_method_argument_side_effects() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public ?Leaf $leaf = null;
}
class Leaf {
    public function run(string $s): string {
        return $s;
    }
}
function noisy(): string {
    echo "N";
    return "ok";
}

$none = null;
$box = new Box();
echo $none?->leaf?->run(noisy()) ?? "x";
echo "\n";
$box->leaf = new Leaf();
echo $box?->leaf?->run(noisy()) ?? "x";
"#,
    );
    assert_eq!(out, "x\nNok");
}
