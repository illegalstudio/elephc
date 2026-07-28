//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP class modifiers and properties, including readonly class constructor initialization, final class instantiates and dispatches methods, and final method dispatches normally without override.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Verifies that a `readonly` class permits property initialization inside its constructor.
/// The property is assigned in `__construct` and read back via `$user->id`.
#[test]
fn test_readonly_class_constructor_initialization() {
    let out = compile_and_run(
        r#"<?php
readonly class User {
    public $id;

    public function __construct($id) {
        $this->id = $id;
    }
}

$user = new User(42);
echo $user->id;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a `final` class can be instantiated and that method calls on the
/// resulting object dispatch correctly (no vtable override possible).
#[test]
fn test_final_class_instantiates_and_dispatches_methods() {
    let out = compile_and_run(
        r#"<?php
final class Receipt {
    public $code = 41;

    public function next() {
        return $this->code + 1;
    }
}

$receipt = new Receipt();
echo $receipt->next();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a `final` method on a base class is callable on a child instance
/// and does not permit overriding. The child defines a separate method to confirm
/// the child object is fully functional.
#[test]
fn test_final_method_dispatches_normally_without_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    final public function label() {
        return "base";
    }
}

class Child extends Base {
    public function suffix() {
        return "child";
    }
}

$child = new Child();
echo $child->label();
echo ":";
echo $child->suffix();
"#,
    );
    assert_eq!(out, "base:child");
}

/// Verifies that a `final` property on a base class is readable by a child instance
/// and that a method on the child can read and augment the property value.
#[test]
fn test_final_property_reads_normally_without_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    final public $value = 40;

    public function value() {
        return $this->value + 2;
    }
}

class Child extends Base {
    public function label() {
        return "answer:";
    }
}

$child = new Child();
echo $child->label();
echo $child->value();
"#,
    );
    assert_eq!(out, "answer:42");
}

/// Verifies typed instance properties with mixed initialization strategies:
/// constructor-only assignment, class-level defaults, nullable with null default,
/// and that reading then assigning via `$user->email` produces the expected output.
#[test]
fn test_typed_properties_defaults_constructor_assignment_and_nullable() {
    let out = compile_and_run(
        r#"<?php
class User {
    public int $id;
    public string $name = "Ada";
    public ?string $email = null;

    public function __construct($id) {
        $this->id = $id;
    }

    public function label() {
        return $this->name . ":" . $this->id;
    }
}

$user = new User(42);
echo $user->label();
echo ":";
echo is_null($user->email);
$user->email = "ada@example.test";
echo ":";
echo $user->email;
"#,
    );
    assert_eq!(out, "Ada:42:1:ada@example.test");
}

/// Verifies that accessing a typed instance property before it is initialized
/// throws a catchable `Error` that `catch(\Error $e)` can observe.
#[test]
fn test_uninitialized_typed_instance_property_is_fatal() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $value;
}

$box = new Box();
try {
    echo $box->value;
} catch (\Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert!(
        out.contains("Typed property Box::$value must not be accessed before initialization"),
        "{out}"
    );
}

/// Verifies that explicitly assigning `0` to a typed instance property constitutes
/// valid initialization and the property reads back as `0`.
#[test]
fn test_typed_instance_property_initialized_to_zero_reads_normally() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $value;
}

$box = new Box();
$box->value = 0;
echo $box->value;
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies that accessing an uninitialized typed static property throws a
/// catchable `Error` that `catch(\Error $e)` can observe.
#[test]
fn test_uninitialized_typed_static_property_is_fatal() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public static int $value;
}

try {
    echo Box::$value;
} catch (\Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert!(
        out.contains("Typed static property Box::$value must not be accessed before initialization"),
        "{out}"
    );
}

/// Verifies that accessing an uninitialized typed property and catching `Error`
/// allows continued execution after the property is initialized (issue #339).
#[test]
fn test_uninitialized_property_catch_then_continue() {
    let out = compile_and_run(
        r#"<?php
class C { public int $x; }
$c = new C();
try { echo $c->x; } catch (\Error $e) { echo "e"; }
$c->x = 5;
echo $c->x;
"#,
    );
    assert_eq!(out, "e5");
}

/// Verifies that catching `Exception` (not `Error`) does NOT catch the
/// uninitialized typed property access (issue #339).
#[test]
fn test_uninitialized_property_catch_exception_does_not_match() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class C { public int $x; }
$c = new C();
try { echo $c->x; } catch (\Exception $e) { echo "caught"; }
echo "end";
"#,
    );
    assert!(err.contains("uncaught"), "{err}");
}
/// Verifies a typed static property explicitly assigned zero remains readable.
#[test]
fn test_typed_static_property_initialized_to_zero_reads_normally() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public static int $value;
}

Box::$value = 0;
echo Box::$value;
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies that a nullable typed static property with an explicit `= null` default
/// is considered initialized (`is_null()` returns true), and that a typed static
/// property without a default remains uninitialized and throws a catchable Error;
/// without a try/catch the no-handler fast path still reports the specific
/// fatal diagnostic (issue #339).
#[test]
fn test_nullable_static_property_default_null_is_initialized() {
    let out = compile_and_run(
        r#"<?php
class WithDefault {
    public static ?int $value = null;
}

echo is_null(WithDefault::$value);
"#,
    );
    assert_eq!(out, "1");

    let err = compile_and_run_expect_failure(
        r#"<?php
class WithoutDefault {
    public static ?int $value;
}

echo WithoutDefault::$value;
"#,
    );
    assert!(
        err.contains("Fatal error: Typed static property WithoutDefault::$value must not be accessed before initialization"),
        "{err}"
    );
}

