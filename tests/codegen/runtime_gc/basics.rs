//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of runtime GC basics, including GC scope cleanup basic, GC return array survives, and GC return array loop.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies GC scope cleanup when a function allocates array and assoc-array locals
/// but returns a primitive integer. No memory leak or use-after-free on 1000 calls.
#[test]
fn test_gc_scope_cleanup_basic() {
    let out = compile_and_run(
        r#"<?php
function process() {
    $arr = [1, 2, 3];
    $map = ["a" => "b"];
    return 42;
}
for ($i = 0; $i < 1000; $i++) { process(); }
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a function-local array is correctly returned and survives the call.
/// Returned array elements are readable after the call returns.
#[test]
fn test_gc_return_array_survives() {
    let out = compile_and_run(
        r#"<?php
function make() {
    $arr = [10, 20, 30];
    return $arr;
}
$result = make();
echo $result[0] . "|" . $result[1] . "|" . $result[2];
"#,
    );
    assert_eq!(out, "10|20|30");
}

/// Verifies GC correctly reclaims temporary arrays in a tight 100k-iteration loop.
/// Tests that refcount cleanup runs repeatedly without corruption or leak.
#[test]
fn test_gc_return_array_loop() {
    let out = compile_and_run(
        r#"<?php
function make() { return [1, 2, 3]; }
for ($i = 0; $i < 100000; $i++) { $x = make(); }
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a function returning an associative array with string keys survives
/// the call and fields are readable by key.
#[test]
fn test_gc_return_assoc_array() {
    let out = compile_and_run(
        r#"<?php
function config() { return ["host" => "localhost", "port" => "3306"]; }
$c = config();
echo $c["host"];
"#,
    );
    assert_eq!(out, "localhost");
}

/// Verifies a borrowed array literal stored in an assoc array survives unset
/// of the source variable. The assoc array entry must still be readable.
#[test]
fn test_gc_assoc_array_literal_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [7, 8, 9];
$map = ["nums" => $inner];
unset($inner);
$saved = $map["nums"];
echo $saved[1];
"#,
    );
    assert_eq!(out, "8");
}

/// Verifies an array assigned to an assoc array entry survives unset of the
/// source variable. Reading the saved entry after unset must return the original value.
#[test]
fn test_gc_assoc_array_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4, 5, 6];
$map = ["nums" => [1]];
$map["nums"] = $inner;
unset($inner);
$saved = $map["nums"];
echo $saved[2];
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies an object returned from a function is heap-allocated correctly and
/// its public property is readable after the call.
#[test]
fn test_gc_return_object() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val;
    public function __construct($v) { $this->val = $v; }
}
function make_box($n) { return new Box($n); }
$b = make_box(42);
echo $b->val;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies explode result (a temporary array) is correctly managed inside a
/// function called 1000 times. The first element must be readable after each call.
#[test]
fn test_gc_explode_in_function_loop() {
    let out = compile_and_run(
        r#"<?php
function parse($data) {
    $parts = explode(",", $data);
    return $parts[0];
}
for ($i = 0; $i < 1000; $i++) { $r = parse("a,b,c"); }
echo $r;
"#,
    );
    assert_eq!(out, "a");
}

/// Verifies that when a function has multiple local arrays and returns one of them,
/// the correct array is returned and non-returned locals are cleaned up.
#[test]
fn test_gc_multiple_locals_one_returned() {
    let out = compile_and_run(
        r#"<?php
function work() {
    $a = [1, 2];
    $b = [3, 4];
    $c = [5, 6];
    return $b;
}
$r = work();
echo $r[0] . "|" . $r[1];
"#,
    );
    assert_eq!(out, "3|4");
}

/// Verifies GC reclaims temporary explode results correctly when reassigning
/// the same variable in a loop. No leak across 1000 iterations.
#[test]
fn test_gc_array_reassign_in_loop() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 1000; $i++) {
    $parts = explode(",", "a,b,c");
}
echo "survived";
"#,
    );
    assert_eq!(out, "survived");
}

/// Verifies nested function calls where inner returns an array and outer passes
/// it through. Tests correct GC cleanup across call boundaries in a 50k iteration loop.
#[test]
fn test_gc_nested_function_arrays() {
    let out = compile_and_run(
        r#"<?php
function inner() { return [1, 2, 3]; }
function outer() {
    $tmp = [4, 5, 6];
    $result = inner();
    return $result;
}
for ($i = 0; $i < 50000; $i++) { $x = outer(); }
echo $x[0];
"#,
    );
    assert_eq!(out, "1");
}
