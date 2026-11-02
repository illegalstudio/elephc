//! Purpose:
//! Interpreter tests pinning the covariance family around a return/parameter type narrowed to a
//! native PHP CLASS (not just a builtin interface) -- `IteratorAggregate::getIterator():
//! Traversable` implemented as `getIterator(): \ArrayIterator`, which is php's own covariance and
//! the exact spelling `Symfony\Component\HttpFoundation\ParameterBag`, `HeaderBag`, and
//! `RouteCollection` all write.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::native_class_return_type_covariance`.
//!
//! Key details:
//! - Every expected string/status is `php -n` 8.5.6's observed output or fatal on the same
//!   fragment, never "no error" alone.
//! - The refusal was at CLASS DECLARATION: `eval_return_class_type_is_a`
//!   (`crate::interpreter::return_type_compat`) resolves a declared class-like return atom by
//!   walking eval-declared classes (`context.has_class`), eval-declared interfaces
//!   (`context.has_interface`), and a hard-coded builtin INTERFACE parent table
//!   (`builtin_class_like_parent_names`, consulted through `interface_parent_names`). `ArrayIterator`
//!   is a native CLASS with no eval declaration at all, so it reached none of those branches --
//!   `builtin_class_like_parent_names("ArrayIterator")` answers `&[]` because the table only knows
//!   builtin INTERFACE names (`Iterator`, `IteratorAggregate`, ...), not concrete native classes.
//!   `eval_array_iterator_class_is_a` (`crate::context::classlike_objects`) already answers this
//!   exact question for `instanceof` (`dynamic_object_is_a`) and says in its own doc comment that
//!   "both the `instanceof` answer and the declared-return-type check need it" -- but the
//!   declared-return-type check never called it. This is a DIFFERENT validator than the one fixed
//!   in `282bfca042` (which corrected `ClassInfo.abstract_methods` / the AOT reflection ABSTRACT
//!   bit read for parent-abstract-method presence): this one lives entirely in
//!   `return_type_compat.rs`'s class-like-name resolver and never touches AOT reflection metadata
//!   for native classes at all -- it is a sibling gap in the SIGNATURE COMPATIBILITY validator, not
//!   the same code path.
//! - `class_method_signature_accepts` (parent-class path, both eval-parent and AOT-native-parent)
//!   and `class_method_satisfies_interface_signature_with_return_mode` (interface path) both
//!   bottom out in the same `method_return_type_signature_accepts` / `eval_return_type_accepts` /
//!   `eval_return_class_type_is_a` chain, so the interface case and the parent-class case share one
//!   fix. Parameter contravariance shares the same chain too (with expected/actual swapped), which
//!   is why the parameter-widening test below was red for the same reason as the return-type ones.
//! - The refusal-path tests (unrelated return type, narrowed parameter, void/non-void mismatch,
//!   nullable widening) do NOT depend on `ArrayIterator` and were already correct before this fix;
//!   they are pinned here so a fix broad enough to accept everything cannot pass silently.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse covariance fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute covariance fragment");
    values.output.clone()
}

/// Runs one fragment and returns the status the declaration failed with.
fn err(fragment: &[u8]) -> EvalStatus {
    let program = parse_fragment(fragment).expect("parse covariance fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect_err("declaration should be refused")
}

/// Verifies `getIterator(): \ArrayIterator` narrows `IteratorAggregate::getIterator(): Traversable`.
///
/// `php -n` 8.5.6 prints `a=1;b=2;T`. This is the reduced Symfony shape: `ParameterBag`,
/// `HeaderBag`, and `RouteCollection` all declare exactly this.
#[test]
fn a_get_iterator_narrowed_to_array_iterator_via_interface_declares_and_runs() {
    assert_eq!(
        out(
            br#"class GIA1 implements \IteratorAggregate {
    private array $items = ["a" => 1, "b" => 2];
    public function getIterator(): \ArrayIterator { return new \ArrayIterator($this->items); }
}
foreach (new GIA1() as $k => $v) { echo $k, "=", $v, ";"; }
echo (new GIA1()) instanceof \Traversable ? "T" : "t";"#
        ),
        "a=1;b=2;T",
    );
}

