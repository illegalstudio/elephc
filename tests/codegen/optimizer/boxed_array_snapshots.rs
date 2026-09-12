//! Purpose:
//! Verifies boxed array projections remain snapshots across mutations and loop iterations.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - Runtime argc and a by-reference PHP array prevent literal-only fixtures from hiding heap reads.

use super::*;

/// Merge snapshots must not be reused across writes to a by-reference array parameter.
#[test]
fn test_boxed_array_merge_snapshots_follow_runtime_mutations() {
    let source = r#"<?php
function inspectMergeSnapshots(array &$items, int $seed): void {
    for ($i = 0; $i < 3; $i++) {
        $items[0] = $seed + $i;
        $before = array_merge($items, [9]);
        $items[0] = $seed + $i + 10;
        $after = array_merge($items, [9]);
        echo $before[0], ":", $after[0], "|";
    }
}
$items = [0];
inspectMergeSnapshots($items, $argc);
echo $items[0];
"#;
    assert_eq!(compile_and_run(source), "1:11|2:12|3:13|13");
}

/// Reversal and values extraction must read each iteration's current contents without changing older results.
#[test]
fn test_boxed_array_projection_snapshots_follow_runtime_mutations() {
    let source = r#"<?php
function inspectArraySnapshots(array &$items, int $seed): void {
    for ($i = 0; $i < 3; $i++) {
        $items[1] = $seed + $i;
        $before = array_reverse($items);
        $values = array_values($items);
        $items[1] = $seed + $i + 10;
        $after = array_reverse($items);
        echo $before[0], ":", $values[1], ":", $after[0], "|";
    }
}
$items = [0, 0];
inspectArraySnapshots($items, $argc);
echo $items[1];
"#;
    assert_eq!(compile_and_run(source), "1:1:11|2:2:12|3:3:13|13");
}
