//! Purpose:
//! Regression tests for dynamic method dispatch on receivers whose static type
//! does not name a single class (a `Mixed` value, or a union of object classes).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Before the fix, a method call on such a receiver emitted no dispatch and
//!   left a garbage value in the result register. Dispatch now reads the
//!   receiver's runtime class id and selects the matching class's method, so
//!   these fixtures assert PHP-equivalent stdout.

use super::*;

/// Verifies a method call on a `: mixed`-returning value dispatches on the runtime
/// class id (the value is an object), both via a local and chained directly.
#[test]
fn test_mixed_receiver_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
class S {
    public int $v;
    public function __construct(int $x) { $this->v = $x; }
    public function doubled(): int { return $this->v * 2; }
}
class C {
    public function make(int $x): mixed {
        if ($x < 0) { return false; }
        return new S($x);
    }
}
$c = new C();
$s = $c->make(5);
echo $s->doubled() . ";" . $c->make(7)->doubled();
"#,
    );
    assert_eq!(out, "10;14");
}

/// Verifies a method call on an `object|false` union receiver dispatches correctly.
#[test]
fn test_object_or_false_union_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
class S {
    public int $v;
    public function __construct(int $x) { $this->v = $x; }
    public function doubled(): int { return $this->v * 2; }
}
class C {
    public function make(int $x): S|bool {
        if ($x < 0) { return false; }
        return new S($x);
    }
}
$c = new C();
$s = $c->make(5);
echo ($s === false) ? "F" : $s->doubled();
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies a dynamic-receiver method call passes its arguments correctly (the
/// receiver and arguments are evaluated once and dispatched together).
#[test]
fn test_mixed_receiver_method_with_arguments() {
    let out = compile_and_run(
        r#"<?php
class S {
    public function add(int $a, int $b): int { return $a + $b; }
}
class C {
    public function make(): mixed { return new S(); }
}
$c = new C();
echo $c->make()->add(3, 4);
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies dynamic dispatch selects the correct class at runtime when several
/// classes define the same method.
#[test]
fn test_mixed_receiver_multiple_candidate_classes() {
    let out = compile_and_run(
        r#"<?php
class Dog { public function speak(): string { return "woof"; } }
class Cat { public function speak(): string { return "meow"; } }
function animal(int $n): mixed { return ($n == 0) ? new Dog() : new Cat(); }
echo animal(0)->speak() . ":" . animal(1)->speak();
"#,
    );
    assert_eq!(out, "woof:meow");
}

/// Verifies a dynamic-receiver method call returning a string works end to end.
#[test]
fn test_mixed_receiver_string_return() {
    let out = compile_and_run(
        r#"<?php
class S {
    public string $n;
    public function __construct(string $n) { $this->n = $n; }
    public function greet(): string { return "hi " . $this->n; }
}
class C {
    public function make(string $n): mixed { return new S($n); }
}
$c = new C();
echo $c->make("ada")->greet();
"#,
    );
    assert_eq!(out, "hi ada");
}

/// Verifies a dynamic-receiver method call on a non-object runtime value fatals
/// (PHP "Call to a member function ... on a non-object") instead of miscompiling.
#[test]
fn test_mixed_receiver_non_object_fatals() {
    let out = compile_and_run_capture(
        r#"<?php
class S { public function d(): int { return 1; } }
function make(int $x): mixed { if ($x < 0) { return false; } return new S(); }
$v = make(-1);
echo $v->d();
"#,
    );
    assert!(!out.success);
    // The harness splits by KIND PREFIX: a `Fatal error:` line is a diagnostic, and the trace
    // lines after it — which carry no prefix — are stdout. The message lives in the first half.
    //
    // php names the receiver's TYPE, and for a bool it names the VALUE: MEASURED on `php -n`
    // 8.5.6, `on false`, never `on bool` and never `on null`.
    assert!(
        out.diagnostics
            .contains("Call to a member function d() on false"),
        "{}",
        out.diagnostics
    );
}

/// php names what the receiver actually was, and `zend_zval_value_name` names a bool by VALUE.
///
/// MEASURED on `php -n` 8.5.6 over the seven shapes below. elephc said `on null` for all of them:
/// the site that refuses a non-object receiver had the runtime tag in hand — the unbox leaves it
/// there and the object guard only compares it — and threw away the answer.
#[test]
fn test_a_non_object_receiver_is_named_by_what_it_is() {
    let out = compile_and_run(
        r#"<?php
function pick(int $x): mixed
{
    return match ($x) {
        0 => null,
        1 => false,
        2 => true,
        3 => 42,
        4 => 1.5,
        5 => "s",
        default => [1],
    };
}
for ($i = 0; $i < 7; $i++) {
    $v = pick($i);
    try { $v->d(); } catch (Throwable $t) { echo $t->getMessage(), "|"; }
}
"#,
    );
    assert_eq!(
        out,
        "Call to a member function d() on null|\
         Call to a member function d() on false|\
         Call to a member function d() on true|\
         Call to a member function d() on int|\
         Call to a member function d() on float|\
         Call to a member function d() on string|\
         Call to a member function d() on array|"
    );
}

/// Regression: a user method whose name collides with a builtin method of a different arity (here
/// `add`, which `DateTime::add(DateInterval)` also defines) must still dispatch correctly for a
/// mixed receiver. The dispatch marshals arguments once with the first candidate's signature, so
/// candidates are filtered by argument arity; otherwise `DateTime::add` could be selected for this
/// 2-argument call depending on (nondeterministic) class-id ordering, corrupting the result.
#[test]
fn test_mixed_receiver_method_name_collides_with_builtin_arity() {
    let out = compile_and_run(
        r#"<?php
class Money { public function add(int $a, int $b): int { return $a + $b; } }
function make(): mixed { return new Money(); }
echo make()->add(40, 2);
"#,
    );
    assert_eq!(out, "42");
}