/// Verifies the same narrowing declares alongside a second interface (`Countable`).
///
/// `php -n` 8.5.6 prints `3;x=1,y=2,z=3,`. Symfony's bags implement both
/// `IteratorAggregate` and `Countable` on the same class, so this is the exact shape rather than
/// a simplification of it.
#[test]
fn a_get_iterator_narrowed_to_array_iterator_across_two_interfaces_declares() {
    assert_eq!(
        out(
            br#"class GIA4 implements \IteratorAggregate, \Countable {
    private array $items = ["x" => 1, "y" => 2, "z" => 3];
    public function getIterator(): \ArrayIterator { return new \ArrayIterator($this->items); }
    public function count(): int { return count($this->items); }
}
$o = new GIA4();
echo count($o), ";";
foreach ($o as $k => $v) { echo $k, "=", $v, ","; }"#
        ),
        "3;x=1,y=2,z=3,",
    );
}

/// Verifies the narrowing also declares across a PARENT CLASS override, not just an interface.
///
/// `php -n` 8.5.6 prints `y=2;done`. `class_method_signature_accepts` (the parent-class path) and
/// the interface path share the same return-type resolver, so this exercises the same fix from a
/// different caller.
#[test]
fn a_parent_class_method_narrowed_to_array_iterator_declares_and_runs() {
    assert_eq!(
        out(
            br#"class GPBase { public function make(): \Traversable { return new \ArrayIterator(["x" => 1]); } }
class GCChild extends GPBase { public function make(): \ArrayIterator { return new \ArrayIterator(["y" => 2]); } }
$c = new GCChild();
foreach ($c->make() as $k => $v) { echo $k, "=", $v, ";"; }
echo "done";"#
        ),
        "y=2;done",
    );
}

/// Verifies `static` may replace a parent's own `self` return type -- unrelated to the
/// `ArrayIterator` gap, pinned as a regression guard for the same resolver.
///
/// `php -n` 8.5.6 prints `GSub;sub`. This case was already correct before this fix: `static`
/// resolves against the OVERRIDING class, so `eval_return_class_type_is_a` reaches
/// `context.class_is_a` for an eval-declared child rather than the native-class gap this fix
/// closes.
#[test]
fn a_return_type_narrowed_to_static_replacing_the_parents_own_class_name_declares() {
    assert_eq!(
        out(
            br#"class GBase { public function make(): self { return new self(); } public function name(): string { return "base"; } }
class GSub extends GBase { public function make(): static { return new static(); } public function name(): string { return "sub"; } }
$s = new GSub();
echo get_class($s->make()), ";", $s->make()->name();"#
        ),
        "GSub;sub",
    );
}

/// Verifies parameter CONTRAVARIANCE: an override may WIDEN a parameter the interface narrowed.
///
/// `php -n` 8.5.6 prints `took;done`. Parameter checking reverses expected/actual through the same
/// `eval_return_class_type_is_a` chain, so this shape needed `ArrayIterator IS-A Traversable` for
/// the same reason the return-type cases did, and was red for the same root cause.
#[test]
fn an_override_may_widen_a_parameter_type_the_interface_narrowed() {
    assert_eq!(
        out(
            br#"interface GI { public function take(\ArrayIterator $x): void; }
class GC implements GI { public function take(\Traversable $x): void { echo "took;"; } }
(new GC())->take(new \ArrayIterator([1]));
echo "done";"#
        ),
        "took;done",
    );
}

