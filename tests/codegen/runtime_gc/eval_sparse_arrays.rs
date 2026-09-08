//! Purpose:
//! Verifies boxed eval arrays retain absent numeric keys through mutation and COW.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Removing an interior element must not recreate it as a dense null slot.
//! - Opaque eval exercises the native boxed-array bridge on every executable target.

use crate::support::*;

/// Unset retains sparse keys, distinguishes missing from null, and leaves copied arrays intact.
#[test]
fn test_core_eval_unset_preserves_sparse_keys_and_cow() {
    let source = r#"<?php
$source = '$array = [10, 20, 30]; $copy = $array;
unset($array[1]);
echo count($array), ":", implode(",", array_keys($array)), ":";
echo array_key_exists(1, $array) ? "bad" : "missing";
echo "|", implode(",", $copy), "|";
$array[1] = null;
echo count($array), ":", implode(",", array_keys($array)), ":";
echo array_key_exists(1, $array) ? "null-present" : "bad";' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "2:0,2:missing|10,20,30|3:0,2,1:null-present");
}

/// Sparse writes do not allocate dense gaps, including through native Mixed property writeback.
#[test]
fn test_core_eval_sparse_array_writeback_preserves_property_owners() {
    let source = r#"<?php
class SparseNativeOwner { public mixed $items = null; }
$source = '$owner = new SparseNativeOwner();
$owner->items = [10, 20, 30];
unset($owner->items[1]);
$copy = $owner->items;
$owner->items[1000000] = 40;
echo count($owner->items), ":", implode(",", array_keys($owner->items)), "|";
echo count($copy), ":", implode(",", array_keys($copy));
unset($owner, $copy);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "3:0,2,1000000|2:0,2");
}
