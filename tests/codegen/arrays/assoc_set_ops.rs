//! Purpose:
//! Integration tests for associative-array set/merge builtins: `array_replace`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries; assertions compare stdout.
//! - Covers in-place overwrite (keeping key position), appended keys, copy-on-write
//!   non-mutation of the source, string values, and case-insensitive calls.

use crate::support::*;

/// Verifies array_replace() overwrites matching keys in place and appends new keys,
/// preserving the first array's key order.
/// Fixture: base {a:1,b:2} replaced by {b:9,c:3} → a=1;b=9;c=3 in that order.
#[test]
fn test_array_replace_overwrite_and_append() {
    let out = compile_and_run(
        r#"<?php
$base = ["a" => 1, "b" => 2];
$over = ["b" => 9, "c" => 3];
$r = array_replace($base, $over);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "a=1;b=9;c=3;");
}

/// Verifies array_replace() does not mutate the source array (copy-on-write).
/// Fixture: base {x:1} replaced by {x:5}; base["x"] stays 1, result["x"] is 5.
#[test]
fn test_array_replace_source_unchanged() {
    let out = compile_and_run(
        r#"<?php
$base = ["x" => 1];
$over = ["x" => 5];
$r = array_replace($base, $over);
echo $base["x"];
echo $r["x"];
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies array_replace() carries string values from both arrays into the result.
/// Fixture: base {name:"alice",role:"user"} replaced by {role:"admin"}.
#[test]
fn test_array_replace_string_values() {
    let out = compile_and_run(
        r#"<?php
$base = ["name" => "alice", "role" => "user"];
$over = ["role" => "admin"];
$r = array_replace($base, $over);
echo $r["name"];
echo "-";
echo $r["role"];
"#,
    );
    assert_eq!(out, "alice-admin");
}

/// Verifies array_replace() result count reflects merged distinct keys.
/// Fixture: {a:1,b:2} replaced by {b:9,c:3,d:4} → 4 distinct keys.
#[test]
fn test_array_replace_count() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace(["a" => 1, "b" => 2], ["b" => 9, "c" => 3, "d" => 4]);
echo count($r);
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies array_replace() is callable case-insensitively, matching PHP builtin name rules.
/// Fixture: mixed-case spelling overwriting one key.
#[test]
fn test_array_replace_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
$r = Array_Replace(["a" => 1], ["a" => 2]);
echo $r["a"];
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies array_diff_assoc() keeps entries whose (key, value) pair is absent from the
/// second array: matching pair dropped, differing value kept, missing key kept.
/// Fixture: {a:1,b:2,c:3} vs {a:1,b:5} → b=2 (value differs) and c=3 (key absent) remain.
#[test]
fn test_array_diff_assoc_basic() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
$b = ["a" => 1, "b" => 5];
$r = array_diff_assoc($a, $b);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "b=2;c=3;");
}

/// Verifies array_diff_assoc() compares values by PHP string cast: integer 5 and string "5"
/// are equal, so the matching pair is dropped.
/// Fixture: {x:5} vs {x:"5"} → empty result (count 0).
#[test]
fn test_array_diff_assoc_string_cast_equality() {
    let out = compile_and_run(
        r#"<?php
$r = array_diff_assoc(["x" => 5], ["x" => "5"]);
echo count($r);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies array_intersect_assoc() keeps only entries whose (key, value) pair appears in
/// the second array.
/// Fixture: {a:1,b:2,c:3} vs {a:1,b:5} → only a=1 matches.
#[test]
fn test_array_intersect_assoc_basic() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
$b = ["a" => 1, "b" => 5];
$r = array_intersect_assoc($a, $b);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "a=1;");
}

/// Verifies array_intersect_assoc() over string values keeps matching key+value pairs and
/// drops differing ones; exercises string-value retain plus temporary-box release.
/// Fixture: {name:"alice",role:"user"} vs {name:"alice",role:"admin"} → only name kept.
#[test]
fn test_array_intersect_assoc_string_values() {
    let out = compile_and_run(
        r#"<?php
$a = ["name" => "alice", "role" => "user"];
$b = ["name" => "alice", "role" => "admin"];
$r = array_intersect_assoc($a, $b);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "name=alice;");
}

