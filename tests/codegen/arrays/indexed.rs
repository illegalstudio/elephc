use crate::support::*;

// --- Phase 13: v0.6 — Array functions ---

#[test]
fn test_array_reverse() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
$b = array_reverse($a);
echo $b[0] . $b[1] . $b[2];
"#,
    );
    assert_eq!(out, "213");
}

#[test]
fn test_array_sum() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_sum($a);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_array_product() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 3, 4];
echo array_product($a);
"#,
    );
    assert_eq!(out, "24");
}

#[test]
fn test_array_search() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(20, $a);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_array_search_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(99, $a) === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

#[test]
fn test_array_search_assigned_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$result = array_search(99, $a);
echo $result === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

#[test]
fn test_array_search_zero_index_is_not_false() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(10, $a) === false ? "miss" : "zero";
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_array_key_exists() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
if (array_key_exists(1, $a)) { echo "yes"; }
if (!array_key_exists(5, $a)) { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_array_merge() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = array_merge($a, $b);
echo count($c);
echo $c[0] . $c[1] . $c[2] . $c[3];
"#,
    );
    assert_eq!(out, "41234");
}

#[test]
fn test_array_slice() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30, 40, 50];
$b = array_slice($a, 1, 3);
echo $b[0] . " " . $b[1] . " " . $b[2];
"#,
    );
    assert_eq!(out, "20 30 40");
}

#[test]
fn test_array_shift() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$first = array_shift($a);
echo $first . " " . count($a);
"#,
    );
    assert_eq!(out, "10 2");
}

#[test]
fn test_array_shift_empty() {
    let out = compile_and_run("<?php $a = [1]; array_shift($a); echo array_shift($a);");
    assert_eq!(out, "");
}

#[test]
fn test_array_unshift() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 3];
$n = array_unshift($a, 1);
echo $n . " " . $a[0];
"#,
    );
    assert_eq!(out, "3 1");
}

#[test]
fn test_range() {
    let out = compile_and_run(
        r#"<?php
$a = range(1, 5);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "5:12345");
}

#[test]
fn test_range_descending() {
    let out = compile_and_run(
        r#"<?php
$a = range(5, 1);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "5:54321");
}

#[test]
fn test_range_single_element() {
    let out = compile_and_run(
        r#"<?php
$a = range(3, 3);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "1:3");
}

#[test]
fn test_array_unique() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 2, 3, 3, 3];
$b = array_unique($a);
echo count($b);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_array_fill() {
    let out = compile_and_run(
        r#"<?php
$a = array_fill(0, 3, 42);
echo $a[0] . " " . $a[1] . " " . $a[2];
"#,
    );
    assert_eq!(out, "42 42 42");
}

#[test]
fn test_array_diff() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = [2, 4];
$c = array_diff($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_intersect() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = [2, 4, 6];
$c = array_intersect($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_rand() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$i = array_rand($a);
if ($i >= 0 && $i < 3) { echo "ok"; }
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_shuffle() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
shuffle($a);
echo count($a);
echo array_sum($a);
"#,
    );
    assert_eq!(out, "515");
}

#[test]
fn test_array_pad() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = array_pad($a, 5, 0);
echo count($b);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_array_splice() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$removed = array_splice($a, 1, 2);
echo count($removed) . " " . count($a);
"#,
    );
    assert_eq!(out, "2 3");
}

#[test]
fn test_array_combine() {
    let out = compile_and_run(
        r#"<?php
$keys = ["a", "b"];
$vals = [1, 2];
$m = array_combine($keys, $vals);
echo count($m);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_flip() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$f = array_flip($a);
echo count($f);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_array_chunk() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$c = array_chunk($a, 2);
echo count($c);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_array_fill_keys() {
    let out = compile_and_run(
        r#"<?php
$keys = ["x", "y"];
$m = array_fill_keys($keys, 0);
echo count($m);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_diff_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => "1", "b" => "2"];
$b = ["a" => "9"];
$c = array_diff_key($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_gc_array_diff_key_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$src = ["keep" => [1, 2], "drop" => [3, 4]];
$mask = ["drop" => 1];
$filtered = array_diff_key($src, $mask);
unset($src);
$saved = $filtered["keep"];
echo $saved[1];
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_intersect_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => "1", "b" => "2"];
$b = ["a" => "9"];
$c = array_intersect_key($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_gc_array_intersect_key_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$src = ["keep" => [5, 6], "drop" => [7, 8]];
$mask = ["keep" => 1];
$filtered = array_intersect_key($src, $mask);
unset($src);
$saved = $filtered["keep"];
echo $saved[0] . "|" . $saved[1];
"#,
    );
    assert_eq!(out, "5|6");
}

#[test]
fn test_asort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
asort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_arsort() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 3, 2];
arsort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_ksort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
ksort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_krsort() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
krsort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_natsort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
natsort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_natcasesort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
natcasesort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}
