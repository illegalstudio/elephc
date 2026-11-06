//! Purpose:
//! Pins behaviour that open issues reported broken and that current main already gets right,
//! one fixture per issue, so the fixes cannot regress silently once the issues are closed.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture is the issue's own reproduction, trimmed or extended with the neighbouring
//!   shapes the issue names; the expected stdout was captured from PHP 8.5.10 (`php -n`).
//! - Leak reports assert a clean heap under `--heap-debug`; values that are merely live at exit
//!   (objects still bound at top level) are not asserted as leaks.

use super::*;

/// `empty(NAN)` answered `true` on linux-x86_64 because `ucomisd` + `sete` treated the
/// unordered comparison as equal to zero. Pins the statically typed float path, the boxed
/// `Mixed` path, and the ordinary zero/non-zero/infinite floats around them.
/// Regression for #627.
#[test]
fn test_issue_627_empty_nan_is_false_for_float_and_boxed_mixed() {
    let out = compile_and_run(
        r#"<?php
$value = NAN;
echo is_nan($value) ? "nan\n" : "not-nan\n";
var_dump(empty($value));
$boxed = $argc == 1 ? NAN : "text";
var_dump(empty($boxed));
var_dump(empty(0.0));
var_dump(empty(-0.0));
var_dump(empty(1.5));
var_dump(empty(INF));
var_dump(empty(-INF));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "nan\nbool(false)\nbool(false)\nbool(true)\nbool(true)\n",
            "bool(false)\nbool(false)\nbool(false)\n",
        )
    );
}

/// A by-value capture whose incoming type is a union leaked its incoming heap value when the
/// closure body overwrote the captured local. Pins the value and a clean heap across repeated
/// invocations. Regression for #772.
#[test]
fn test_issue_772_union_by_value_capture_overwrite_releases_incoming_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = $argc > 1 ? 1 : "z" . $argc;
$f = function (int $n) use ($m) { $m = 1; $m = "s" . $n; return $m; };
for ($i = 0; $i < 20; $i++) { $f($i); }
var_dump($f(2));
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "string(2) \"s2\"\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A null-initialized local later assigned a heap string leaked one detached cast copy per
/// executed read. Pins repeated reads with a clean heap. Regression for #789 (trigger 1).
#[test]
fn test_issue_789_null_then_string_reads_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$b = null;
$b = "s" . $argc;
echo $b, "|";
for ($i = 0; $i < 10; $i++) { echo $b; }
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("s1|{}", "s1".repeat(10)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A retype whose right-hand side reads the old heap string leaked the cast copy of that read.
/// Regression for #789 (trigger 2).
#[test]
fn test_issue_789_retype_reading_the_old_string_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = "n" . $argc;
$a = strlen($a);
echo $a;
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A retype whose new value contains the old heap string leaked the cast copy taken for the
/// array literal. Regression for #789 (trigger 3).
#[test]
fn test_issue_789_retype_wrapping_the_old_string_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = "s" . $argc;
$a = [$a];
echo $a[0];
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "s1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A nested index write into an array property element (`$o->x[0][1] = 9`) was silently
/// dropped, leaving the stale inner array in the object. Pins default-initialized,
/// constructor-initialized, and `mixed` property slots plus the nested append spelling.
/// Regression for #1206.
#[test]
fn test_issue_1206_nested_index_write_into_array_property_element() {
    let out = compile_and_run(
        r#"<?php
class A { public array $x = [[1, 2], [3]]; }
class B { public array $x; public function __construct() { $this->x = [[1, 2], [3]]; } }
class C { public mixed $x = [["k" => 1]]; }
$a = new A(); $a->x[0][1] = 9; echo "A:", $a->x[0][1], "|";
$b = new B(); $b->x[0][1] = 9; echo "B:", $b->x[0][1], "|";
$c = new C(); $c->x[0]["k"] = 9; echo "C:", $c->x[0]["k"], "|";
$a->x[0][] = 7; echo "append:", count($a->x[0]), " ", json_encode($a->x), "\n";
"#,
    );
    assert_eq!(out, "A:9|B:9|C:9|append:3 [[1,9,7],[3]]\n");
}

/// `array_push()` into a static property inside a loop lost the array once it reallocated.
/// Pins the counts through several growths and the element pushed after them, with a clean
/// heap. Regression for #1207.
#[test]
fn test_issue_1207_array_push_into_static_property_grows_inside_a_loop() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public static array $shared = [1]; }
for ($i = 2; $i < 8; $i++) { array_push(Box::$shared, $i); echo count(Box::$shared), "|"; }
for ($i = 8; $i < 34; $i++) { array_push(Box::$shared, $i); }
echo count(Box::$shared), ",", Box::$shared[32], "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2|3|4|5|6|7|33,33\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reading a static-property array through `implode()` while pushing into it in a loop
/// aborted with "Possible integer overflow in memory allocation". Regression for #1207.
#[test]
fn test_issue_1207_static_property_array_push_then_implode_in_a_loop() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public static array $shared = [1]; }
for ($i = 2; $i < 10; $i++) { array_push(Box::$shared, $i); echo count(Box::$shared), ":", implode(",", Box::$shared), "\n"; }
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "2:1,2\n3:1,2,3\n4:1,2,3,4\n5:1,2,3,4,5\n6:1,2,3,4,5,6\n",
            "7:1,2,3,4,5,6,7\n8:1,2,3,4,5,6,7,8\n9:1,2,3,4,5,6,7,8,9\n",
        ),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Appending after a `PHP_INT_MAX` key wrapped the next key to `PHP_INT_MIN` instead of
/// throwing. Pins the catchable `Error`, its message, and that the array is left unchanged, for
/// a hash and for a packed-looking literal. Regression for #1315.
#[test]
fn test_issue_1315_append_after_php_int_max_throws_and_keeps_the_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["x" => 1, PHP_INT_MAX => "max"];
try { $a[] = "next"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_export(array_keys($a)); echo "\n";
$b = [PHP_INT_MAX => 1];
try { $b[] = 2; } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo count($b), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "Error: Cannot add element to the array as the next element is already occupied\n",
            "array (\n  0 => 'x',\n  1 => 9223372036854775807,\n)\n",
            "Cannot add element to the array as the next element is already occupied\n",
            "1\n",
        ),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
