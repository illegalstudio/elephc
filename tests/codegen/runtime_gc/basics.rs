use crate::support::*;

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
