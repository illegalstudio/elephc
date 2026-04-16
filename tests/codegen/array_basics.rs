use crate::support::*;

// --- Arrays ---

#[test]
fn test_array_literal_and_count() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; echo count($a);");
    assert_eq!(out, "3");
}

#[test]
fn test_array_access() {
    let out =
        compile_and_run("<?php $a = [10, 20, 30]; echo $a[0] . \" \" . $a[1] . \" \" . $a[2];");
    assert_eq!(out, "10 20 30");
}

#[test]
fn test_array_access_variable_index() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; $i = 2; echo $a[$i];");
    assert_eq!(out, "30");
}

#[test]
fn test_string_indexing_returns_single_character() {
    let out = compile_and_run(r#"<?php $s = "hello"; echo $s[1];"#);
    assert_eq!(out, "e");
}

#[test]
fn test_string_indexing_out_of_bounds_returns_empty_string() {
    let out = compile_and_run(r#"<?php $s = "hello"; echo "[" . $s[99] . "]";"#);
    assert_eq!(out, "[]");
}

#[test]
fn test_string_indexing_negative_offset_counts_from_end() {
    let out = compile_and_run(r#"<?php $s = "hello"; echo $s[-1];"#);
    assert_eq!(out, "o");
}

#[test]
fn test_string_indexing_with_variable_offset() {
    let out = compile_and_run(r#"<?php $s = "hello"; $i = 3; echo $s[$i];"#);
    assert_eq!(out, "l");
}

#[test]
fn test_string_indexing_empty_string_returns_empty_string() {
    let out = compile_and_run(r#"<?php $s = ""; $i = 0; echo "[" . $s[$i] . "]";"#);
    assert_eq!(out, "[]");
}

#[test]
fn test_string_indexing_negative_beyond_length_returns_empty() {
    let out = compile_and_run(r#"<?php $s = "hi"; echo "[" . $s[-10] . "]";"#);
    assert_eq!(out, "[]");
}

#[test]
fn test_string_indexing_exactly_negative_length_returns_first() {
    let out = compile_and_run(r#"<?php $s = "abc"; echo $s[-3];"#);
    assert_eq!(out, "a");
}

#[test]
fn test_string_indexing_at_length_returns_empty() {
    let out = compile_and_run(r#"<?php $s = "ab"; echo "[" . $s[2] . "]";"#);
    assert_eq!(out, "[]");
}

#[test]
fn test_string_indexing_last_valid_index() {
    let out = compile_and_run(r#"<?php $s = "abc"; echo $s[2];"#);
    assert_eq!(out, "c");
}

#[test]
fn test_array_assign() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; $a[1] = 99; echo $a[1];");
    assert_eq!(out, "99");
}

#[test]
fn test_array_assign_into_empty_array_updates_length() {
    let out = compile_and_run(r#"<?php $a = []; $a[0] = 7; echo count($a) . "|" . $a[0];"#);
    assert_eq!(out, "1|7");
}

#[test]
fn test_array_push() {
    let out = compile_and_run("<?php $a = [1, 2]; $a[] = 3; echo count($a) . \" \" . $a[2];");
    assert_eq!(out, "3 3");
}

#[test]
fn test_array_push_builtin() {
    let out =
        compile_and_run("<?php $a = [10]; array_push($a, 20); echo count($a) . \" \" . $a[1];");
    assert_eq!(out, "2 20");
}

#[test]
fn test_foreach_int() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; foreach ($a as $v) { echo $v; }");
    assert_eq!(out, "123");
}

#[test]
fn test_foreach_string() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "abc");
}

#[test]
fn test_foreach_break() {
    let out = compile_and_run(
        "<?php $a = [1, 2, 3, 4, 5]; foreach ($a as $v) { if ($v == 3) { break; } echo $v; }",
    );
    assert_eq!(out, "12");
}

#[test]
fn test_array_in_function() {
    let out = compile_and_run(
        r#"<?php
function sum($arr) {
    $total = 0;
    foreach ($arr as $v) {
        $total += $v;
    }
    return $total;
}
echo sum([1, 2, 3, 4, 5]);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_string_array() {
    let out = compile_and_run(
        r#"<?php
$names = ["Alice", "Bob"];
$names[] = "Charlie";
echo count($names) . ": ";
foreach ($names as $n) { echo $n . " "; }
"#,
    );
    assert_eq!(out, "3: Alice Bob Charlie ");
}

// --- Array functions ---

#[test]
fn test_array_pop() {
    let out =
        compile_and_run("<?php $a = [1, 2, 3]; $v = array_pop($a); echo $v . \" \" . count($a);");
    assert_eq!(out, "3 2");
}

#[test]
fn test_array_pop_empty() {
    let out = compile_and_run("<?php $a = [1]; array_pop($a); echo array_pop($a);");
    assert_eq!(out, "");
}

#[test]
fn test_in_array_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(20, $a);");
    assert_eq!(out, "1");
}

#[test]
fn test_in_array_not_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(99, $a);");
    assert_eq!(out, "0");
}

#[test]
fn test_in_array_string_found() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; echo in_array("b", $a);"#);
    assert_eq!(out, "1");
}

#[test]
fn test_in_array_string_not_found() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; echo in_array("x", $a);"#);
    assert_eq!(out, "0");
}

#[test]
fn test_sort() {
    let out =
        compile_and_run(r#"<?php $a = [5, 3, 1, 4, 2]; sort($a); foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "12345");
}

#[test]
fn test_rsort() {
    let out =
        compile_and_run(r#"<?php $a = [1, 3, 2]; rsort($a); foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "321");
}

#[test]
fn test_array_keys() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $k = array_keys($a); foreach ($k as $v) { echo $v; }"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_isset() {
    let out = compile_and_run("<?php $x = 42; echo isset($x);");
    assert_eq!(out, "1");
}

#[test]
fn test_array_values() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $v = array_values($a); foreach ($v as $x) { echo $x; }"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_die() {
    let out = compile_and_run("<?php echo \"before\"; die(); echo \"after\";");
    assert_eq!(out, "before");
}

// --- Nested control flow ---

#[test]
fn test_nested_if() {
    let out = compile_and_run(
        "<?php $x = 5; if ($x > 0) { if ($x > 3) { echo \"big\"; } else { echo \"small\"; } }",
    );
    assert_eq!(out, "big");
}

#[test]
fn test_nested_loops() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 3; $i++) { for ($j = 0; $j < 2; $j++) { echo $i . $j . \" \"; } }",
    );
    assert_eq!(out, "00 01 10 11 20 21 ");
}

#[test]
fn test_for_continue() {
    let out =
        compile_and_run("<?php for ($i = 0; $i < 5; $i++) { if ($i == 2) { continue; } echo $i; }");
    assert_eq!(out, "0134");
}

#[test]
fn test_while_with_function() {
    let out = compile_and_run(
        r#"<?php
function sum_to($n) {
    $s = 0;
    $i = 1;
    while ($i <= $n) {
        $s = $s + $i;
        $i++;
    }
    return $s;
}
echo sum_to(10);
"#,
    );
    assert_eq!(out, "55");
}

#[test]
fn test_function_with_if_return() {
    let out = compile_and_run(
        r#"<?php
function abs_val($x) {
    if ($x < 0) {
        return -$x;
    }
    return $x;
}
echo abs_val(-5) . " " . abs_val(3);
"#,
    );
    assert_eq!(out, "5 3");
}

#[test]
fn test_function_calling_function() {
    let out = compile_and_run(
        r#"<?php
function square($x) { return $x * $x; }
function sum_of_squares($a, $b) { return square($a) + square($b); }
echo sum_of_squares(3, 4);
"#,
    );
    assert_eq!(out, "25");
}

#[test]
fn test_multiple_elseif() {
    let out = compile_and_run(
        r#"<?php
$x = 4;
if ($x == 1) { echo "one"; }
elseif ($x == 2) { echo "two"; }
elseif ($x == 3) { echo "three"; }
elseif ($x == 4) { echo "four"; }
else { echo "other"; }
"#,
    );
    assert_eq!(out, "four");
}

