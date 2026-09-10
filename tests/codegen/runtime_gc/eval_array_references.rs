//! Purpose:
//! Verifies eval-to-native PHP array reference slots use boxed storage and balanced owners.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - References may replace packed arrays with hashes while caller aliases retain their old value.

use crate::support::*;

/// Direct, named and dynamic native calls publish boxed array replacements without losing aliases.
#[test]
fn test_core_eval_native_array_references_preserve_layouts_aliases_and_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function replaceEvalArray(array &$items, bool $keyed): void {
    if ($keyed) {
        $items = ["left" => str_repeat("L", 2), "right" => str_repeat("R", 2)];
    } else {
        $items = [str_repeat("A", 2), str_repeat("B", 2)];
    }
}
function keepEvalArray(array &$items): int { return count($items); }
echo eval('$items = ["seed"];
$alias = $items;
replaceEvalArray($items, false);
echo $items[0], $items[1], "|";
$packed = $items;
$fn = "replaceEvalArray";
$fn($items, true);
echo $items["left"], $items["right"], "|";
$keyed = $items;
replaceEvalArray(keyed: false, items: $items);
echo $items[0], $items[1], "|", keepEvalArray($items), "|";
echo $alias[0], "|", $packed[0], $packed[1], "|", $keyed["left"], $keyed["right"], "|";
unset($items, $alias, $packed, $keyed, $fn);
return "done";');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "AABB|LLRR|AABB|2|seed|AABB|LLRR|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