/// Verifies array_diff_assoc() and array_intersect_assoc() are callable case-insensitively.
/// Fixture: mixed-case spellings over the same fixtures.
#[test]
fn test_assoc_diff_intersect_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
$d = Array_Diff_Assoc(["a" => 1, "b" => 2], ["a" => 1]);
echo count($d);
$i = Array_Intersect_Assoc(["a" => 1, "b" => 2], ["a" => 1]);
echo count($i);
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies array_replace_recursive() merges nested associative arrays key-by-key instead
/// of overwriting them wholesale.
/// Fixture: {cfg:{x:1,y:2}} replaced by {cfg:{y:9,z:3}} → cfg = {x:1,y:9,z:3}.
#[test]
fn test_array_replace_recursive_nested_merge() {
    let out = compile_and_run(
        r#"<?php
$base = ["cfg" => ["x" => 1, "y" => 2]];
$over = ["cfg" => ["y" => 9, "z" => 3]];
$r = array_replace_recursive($base, $over);
$c = $r["cfg"];
echo $c["x"];
echo $c["y"];
echo $c["z"];
"#,
    );
    assert_eq!(out, "193");
}

/// Verifies array_replace_recursive() overwrites non-array values like array_replace.
/// Fixture: {a:1,b:2} replaced by {b:9} → a kept, b overwritten.
#[test]
fn test_array_replace_recursive_scalar_overwrite() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace_recursive(["a" => 1, "b" => 2], ["b" => 9]);
echo $r["a"];
echo $r["b"];
"#,
    );
    assert_eq!(out, "19");
}

/// Verifies array_replace_recursive() leaves the source arrays (and their nested arrays)
/// unchanged (copy-on-write through the recursive clone).
/// Fixture: nested {cfg:{x:1}} replaced by {cfg:{x:5}}; base nested x stays 1.
#[test]
fn test_array_replace_recursive_source_unchanged() {
    let out = compile_and_run(
        r#"<?php
$base = ["cfg" => ["x" => 1]];
$over = ["cfg" => ["x" => 5]];
$r = array_replace_recursive($base, $over);
$bc = $base["cfg"];
$rc = $r["cfg"];
echo $bc["x"];
echo $rc["x"];
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies array_merge_recursive() recursively merges nested associative arrays sharing a key.
/// Fixture: {cfg:{a:1}} + {cfg:{b:2}} → cfg = {a:1,b:2}.
#[test]
fn test_array_merge_recursive_nested_merge() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive(["cfg" => ["a" => 1]], ["cfg" => ["b" => 2]]);
foreach ($r["cfg"] as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "a=1;b=2;");
}

/// Verifies array_merge_recursive() combines two scalar values at a colliding string key
/// into a renumbered list.
/// Fixture: {k:1} + {k:2} → k = [0=>1, 1=>2].
#[test]
fn test_array_merge_recursive_scalar_combine() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive(["k" => 1], ["k" => 2]);
echo count($r["k"]);
echo ":";
foreach ($r["k"] as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "2:0=1;1=2;");
}

/// Verifies array_merge_recursive() keeps non-colliding string keys from both arrays.
/// Fixture: {a:1} + {b:2} → {a:1, b:2}.
#[test]
fn test_array_merge_recursive_no_collision() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive(["a" => 1], ["b" => 2]);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "a=1;b=2;");
}

/// Verifies array_merge_recursive() renumbers integer keys sequentially across both arrays.
/// Fixture: {5:10} + {9:20} → {0:10, 1:20}.
#[test]
fn test_array_merge_recursive_int_keys_renumber() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive([5 => 10], [9 => 20]);
foreach ($r as $k => $v) { echo "$k=$v;"; }
"#,
    );
    assert_eq!(out, "0=10;1=20;");
}

/// Verifies array_merge_recursive() string-scalar collisions combine into a list with the
/// correct string values preserved (persisted independently of the temporary wrappers).
/// Fixture: {k:"ab"} + {k:"cd"} → k = ["ab", "cd"].
#[test]
fn test_array_merge_recursive_string_combine() {
    let out = compile_and_run(
        r#"<?php
$r = array_merge_recursive(["k" => "ab"], ["k" => "cd"]);
echo count($r["k"]);
echo ":";
foreach ($r["k"] as $v) { echo $v; echo ","; }
"#,
    );
    assert_eq!(out, "2:ab,cd,");
}