/// Verifies the opposite parameter direction is REFUSED: an override may not NARROW a parameter.
///
/// `php -n` 8.5.6: `Fatal error: Declaration of GC2::take(ArrayIterator $x): void must be
/// compatible with GI2::take(Traversable $x): void`. Unrelated to the `ArrayIterator` gap -- this
/// direction never needed `ArrayIterator IS-A Traversable`, only the reverse, which was already
/// false before this fix and must remain false after it.
#[test]
fn an_override_may_not_narrow_a_parameter_type_the_interface_widened() {
    assert_eq!(
        err(
            br#"interface GI2 { public function take(\Traversable $x): void; }
class GC2 implements GI2 { public function take(\ArrayIterator $x): void { echo "took;"; } }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}

/// Verifies a return type unrelated to the interface's target is REFUSED, not silently accepted.
///
/// `php -n` 8.5.6: `Fatal error: Declaration of GBad::make(): DateTime must be compatible with
/// GI3::make(): ArrayIterator`. A fix broad enough to always accept a native-class actual type
/// would pass every acceptance test above and lose this refusal -- `DateTime` is not a
/// `\ArrayIterator` under any resolution path.
#[test]
fn a_return_type_unrelated_to_the_interfaces_target_is_refused() {
    assert_eq!(
        err(
            br#"interface GI3 { public function make(): \ArrayIterator; }
class GBad implements GI3 { public function make(): \DateTime { return new \DateTime(); } }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}

/// Verifies a `void` requirement cannot be replaced by a concrete return type.
///
/// `php -n` 8.5.6: `Fatal error: Declaration of GC4::f(): int must be compatible with GI4::f():
/// void`. Unrelated to the `ArrayIterator` gap -- `eval_return_type_is_void` already special-cases
/// `void` before any class-like name resolution runs.
#[test]
fn an_interface_void_return_cannot_be_replaced_by_a_concrete_type() {
    assert_eq!(
        err(
            br#"interface GI4 { public function f(): void; }
class GC4 implements GI4 { public function f(): int { return 1; } }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}

/// Verifies the opposite `void` direction is refused too: `void` cannot replace a concrete type.
///
/// `php -n` 8.5.6: `Fatal error: Declaration of GC5::f(): void must be compatible with GI5::f():
/// int`.
#[test]
fn a_concrete_return_type_cannot_be_narrowed_to_void() {
    assert_eq!(
        err(
            br#"interface GI5 { public function f(): int; }
class GC5 implements GI5 { public function f(): void { } }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}

/// Verifies `never` may override ANY declared return type, including `void`.
///
/// `php -n` 8.5.6 prints `declared;caught:boom`. `never` is php's bottom type for return
/// covariance: `eval_return_type_is_never(actual_type)` already returns `true` unconditionally, so
/// this is unrelated to the `ArrayIterator` gap and pinned as a regression guard.
#[test]
fn a_never_return_may_override_any_declared_return_type() {
    assert_eq!(
        out(
            br#"interface GI6 { public function f(): void; }
class GC6 implements GI6 { public function f(): never { throw new \Exception("boom"); } }
echo "declared;";
try { (new GC6())->f(); } catch (\Throwable $e) { echo "caught:", $e->getMessage(); }"#
        ),
        "declared;caught:boom",
    );
}

/// Verifies a NULLABLE narrowing of a native class return type is refused, not accepted.
///
/// `php -n` 8.5.6: `Fatal error: Declaration of GC7::make(): ?ArrayIterator must be compatible
/// with GI7::make(): Traversable`. This guards that fixing `ArrayIterator IS-A Traversable` did
/// not also relax the non-null check: `eval_return_type_allows_null(actual) &&
/// !eval_return_type_allows_null(expected)` rejects before any class-like resolution is reached,
/// so `?ArrayIterator` must stay refused even though bare `ArrayIterator` is now accepted.
#[test]
fn a_nullable_narrowing_of_a_native_class_return_type_is_refused() {
    assert_eq!(
        err(
            br#"interface GI7 { public function make(): \Traversable; }
class GC7 implements GI7 { public function make(): ?\ArrayIterator { return null; } }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}
