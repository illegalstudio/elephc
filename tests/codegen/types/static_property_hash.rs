//! Purpose:
//! Integration tests for writing a STRING key into a `static array` property.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `public static array $store = []` refused `self::$store[$k] = $v` outright — `Array index
//!   must be integer` — while the INSTANCE property one line away accepted the same write and
//!   promoted the property to hash storage. php runs both, so the static path was rejecting
//!   ordinary code: the shape came out of php's own userspace stream-wrapper idiom, where a
//!   wrapper keeps its bodies in a static map keyed by URL.
//! - Accepting it in the CHECKER alone is a false win. The lowering's write path had no
//!   associative branch either, so the write fell through to the Mixed fallback while every READ
//!   loaded the property as a hash: `count(self::$store)` answered 0 after two writes, in
//!   silence. Both halves are what this pins.
//! - A declared `array<int>` must still refuse a widening write, which is the guard the instance
//!   path already carried and the static path now shares.

use crate::support::*;

/// Verifies a string key reaches a static array property, and reads back.
#[test]
fn a_string_key_writes_into_a_static_array_property() {
    let out = compile_and_run_capture(
        r#"<?php
class S {
    public static array $store = [];
    public static function put(string $k, string $v): void { self::$store[$k] = $v; }
}
S::put("a", "1");
S::$store["b"] = "2";
S::put("a", "over");
var_dump(S::$store, count(S::$store), S::$store["a"], isset(S::$store["b"]));
foreach (S::$store as $key => $value) { echo $key, "=", $value, ";"; }
echo "\n";
var_dump(array_keys(S::$store), array_values(S::$store));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(2) {\n  [\"a\"]=>\n  string(4) \"over\"\n  [\"b\"]=>\n  string(1) \"2\"\n}\n\
         int(2)\n\
         string(4) \"over\"\n\
         bool(true)\n\
         a=over;b=2;\n\
         array(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n}\n\
         array(2) {\n  [0]=>\n  string(4) \"over\"\n  [1]=>\n  string(1) \"2\"\n}\n"
    );
}

/// Verifies an INTEGER key still uses list storage, so the promotion is not unconditional.
#[test]
fn an_integer_key_keeps_a_static_array_a_list() {
    let out = compile_and_run_capture(
        r#"<?php
class L {
    public static array $rows = [];
}
L::$rows[] = "push";
L::$rows[1] = "set";
var_dump(L::$rows, count(L::$rows));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(2) {\n  [0]=>\n  string(4) \"push\"\n  [1]=>\n  string(3) \"set\"\n}\nint(2)\n"
    );
}

/// Verifies a static property inherited through `self::`/`static::`/the class name is one store.
#[test]
fn every_spelling_of_the_receiver_reaches_one_store() {
    let out = compile_and_run_capture(
        r#"<?php
class Reg {
    public static array $map = [];
    public static function a(string $k): void { self::$map[$k] = "self"; }
    public static function b(string $k): void { static::$map[$k] = "static"; }
}
Reg::a("one");
Reg::b("two");
Reg::$map["three"] = "named";
var_dump(count(Reg::$map), Reg::$map["one"], Reg::$map["two"], Reg::$map["three"]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "int(3)\nstring(4) \"self\"\nstring(6) \"static\"\nstring(5) \"named\"\n"
    );
}
