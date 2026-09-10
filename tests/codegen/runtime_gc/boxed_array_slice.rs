//! Purpose:
//! Verifies boxed indexed slices preserve source storage and balance private conversion owners.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Mixed parameters exercise runtime element conversion; heap checks include discarded slices.

use crate::support::*;

/// Reading slices through a Mixed parameter leaves concrete aliases and later writes independent.
#[test]
fn test_core_boxed_array_slice_preserves_concrete_sources_and_aliases() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function sliceSnapshot(mixed $values, int $offset, ?int $length = null): array {
    return array_slice($values, $offset, $length);
}
$values = [10, 20, 30, 40];
$alias = $values;
$middle = sliceSnapshot($values, 1, 2);
$tail = sliceSnapshot($values, -2);
echo implode(",", $values), "|", implode(",", $alias), "|";
$values[1] = 99;
unset($values, $alias);
echo implode(",", $middle), "|", implode(",", $tail);
unset($middle, $tail);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "10,20,30,40|10,20,30,40|20,30|30,40", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Selected strings outlive the original source while empty and discarded conversions leave no owners.
#[test]
fn test_core_boxed_array_slice_releases_private_string_snapshots() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function stringSlices(mixed $values): array {
    for ($i = 0; $i < 8; $i++) {
        array_slice($values, 0, -1);
        array_slice($values, 9);
    }
    return array_slice($values, 1, 2);
}
$values = [str_repeat("a", 3), str_repeat("b", 3), str_repeat("c", 3), str_repeat("d", 3)];
$slice = stringSlices($values);
echo implode(",", $values), "|";
unset($values);
echo implode(",", $slice);
unset($slice);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "aaa,bbb,ccc,ddd|bbb,ccc", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Result object owners survive source retirement and release exactly once after the last slice.
#[test]
fn test_core_boxed_array_slice_transfers_selected_object_lifetimes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SliceLifetime {
    public function __construct(public int $id) {}
    public function __destruct() { echo "drop:", $this->id, "|"; }
}
function objectSlice(mixed $values): array { return array_slice($values, 1, 2); }
$values = [new SliceLifetime(1), new SliceLifetime(2), new SliceLifetime(3)];
$slice = objectSlice($values);
unset($values);
echo $slice[0]->id, ":", $slice[1]->id, "|";
unset($slice);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "drop:1|2:3|drop:2|drop:3|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
