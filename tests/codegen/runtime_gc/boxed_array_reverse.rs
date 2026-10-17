//! Purpose:
//! Covers reversing a concrete Mixed-valued hash across alias and ownership boundaries.
//!
//! Called from:
//! - The runtime GC codegen integration module.
//!
//! Key details:
//! - The concrete hash is boxed for `__rt_array_reverse_boxed`; the borrowed source keeps its
//!   owner and every temporary is released.

use crate::support::*;

/// A concrete Mixed-valued hash reverses with and without preserved keys, keeps a copied alias
/// intact, and releases every owner.
#[test]
fn test_core_concrete_mixed_hash_reverse_balances() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 4; $i++) {
    $items = ["x" => 1, "y" => str_repeat('s', 12), "z" => 3];
    $copy = $items;
    $plain = array_reverse($items);
    $kept = array_reverse($items, true);
    echo implode(',', $plain), '|', $kept['y'] === $items['y'] ? 'same' : 'diff', '|', implode(',', $copy), '|';
    unset($items, $copy, $plain, $kept);
}
echo 'done';
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "3,ssssssssssss,1|same|1,ssssssssssss,3|".repeat(4) + "done",
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
