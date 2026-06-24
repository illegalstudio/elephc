//! Purpose:
//! Integration tests for end-to-end codegen coverage of the PHP `clone` expression,
//! including shallow-copy independence, `__clone()` dispatch, inherited `__clone`,
//! dynamic properties, nested-object sharing, Mixed-boxed operands, and churn safety.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Every fixture is cross-checked against the PHP interpreter during development.

use super::*;

/// Verifies that `clone` produces an independent shallow copy: mutating a scalar
/// property on the clone leaves the source untouched, for int, string, and array
/// properties alike.
#[test]
fn test_clone_shallow_copy_independence() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n;
    public string $s;
    /** @var int[] */
    public array $a;
    public function __construct(int $n, string $s, array $a) {
        $this->n = $n;
        $this->s = $s;
        $this->a = $a;
    }
}
$x = new Box(1, "orig", [10, 20]);
$y = clone $x;
$y->n = 99;
$y->s = "changed";
$y->a[0] = 99;
echo $x->n, " ", $x->s, " ", $x->a[0], "\n";
echo $y->n, " ", $y->s, " ", $y->a[0], "\n";
"#,
    );
    assert_eq!(
        out,
        "1 orig 10\n99 changed 99\n"
    );
}

/// Verifies that `__clone()` is invoked exactly once on the freshly copied instance
/// and that it observes the clone (not the source) as `$this`.
#[test]
fn test_clone_invokes_clone_method_once() {
    let out = compile_and_run(
        r#"<?php
class P {
    public int $n;
    public function __construct(int $n) { $this->n = $n; }
    public function __clone() { echo "cloned\n"; $this->n = $this->n + 100; }
}
$a = new P(5);
$b = clone $a;
echo $a->n, "\n";
echo $b->n, "\n";
"#,
    );
    assert_eq!(out, "cloned\n5\n105\n");
}

/// Verifies that an inherited `__clone()` dispatches to the ancestor's emitted
/// implementation when the subclass declares none, matching PHP.
#[test]
fn test_clone_inherited_clone() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public int $n;
    public function __construct(int $n) { $this->n = $n; }
    public function __clone() { echo "base-clone\n"; $this->n = 777; }
}
class Child extends Base {}
$a = new Child(1);
$b = clone $a;
echo $a->n, "\n";
echo $b->n, "\n";
"#,
    );
    assert_eq!(out, "base-clone\n1\n777\n");
}

/// Verifies that `clone` is a shallow copy: object-typed properties are shared by
/// reference, so mutating the nested object through the clone is visible on the
/// source (the canonical PHP shallow-clone contract).
#[test]
fn test_clone_nested_object_is_shared() {
    let out = compile_and_run(
        r#"<?php
class Inner {
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
}
class Outer {
    public Inner $i;
    public function __construct(Inner $i) { $this->i = $i; }
}
$a = new Outer(new Inner(1));
$b = clone $a;
$b->i->v = 99;
echo $a->i->v, "\n";
echo $b->i->v, "\n";
"#,
    );
    assert_eq!(out, "99\n99\n");
}

/// Verifies that `#[\AllowDynamicProperties]` dynamic properties are cloned into an
/// independent container: reassigning a dynamic property on the clone does not alter
/// the source.
#[test]
fn test_clone_with_dynamic_properties() {
    let out = compile_and_run(
        r#"<?php
#[\AllowDynamicProperties]
class P {}
$a = new P();
$a->dyn = "first";
$b = clone $a;
$b->dyn = "second";
echo $a->dyn, "\n";
echo $b->dyn, "\n";
"#,
    );
    assert_eq!(out, "first\nsecond\n");
}

/// Verifies that `clone` works inside a function that returns the copy, with the
/// same independence and `__clone()` semantics as at the top level.
#[test]
fn test_clone_inside_function() {
    let out = compile_and_run(
        r#"<?php
class P {
    public int $n;
    public function __construct(int $n) { $this->n = $n; }
    public function __clone() { $this->n = $this->n * 2; }
}
function duplicate(P $p): P {
    return clone $p;
}
$a = new P(21);
$b = duplicate($a);
echo $a->n, "\n";
echo $b->n, "\n";
"#,
    );
    assert_eq!(out, "21\n42\n");
}

/// Verifies the Symfony `DeepClone` receiver shape: cloning an object read out of an
/// untyped (Mixed-element) array. The clone helper unboxes the Mixed cell, clones the
/// inner object, runs `__clone`, and reboxes the result.
#[test]
fn test_clone_mixed_boxed_object() {
    let out = compile_and_run(
        r#"<?php
class P {
    public int $n;
    public string $s;
    public function __construct(int $n, string $s) { $this->n = $n; $this->s = $s; }
    public function __clone() { echo "cloned\n"; }
}
$prototypes = [];
$prototypes[0] = new P(42, "hello");
$object = clone $prototypes[0];
echo $object->n, " ", $object->s, "\n";
$prototypes[0]->n = 99;
echo $object->n, "\n";
"#,
    );
    assert_eq!(out, "cloned\n42 hello\n42\n");
}

/// Verifies that repeated cloning under churn stays GC-clean: a tight loop cloning
/// objects with string, array, and object properties must not corrupt the heap or
/// double-free shared children. The final cloned value is asserted for correctness.
#[test]
fn test_clone_gc_clean_under_churn() {
    let out = compile_and_run(
        r#"<?php
class Inner {
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
}
class Box {
    public int $n;
    public string $s;
    public array $a;
    public Inner $i;
    public function __construct(int $n, string $s, array $a, Inner $i) {
        $this->n = $n;
        $this->s = $s;
        $this->a = $a;
        $this->i = $i;
    }
}
function churn(): int {
    $src = new Box(1, "x", [1, 2, 3], new Inner(7));
    $last = $src;
    for ($k = 0; $k < 2000; $k++) {
        $last = clone $src;
    }
    return $last->n + $last->i->v;
}
echo churn(), "\n";
"#,
    );
    assert_eq!(out, "8\n");
}