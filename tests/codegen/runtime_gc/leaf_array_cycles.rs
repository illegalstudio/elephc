//! Purpose:
//! Verifies cycle collection retires arrays that cannot themselves contain graph edges.
//!
//! Called from:
//! - Executable runtime GC coverage on all supported host targets.
//!
//! Key details:
//! - Mixed properties preserve empty, integer, float, boolean and string array layouts.
//! - An external alias must retain its payload while the unreachable cycle releases its owner.

use crate::support::*;

/// Empty and scalar array leaves release normally when their owning cycle is swept.
#[test]
fn test_cycle_collection_releases_non_graph_array_leaves() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class LeafArrayCycleOwner {
    public mixed $peer = null;
    public mixed $payload = null;
}
function createLeafArrayCycle(mixed $payload): void {
    $owner = new LeafArrayCycleOwner();
    $owner->peer = $owner;
    $owner->payload = $payload;
}
createLeafArrayCycle([]);
createLeafArrayCycle([1, 2]);
createLeafArrayCycle([1.5, 2.5]);
createLeafArrayCycle([false, true]);
createLeafArrayCycle([str_repeat('x', 5), 'text']);
$kept = [str_repeat('alive', 1)];
createLeafArrayCycle($kept);
gc_collect_cycles();
echo $kept[0], '|done';
unset($kept);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "alive|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
