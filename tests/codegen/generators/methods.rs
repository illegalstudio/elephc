//! Purpose:
//! Regression tests for generator *methods*: a class method whose body contains
//! `yield` must be typed as returning a `Generator`, exactly like a generator
//! function, whether or not it carries a `: Generator` return hint.
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Before the fix the checker's method pass inferred `void` for an unhinted
//!    generator method (its body has no value `return`), so `foreach` over the
//!    call warned "null given" and never ran the loop; a `: Generator` hint hit
//!    the "must return a value on every path" coverage check and failed to
//!    compile at all. Free functions were unaffected, so every fixture here
//!    iterates a *method* result.
//!  - Expected values are real `LC_ALL=C php` 8.4 output.

use crate::support::*;

/// Verifies that a method whose body yields, declared with no return hint at
/// all, is still typed as a `Generator`: `foreach` over the call iterates the
/// yielded values with PHP's auto-incrementing keys instead of warning that the
/// method returned null.
#[test]
fn test_generator_method_without_return_hint_iterates() {
    let out = compile_and_run(
        r#"<?php
class Box {
    private array $items = [1, 2, 3];
    public function items() { foreach ($this->items as $i) { yield $i; } }
}
foreach ((new Box)->items() as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 1:2 2:3 ");
}

/// Verifies the same shape with an explicit `: Generator` return hint, plus a
/// static generator method and a generator method with a `return` value read
/// back through `getReturn()`. The hint used to trip the declared-return
/// coverage check, which a generator body legitimately cannot satisfy.
#[test]
fn test_generator_method_with_generator_return_hint_iterates() {
    let out = compile_and_run(
        r#"<?php
class Box {
    private array $items = [4, 5];
    public function items(): Generator { foreach ($this->items as $i) { yield $i; } }
    public static function letters(): Generator { yield "a"; yield "b"; }
    public function tally(): Generator { yield 1; return 7; }
}
$b = new Box();
foreach ($b->items() as $k => $v) { echo "$k:$v "; }
foreach (Box::letters() as $k => $v) { echo "$k:$v "; }
$g = $b->tally();
foreach ($g as $v) { echo $v; }
echo " ", $g->getReturn();
"#,
    );
    assert_eq!(out, "0:4 1:5 0:a 1:b 1 7");
}

/// Verifies generator methods reached through a trait and through an abstract
/// declaration: the trait method is flattened into the using class and the
/// override is checked against the abstract `: Generator` signature, so both
/// must survive the generator return-type override without a coverage error.
#[test]
fn test_generator_method_through_trait_and_abstract_override() {
    let out = compile_and_run(
        r#"<?php
trait Yielder { public function two(): Generator { yield "t0"; yield "t1"; } }
class Holder { use Yielder; }
abstract class Base { abstract public function seq(): Generator; }
class Impl extends Base { public function seq(): Generator { yield "i0"; } }
foreach ((new Holder)->two() as $k => $v) { echo "$k=$v "; }
foreach ((new Impl)->seq() as $k => $v) { echo "$k=$v "; }
"#,
    );
    assert_eq!(out, "0=t0 1=t1 0=i0 ");
}

/// Verifies a wider return hint that still accepts a `Generator` keeps working:
/// `iterable` is a supertype of `Generator`, so PHP accepts the declaration and
/// the method iterates normally.
#[test]
fn test_generator_method_with_iterable_return_hint() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function items(): iterable { yield 1; yield 2; }
}
foreach ((new Box)->items() as $k => $v) { echo "$k:$v "; }
"#,
    );
    assert_eq!(out, "0:1 1:2 ");
}

/// Verifies an assignment inside a yielded operand is visible to the rest of that operand.
///
/// Every compiled Twig template filters a value this way
/// (`yield (($tmp = f()) && $tmp instanceof Markup ? (string) $tmp : $tmp)`); the checker used to
/// infer the yielded operand without its assignment effects and refused the read of `$tmp` as an
/// undefined variable, which stopped symfony/demo's preload build. Expected output from php 8.5.
#[test]
fn test_yield_operand_assignment_is_visible_to_the_same_operand() {
    let out = compile_and_run(
        r#"<?php
class Markup { public function __toString(): string { return "m"; } }
class Rt { public function isRtl(): bool { return true; } public function title(): Markup { return new Markup(); } }
class Tpl {
    public function display(Rt $rt): iterable {
        yield (((($tmp = $rt->isRtl()) && $tmp instanceof Markup ? (string) $tmp : $tmp)) ? ("rtl") : ("ltr"));
        yield "|";
        yield (($tmp = $rt->title()) && $tmp instanceof Markup ? (string) $tmp : $tmp);
    }
}
foreach ((new Tpl())->display(new Rt()) as $chunk) { echo $chunk; }
"#,
    );
    assert_eq!(out, "rtl|m");
}
