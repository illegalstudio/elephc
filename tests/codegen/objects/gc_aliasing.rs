use super::*;

#[test]
fn test_gc_array_alias_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$b = $a;
unset($a);
echo $b[0];
echo $b[1];
echo $b[2];
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_gc_returned_array_alias_survives_caller_unset() {
    let out = compile_and_run(
        r#"<?php
function share($arr) {
    return $arr;
}

$a = [7, 8];
$b = share($a);
unset($a);
echo $b[0];
echo $b[1];
"#,
    );
    assert_eq!(out, "78");
}

#[test]
fn test_gc_returned_object_alias_survives_caller_unset() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val = 0; }

function share($box) {
    return $box;
}

$a = new Box();
$a->val = 41;
$b = share($a);
unset($a);
echo $b->val;
"#,
    );
    assert_eq!(out, "41");
}

#[test]
fn test_gc_array_push_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [9];
$outer = [];
$outer[] = $inner;
unset($inner);
echo $outer[0][0];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_gc_indexed_array_literal_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [3, 4];
$outer = [$inner];
unset($inner);
echo $outer[0][1];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_array_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4];
$outer = [[1], [2]];
$outer[1] = $inner;
unset($inner);
echo $outer[1][0];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_property_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
class Holder { public $value; }

$inner = [7];
$h = new Holder();
$h->value = $inner;
unset($inner);
$saved = $h->value;
echo $saved[0];
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_gc_static_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
function hold_once() {
    static $saved = [];
    $tmp = [5];
    $saved = $tmp;
    unset($tmp);
    echo $saved[0];
}

hold_once();
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_gc_spread_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [8];
$src = [$inner];
$dst = [...$src];
unset($src);
unset($inner);
echo $dst[0][0];
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_gc_array_merge_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [6];
$left = [$inner];
$right = [[7]];
$merged = array_merge($left, $right);
unset($left);
unset($inner);
echo $merged[0][0] . "|" . $merged[1][0];
"#,
    );
    assert_eq!(out, "6|7");
}

#[test]
fn test_gc_array_chunk_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [5];
$rows = [$inner, [9]];
$chunks = array_chunk($rows, 1);
unset($rows);
unset($inner);
echo $chunks[0][0][0] . "|" . $chunks[1][0][0];
"#,
    );
    assert_eq!(out, "5|9");
}

#[test]
fn test_gc_array_slice_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [2];
$src = [[1], $inner, [3]];
$slice = array_slice($src, 1, 1);
unset($src);
unset($inner);
echo $slice[0][0];
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_gc_array_reverse_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4];
$src = [[1], $inner, [7]];
$rev = array_reverse($src);
unset($src);
unset($inner);
echo $rev[1][0];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_array_pad_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [5];
$src = [[1]];
$padded = array_pad($src, 3, $inner);
unset($src);
unset($inner);
echo $padded[1][0] . "|" . $padded[2][0];
"#,
    );
    assert_eq!(out, "5|5");
}

#[test]
fn test_gc_array_unique_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [3];
$src = [$inner, $inner, [4]];
$uniq = array_unique($src);
unset($src);
unset($inner);
echo count($uniq) . "|" . $uniq[0][0] . "|" . $uniq[1][0];
"#,
    );
    assert_eq!(out, "2|3|4");
}

#[test]
fn test_gc_array_splice_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [7];
$src = [[1], $inner, [9]];
$removed = array_splice($src, 1, 1);
unset($src);
unset($inner);
echo $removed[0][0];
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_gc_array_diff_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [6];
$left = [$inner, [8]];
$right = [[8]];
$diff = array_diff($left, $right);
unset($left);
unset($inner);
echo $diff[0][0];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_gc_array_intersect_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [9];
$left = [[1], $inner];
$right = [$inner];
$both = array_intersect($left, $right);
unset($left);
unset($right);
unset($inner);
echo $both[0][0];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_gc_array_filter_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
function keep_pair($x) { return count($x) == 2; }
$inner = [10, 11];
$rows = [[1], $inner, [2, 3]];
$filtered = array_filter($rows, "keep_pair");
unset($rows);
unset($inner);
echo $filtered[0][1] . "|" . $filtered[1][0];
"#,
    );
    assert_eq!(out, "11|2");
}

#[test]
fn test_gc_array_fill_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [12];
$filled = array_fill(0, 2, $inner);
unset($inner);
echo $filled[0][0] . "|" . $filled[1][0];
"#,
    );
    assert_eq!(out, "12|12");
}

#[test]
fn test_gc_array_combine_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [13];
$keys = ["keep"];
$vals = [$inner];
$map = array_combine($keys, $vals);
unset($vals);
unset($inner);
$saved = $map["keep"];
echo $saved[0];
"#,
    );
    assert_eq!(out, "13");
}

#[test]
fn test_gc_array_fill_keys_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [14];
$keys = ["a", "b"];
$map = array_fill_keys($keys, $inner);
unset($inner);
$first = $map["a"];
$second = $map["b"];
echo $first[0] . "|" . $second[0];
"#,
    );
    assert_eq!(out, "14|14");
}