/// Verifies array_merge_recursive() is callable case-insensitively, matching PHP builtin name rules.
/// Fixture: mixed-case spelling combining two scalar values.
#[test]
fn test_array_merge_recursive_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
$r = Array_Merge_Recursive(["k" => 1], ["k" => 2]);
echo count($r["k"]);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies array_multisort() sorts the first array ascending and reorders the second in tandem,
/// mutating both arrays in place (by reference).
/// Fixture: [3,1,2] + [30,10,20] → [1,2,3] + [10,20,30].
#[test]
fn test_array_multisort_parallel() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
$b = [30, 10, 20];
array_multisort($a, $b);
foreach ($a as $v) { echo $v; }
echo "|";
foreach ($b as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "123|102030");
}

/// Verifies array_multisort() returns true and leaves an already-sorted pair unchanged.
/// Fixture: [1,2,3] + [7,8,9] stay aligned; the call returns true.
#[test]
fn test_array_multisort_already_sorted_returns_true() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$b = [7, 8, 9];
$ok = array_multisort($a, $b);
echo $ok ? "y" : "n";
foreach ($b as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "y789");
}

/// Verifies array_multisort() is callable case-insensitively and keeps a stable tandem order.
/// Fixture: [2,2,1] + [20,21,10] → first array sorted, ties keep insertion order.
#[test]
fn test_array_multisort_case_insensitive_stable() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 2, 1];
$b = [20, 21, 10];
Array_Multisort($a, $b);
foreach ($a as $v) { echo $v; }
echo "|";
foreach ($b as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "122|102021");
}

/// Verifies array_replace() accepts scalar indexed-array inputs: the indexed first array is
/// converted to an integer-keyed hash and the second array overwrites the matching index.
/// Fixture: [10,20,30] replaced by {1:99} → 0:10 1:99 2:30.
#[test]
fn test_array_replace_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace([10, 20, 30], [1 => 99]);
foreach ($r as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:10 1:99 2:30 ");
}

/// Verifies array_replace() with an indexed first array and an associative second array that
/// shares the integer key space (homogeneous keys) overwrites and appends correctly.
/// Fixture: [1,2,3] replaced by {1:99, 3:40} → 0:1 1:99 2:3 3:40.
#[test]
fn test_array_replace_indexed_then_int_keys() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace([1, 2, 3], [1 => 99, 3 => 40]);
foreach ($r as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 1:99 2:3 3:40 ");
}

/// Verifies array_replace_recursive() accepts scalar indexed inputs (converted to hashes).
/// Fixture: [1,2,3] replaced by {1:9} → 0:1 1:9 2:3.
#[test]
fn test_array_replace_recursive_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$r = array_replace_recursive([1, 2, 3], [1 => 9]);
foreach ($r as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 1:9 2:3 ");
}

/// Verifies array_diff_assoc() accepts indexed inputs, keeping first-array entries whose
/// (key, value) pair is absent from the second array.
/// Fixture: [1,2,3] minus [1,5] → 1:2 2:3 (index 0 matches value 1, index 1 differs).
#[test]
fn test_array_diff_assoc_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$d = array_diff_assoc([1, 2, 3], [1, 5]);
foreach ($d as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "1:2 2:3 ");
}

/// Verifies array_intersect_assoc() accepts indexed inputs, keeping first-array entries whose
/// (key, value) pair is present in the second array.
/// Fixture: [1,2,3] intersect [1,5] → 0:1 (only index 0 matches by key and value).
#[test]
fn test_array_intersect_assoc_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$i = array_intersect_assoc([1, 2, 3], [1, 5]);
foreach ($i as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 ");
}

/// Verifies array_merge_recursive() accepts indexed inputs, appending and renumbering integer keys.
/// Fixture: [1,2] merged with [3,4] → 0:1 1:2 2:3 3:4.
#[test]
fn test_array_merge_recursive_indexed_inputs() {
    let out = compile_and_run(
        r#"<?php
$m = array_merge_recursive([1, 2], [3, 4]);
foreach ($m as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 1:2 2:3 3:4 ");
}
