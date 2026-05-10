//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object static properties, including class static method string param, class static and instance, and static property read write.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

#[test]
fn test_class_static_method_string_param() {
    let out = compile_and_run(
        r#"<?php
class Utils {
    public static function greet($name) { return "Hello " . $name; }
}
echo Utils::greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_class_static_and_instance() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $n;
    public function __construct($n) { $this->n = $n; }
    public function next() { return $this->n + 1; }
    public static function make($n) { return new Counter($n); }
}
$c = Counter::make(4);
echo $c->next();
"#,
    );
    assert_eq!(out, "5");
}

// === Nested array access tests ===

#[test]
fn test_static_property_read_write() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public static int $count = 1;
}
echo Counter::$count;
Counter::$count = 5;
echo Counter::$count;
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_static_property_self_access_in_static_method() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public static int $count = 1;
    public static function bump() {
        self::$count = self::$count + 1;
        return self::$count;
    }
}
echo Counter::bump();
echo Counter::bump();
"#,
    );
    assert_eq!(out, "23");
}

#[test]
fn test_static_property_parent_access_in_static_method() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected static int $seed = 4;
}
class Child extends Base {
    public static function read() {
        return parent::$seed;
    }
}
echo Child::read();
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_static_property_inherited_storage_is_shared() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static int $count = 2;
    public static function set($value) {
        static::$count = $value;
    }
}
class Child extends Base {}
Child::set(9);
echo Base::$count;
echo Child::$count;
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_static_property_redeclaration_uses_child_storage() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static int $count = 1;
    public static function get() {
        return static::$count;
    }
    public static function set($value) {
        static::$count = $value;
    }
}
class Child extends Base {
    public static int $count = 2;
}
echo Base::get() . ":" . Child::get() . ":";
Child::set(9);
echo Base::$count . ":" . Child::$count;
"#,
    );
    assert_eq!(out, "1:2:1:9");
}

#[test]
fn test_static_property_direct_array_writes() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static array $items = [];
}
Registry::$items[] = 4;
Registry::$items[] = 5;
Registry::$items[1] = 8;
echo Registry::$items[0] . ":" . Registry::$items[1];
"#,
    );
    assert_eq!(out, "4:8");
}

#[test]
fn test_static_property_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public static int $count = 4;
}
Counter::$count += 6;
Counter::$count *= 2;
echo Counter::$count;
"#,
    );
    assert_eq!(out, "20");
}

#[test]
fn test_static_property_array_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [3, 5, 7];
}
Registry::$items[0] += 9;
Registry::$items[2] -= 4;
echo Registry::$items[0] . ":" . Registry::$items[2];
"#,
    );
    assert_eq!(out, "12:3");
}

#[test]
fn test_static_property_array_compound_assign_evaluates_index_once() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [3, 5, 7];
}

function idx() {
    echo "i";
    return 1;
}

Registry::$items[idx()] += 6;
echo ":" . Registry::$items[1];
"#,
    );
    assert_eq!(out, "i:11");
}

#[test]
fn test_static_property_redeclared_array_writes_are_late_bound() {
    let out = compile_and_run(
        r#"<?php
class BaseBag {
    public static array $items = [];
    public static function add($value) {
        static::$items[] = $value;
    }
    public static function replaceFirst($value) {
        static::$items[0] = $value;
    }
    public static function first() {
        return static::$items[0];
    }
}
class ChildBag extends BaseBag {
    public static array $items = [];
}
BaseBag::add(1);
ChildBag::add(2);
ChildBag::replaceFirst(7);
echo BaseBag::first() . ":" . ChildBag::first();
"#,
    );
    assert_eq!(out, "1:7");
}

#[test]
fn test_static_property_late_bound_private_redeclaration_read_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Base {
    private static int $count = 1;
    public static function read() {
        echo static::$count;
    }
}
class Child extends Base {
    private static int $count = 2;
}
Child::read();
"#,
    );
    assert!(
        err.contains("Cannot access private static property"),
        "{err}"
    );
}

#[test]
fn test_static_property_late_bound_private_redeclaration_write_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Base {
    private static int $count = 1;
    public static function write() {
        static::$count = 3;
    }
}
class Child extends Base {
    private static int $count = 2;
}
Child::write();
"#,
    );
    assert!(
        err.contains("Cannot access private static property"),
        "{err}"
    );
}

#[test]
fn test_static_string_property_assignment() {
    let out = compile_and_run(
        r#"<?php
class Labels {
    public static string $name = "a";
}
echo Labels::$name;
Labels::$name = "bc";
echo Labels::$name;
"#,
    );
    assert_eq!(out, "abc");
}