/// Verifies that an untyped instance property with a `= null` default is strictly
/// null (`=== null` is true, `== null` is true) and that `var_dump` emits `NULL`.
#[test]
fn test_untyped_null_property_default_is_strictly_null() {
    let out = compile_and_run(
        r#"<?php
class A { public $x = null; }
$a = new A();
var_dump($a->x);
echo is_null($a->x) ? "y" : "n", "\n";
echo ($a->x === null) ? "y" : "n", "\n";
echo ($a->x !== null) ? "y" : "n", "\n";
echo ($a->x == null) ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "NULL\ny\ny\nn\ny\n");
}

/// Verifies that an untyped static property with a `= null` default is strictly
/// null (`=== null` is true, `== null` is true) and that `var_dump` emits `NULL`.
#[test]
fn test_untyped_static_null_property_default_is_strictly_null() {
    let out = compile_and_run(
        r#"<?php
class A { public static $x = null; }
var_dump(A::$x);
echo is_null(A::$x) ? "y" : "n", "\n";
echo (A::$x === null) ? "y" : "n", "\n";
echo (A::$x !== null) ? "y" : "n", "\n";
echo (A::$x == null) ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "NULL\ny\ny\nn\ny\n");
}

/// Verifies that assigning `null` to an untyped instance or static property that
/// previously held a non-null value results in a strictly-null value
/// (`=== null` is true).
#[test]
fn test_untyped_property_assignment_to_null_is_strictly_null() {
    let out = compile_and_run(
        r#"<?php
class A {
    public $x = 1;
    public static $y = 1;
}
$a = new A();
$a->x = null;
A::$y = null;
echo is_null($a->x) ? "y" : "n", "\n";
echo ($a->x === null) ? "y" : "n", "\n";
echo is_null(A::$y) ? "y" : "n", "\n";
echo (A::$y === null) ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "y\ny\ny\ny\n");
}

/// Verifies that a static property on a `readonly` class is mutable
/// (readonly only affects instance properties, not static ones).
#[test]
fn test_readonly_class_static_property_is_mutable() {
    let out = compile_and_run(
        r#"<?php
readonly class Counter {
    public static int $count = 0;
}
Counter::$count = 5;
echo Counter::$count;
Counter::$count = Counter::$count + 1;
echo ":";
echo Counter::$count;
"#,
    );
    assert_eq!(out, "5:6");
}

/// Verifies that a static property on an `abstract readonly` class is mutable,
/// matching the behaviour of plain `readonly` classes.
#[test]
fn test_readonly_abstract_class_static_property_is_mutable() {
    let out = compile_and_run(
        r#"<?php
abstract readonly class Counter {
    public static int $count = 0;
}
Counter::$count = 7;
echo Counter::$count;
Counter::$count = Counter::$count + 1;
echo ":";
echo Counter::$count;
"#,
    );
    assert_eq!(out, "7:8");
}

/// Verifies that a static property inherited through a `readonly` child class
/// remains mutable and that both base and child share the same static slot.
#[test]
fn test_readonly_inherited_static_property_remains_mutable() {
    let out = compile_and_run(
        r#"<?php
readonly class Base {
    public static int $shared = 1;
}
readonly class Child extends Base {
}
Child::$shared = 42;
echo Base::$shared;
echo ":";
echo Child::$shared;
"#,
    );
    assert_eq!(out, "42:42");
}

/// End-to-end smoke test using the checked-in `examples/final-classes/main.php`
/// fixture. Verifies the example compiles, runs, and produces `"invoice:42\n"`.
#[test]
fn test_example_final_classes_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/final-classes/main.php"));
    assert_eq!(out, "invoice:42\n");
}

/// End-to-end smoke test using the checked-in `examples/typed-properties/main.php`
/// fixture. Verifies the example compiles, runs, and produces `"Ada:42\nmissing email\n"`.
#[test]
fn test_example_typed_properties_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/typed-properties/main.php"));
    let eol = target().platform.php_eol();
    assert_eq!(out, format!("Ada:42{eol}missing email{eol}"));
}

/// Verifies PHP 8.4 asymmetric visibility at runtime: a `public private(set)` property is
/// writable from inside the class and readable from outside.
#[test]
fn test_asymmetric_visibility_internal_write_external_read() {
    let out = compile_and_run(
        "<?php
        class Counter {
            public private(set) int $value = 0;
            public function increment(): void { $this->value = $this->value + 1; }
        }
        $c = new Counter();
        $c->increment();
        $c->increment();
        echo $c->value;
        ",
    );
    assert_eq!(out, "2");
}

/// Verifies that a subclass may write a `protected(set)` property inherited from its parent.
#[test]
fn test_asymmetric_visibility_protected_set_subclass_write() {
    let out = compile_and_run(
        "<?php
        class Base { public protected(set) string $name = \"base\"; }
        class Derived extends Base {
            public function rename(string $n): void { $this->name = $n; }
        }
        $d = new Derived();
        $d->rename(\"derived\");
        echo $d->name;
        ",
    );
    assert_eq!(out, "derived");
}

/// Compiles and runs the checked-in `examples/asymmetric-visibility/main.php` fixture, which
/// models an account whose balance is publicly readable but only privately writable.
#[test]
fn test_example_asymmetric_visibility_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/asymmetric-visibility/main.php"));
    assert_eq!(out, "balance: 120\ninsufficient funds\nbalance: 120\n");
}
