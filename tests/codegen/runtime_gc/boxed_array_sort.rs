//! Purpose:
//! Covers scalar sorting through declared PHP array parameters and references.
//!
//! Called from:
//! - The runtime GC codegen integration module.
//!
//! Key details:
//! - Sorting normalizes boxed layouts, reindexes keys and preserves COW aliases and payload owners.

use crate::support::*;

/// Both directions sort boxed lists and maps independently of aliases with a clean heap.
#[test]
fn test_boxed_array_sort_reindexes_keys_and_preserves_aliases() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function ascending(array &$values): void { sort($values); }
function descending(array &$values): void { rsort($values); }
function sortedCopy(array $values): array { sort($values); return $values; }
$list = [3, 1, 2];
$alias = $list;
ascending($list);
echo implode(",", $list), "|", implode(",", $alias), "|";
$map = ["last" => 3, 7 => 1, "middle" => 2];
$mapAlias = $map;
descending($map);
echo implode(",", array_keys($map)), ":", implode(",", $map), "|";
echo implode(",", array_keys($mapAlias)), ":", implode(",", $mapAlias), "|";
$copy = sortedCopy($mapAlias);
echo implode(",", $copy), "|", implode(",", $mapAlias), "|";
$empty = [];
ascending($empty);
descending($empty);
echo count($empty);
unset($list, $alias, $map, $mapAlias, $copy, $empty);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1,2,3|3,1,2|0,1,2:3,2,1|last,7,middle:3,1,2|1,2,3|3,1,2|0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// String and float layouts retain their values and tags when converted into sortable Mixed slots.
#[test]
fn test_boxed_array_sort_string_float_and_mixed_scalar_ownership() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function ascending(array &$values): void { sort($values); }
function descending(array &$values): void { rsort($values); }
$words = [str_repeat("z", 3), str_repeat("a", 3), str_repeat("m", 3)];
$alias = $words;
ascending($words);
echo implode(",", $words), "|", implode(",", $alias), "|";
$floats = [1.25, 3.5, 2.25];
descending($floats);
echo implode(",", $floats), ":", gettype($floats[0]), "|";
$mixed = [10, "9", 2];
ascending($mixed);
echo $mixed[0], ":", gettype($mixed[0]), ",", $mixed[1], ":", gettype($mixed[1]), ",", $mixed[2], "|";
descending($mixed);
echo implode(",", $mixed);
unset($words, $alias, $floats, $mixed);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "aaa,mmm,zzz|zzz,aaa,mmm|3.5,2.25,1.25:double|2:integer,9:string,10|10,9,2", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Declared arrays retain the existing explicit restriction against non-scalar Mixed sorting.
#[test]
fn test_boxed_array_sort_keeps_non_scalar_guard() {
    for name in ["sort", "rsort"] {
        let error = compile_and_run_expect_failure(&format!(r#"<?php
function guardedSort(array &$values): void {{ {name}($values); }}
$values = [[2], [1]];
guardedSort($values);
"#));
        assert!(error.contains("sorting Mixed arrays containing non-scalar values is not supported"), "{error}");
    }
}
