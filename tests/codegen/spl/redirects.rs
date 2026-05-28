//! Purpose:
//! Codegen regressions for SPL-related builtins that redirect to compiler runtime helpers.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - Fixtures keep SPL builtin behavior covered without relying on autoload-specific setup.

use crate::support::*;

/// Verifies `count()` calls `Countable::count()` on a class implementing Countable,
/// returning the method's result rather than a fixed value.
#[test]
fn test_count_dispatches_to_countable_method() {
    let out = compile_and_run(
        r#"<?php
class Counter implements Countable {
    public function __construct(private int $n) {}
    public function count(): int { return $this->n * 10; }
}
$c = new Counter(7);
echo count($c);
"#,
    );
    assert_eq!(out, "70");
}

/// Verifies `count()` still returns the correct length when applied to a plain array
/// (regression: Countable dispatch must not interfere with array counting).
#[test]
fn test_count_on_array_still_works() {
    let out = compile_and_run("<?php echo count([10, 20, 30]);");
    assert_eq!(out, "3");
}

/// Verifies `count()` resolves the most-derived `Countable::count()` override
/// through inheritance, not just the interface-level implementation.
#[test]
fn test_count_polymorphic_via_inheritance() {
    let out = compile_and_run(
        r#"<?php
class Base implements Countable {
    public function __construct(protected int $n) {}
    public function count(): int { return $this->n; }
}
class Doubled extends Base {
    public function count(): int { return $this->n * 2; }
}
$base = new Base(5);
$doubled = new Doubled(5);
echo count($base);
echo ":";
echo count($doubled);
"#,
    );
    assert_eq!(out, "5:10");
}

/// Verifies `count()` works on a class that implements `Countable` alongside
/// `OuterIterator` (which extends `Iterator`); the `Countable` contract is satisfied
/// even though `OuterIterator` is unrelated to counting.
#[test]
fn test_count_indirect_countable_via_interface_extension() {
    // A class implementing OuterIterator (which extends Iterator) and
    // also Countable should work with count(): the Countable contract is
    // satisfied even though OuterIterator is unrelated.
    let out = compile_and_run(
        r#"<?php
class Both implements Countable, OuterIterator {
    public function count(): int { return 42; }
    public function getInnerIterator(): ?Iterator { return null; }
    public function current(): mixed { return null; }
    public function key(): mixed { return 0; }
    public function next(): void {}
    public function valid(): bool { return false; }
    public function rewind(): void {}
}
$b = new Both();
echo count($b);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies `spl_object_id()` returns a stable ID for the same object across multiple
/// calls and a unique ID for different object instances.
#[test]
fn test_spl_object_id_accepts_concrete_object() {
    let out = compile_and_run(
        r#"<?php
class Box {}
$a = new Box();
$b = new Box();
echo (spl_object_id($a) === spl_object_id($a)) ? "stable" : "drift";
echo ":";
echo (spl_object_id($a) !== spl_object_id($b)) ? "unique" : "same";
"#,
    );
    assert_eq!(out, "stable:unique");
}

/// Verifies `spl_object_hash()` returns a stable hash for the same object across
/// multiple calls and a unique hash for different object instances.
#[test]
fn test_spl_object_hash_accepts_concrete_object() {
    let out = compile_and_run(
        r#"<?php
class Box {}
$a = new Box();
$b = new Box();
echo (spl_object_hash($a) === spl_object_hash($a)) ? "stable" : "drift";
echo ":";
echo (spl_object_hash($a) !== spl_object_hash($b)) ? "unique" : "same";
"#,
    );
    assert_eq!(out, "stable:unique");
}
