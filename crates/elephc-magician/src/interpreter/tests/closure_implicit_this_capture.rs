//! Purpose:
//! Pins `$this` capture for a NON-static closure or arrow function declared inside a method: PHP
//! binds `$this` into such a closure automatically, the same way it binds the lexical and
//! late-static class scopes, and elephc had never captured it at all.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::closure_implicit_this_capture`.
//!
//! Key details:
//! - Every expected value is `php -n` 8.5.6's observed output on the same fragment (see
//!   `scratchpad/thisprobes/*.php` and `scratchpad/r200/sess_this_*.php` for the exact php-vs-elephc
//!   differential measurements this file distills into unit tests).
//! - The bug had two independent halves. First, `EvalClosure` had no field at all for `$this` --
//!   `eval_closure_expr` recorded the lexical/late-static class scopes at the declaration site
//!   (`set_declaring_class_scopes`) but never looked up `$this` in the enclosing scope, because a
//!   closure's own `use (...)` capture list and an arrow function's *implicit* capture list both
//!   explicitly exclude it (`infer_arrow_closure_captures` filters `"this"` out on purpose --
//!   PHP's automatic `$this` binding is not a `use` capture). Second, even with that value
//!   captured, the dispatch path an ordinary `$f()` variable call actually takes --
//!   `eval_dynamic_call` -> `eval_callable` -> `EvaluatedCallable::Named` ->
//!   `eval_evaluated_callable_with_call_array_args`'s `Named` arm in `array_dispatch.rs` -- called
//!   `eval_closure_with_evaluated_args_and_bound_scope_ref_mode` unconditionally, which hardcodes
//!   `this_object: None` and folds `called_class` down to the closure's own LEXICAL scope. That
//!   second half is why a closure declared in a parent class method, but invoked on a child
//!   instance, resolved `static::class` to the parent: the fix routes a closure with a captured
//!   `$this` through the already-correct `..._bound_this_scope_ref_mode` path instead, which
//!   derives the late-static class from the captured object's own runtime class.
//! - `Closure::bind`/`bindTo`/`->call()` were NOT part of this bug (`eval_bound_closure_with_call_args*`
//!   in `execution.rs` already threaded an explicit `bound_this` through); they are pinned here
//!   only as a family regression, unaffected by this change.
//! - A `static function`/`static fn` must keep failing on `$this`, even when one is available in
//!   the surrounding scope (a static closure/arrow declared inside a live instance method): the
//!   capture in `eval_closure_expr` is gated on `!is_static` for exactly this reason. Both refusal
//!   tests read `isset($this)` rather than a property through `$this`, because a property read on
//!   an unbound `$this` hits an unrelated, pre-existing `FakeOps` gap (it answers
//!   `UnsupportedConstruct` for a property read on `null`, where the real runtime warns and yields
//!   null) that is orthogonal to this fix. `php -n` itself raises a dedicated, catchable
//!   `Error: Using $this when not in object context` for a property read there -- elephc raises a
//!   generic `Undefined variable $this` warning instead, with no "not in object context"
//!   diagnostic anywhere in the interpreter; that gap predates this fix and is out of scope here.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse closure this-capture fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute closure this-capture fragment");
    values.output.clone()
}

/// Verifies a non-static closure declared inside a method reads `$this` on a normal `$f()` call.
///
/// `php -n` 8.5.6 prints `this_in_closure:T`. Before the fix this fataled: reading `$this` warned
/// `Undefined variable $this`, the closure returned null, and the declared `: string` return type
/// on `viaClosure()` then raised an uncaught `TypeError`.
#[test]
fn a_non_static_closure_reads_this_declared_inside_a_method() {
    assert_eq!(
        out(
            br#"class Box {
    private string $tag = "T";
    public function viaClosure(): string {
        $f = function (): string { return $this->tag; };
        return $f();
    }
}
echo "this_in_closure:", (new Box())->viaClosure(), "\n";
echo "sentinel";"#
        ),
        "this_in_closure:T\nsentinel",
    );
}

/// Verifies a non-static closure's WRITE through `$this` reaches the real object, not a copy.
///
/// `php -n` 8.5.6 prints `items:a,b`. Reading `$b->items` from OUTSIDE the closure, after two
/// calls that each append through `$this` from inside it, is what discriminates a real reference
/// from a copy: a copy would leave `$b->items` empty while silently mutating a throwaway object.
#[test]
fn a_non_static_closure_write_through_this_reaches_the_real_object() {
    assert_eq!(
        out(
            br#"class Box {
    public array $items = [];
    public function addVia(string $v): void {
        $f = function (string $x) { $this->items[] = $x; };
        $f($v);
    }
}
$b = new Box();
$b->addVia("a");
$b->addVia("b");
echo "items:", implode(",", $b->items), "\n";
echo "sentinel";"#
        ),
        "items:a,b\nsentinel",
    );
}

/// Verifies an arrow function implicitly captures `$this` the same way a closure does.
///
/// `php -n` 8.5.6 prints `this_in_arrow:T`. `infer_arrow_closure_captures` deliberately excludes
/// `"this"` from an arrow function's inferred `use`-like capture list (PHP's automatic binding is
/// not a `use` capture), which is exactly why this needs its own declaration-site capture rather
/// than piggybacking on the arrow function's normal capture inference.
#[test]
fn an_arrow_function_captures_this_implicitly() {
    assert_eq!(
        out(
            br#"class Box {
    private string $tag = "T";
    public function viaArrow(): string {
        $f = fn () => $this->tag;
        return $f();
    }
}
echo "this_in_arrow:", (new Box())->viaArrow(), "\n";
echo "sentinel";"#
        ),
        "this_in_arrow:T\nsentinel",
    );
}

/// Verifies `$this` inside a closure nested in another (non-static) closure still resolves.
///
/// `php -n` 8.5.6 prints `this_in_nested:nested-T`. The inner closure's own declaration-site
/// capture must see `$this` bound into the OUTER closure's activation scope by this same fix.
#[test]
fn this_resolves_inside_a_closure_nested_in_another_closure() {
    assert_eq!(
        out(
            br#"class Box {
    private string $tag = "nested-T";
    public function viaNested(): string {
        $outer = function () {
            $inner = function () { return $this->tag; };
            return $inner();
        };
        return $outer();
    }
}
echo "this_in_nested:", (new Box())->viaNested(), "\n";
echo "sentinel";"#
        ),
        "this_in_nested:nested-T\nsentinel",
    );
}

/// Verifies a closure declared in a PARENT class method, invoked on a CHILD instance, sees `self::`
/// as the declaring (parent) class and `static::` as the child's own runtime class.
///
/// `php -n` 8.5.6 prints `parent_child:child-tag:ParentBox:ChildBox`. Before the fix this printed
/// `parent_child:` (empty -- `$this` unresolved) `:ParentBox:ParentBox`: the dispatch path an
/// ordinary `$f()` call takes folded `called_class` down to the closure's own lexical scope
/// instead of the bound object's runtime class, losing late static binding along with `$this`.
#[test]
fn a_closure_from_a_parent_method_sees_static_as_the_child_instance() {
    assert_eq!(
        out(
            br#"class ParentBox {
    protected string $tag = "parent-tag";
    public function makeClosure() {
        return function () {
            return $this->tag . ":" . self::class . ":" . static::class;
        };
    }
}
class ChildBox extends ParentBox {
    protected string $tag = "child-tag";
}
$child = new ChildBox();
$f = $child->makeClosure();
echo "parent_child:", $f(), "\n";
echo "sentinel";"#
        ),
        "parent_child:child-tag:ParentBox:ChildBox\nsentinel",
    );
}

/// Verifies `Closure::bind`, `bindTo`, and `->call()` still rebind `$this` -- including granting
/// access to a PRIVATE member through the scope argument -- unaffected by the implicit-capture fix.
///
/// `php -n` 8.5.6 prints `bind:other-private`, `bindTo:other-private`, `call:other-private`. This
/// family member was never broken (`eval_bound_closure_with_call_args*` already threaded an
/// explicit `bound_this` through `eval_closure_with_evaluated_args_and_bound_this_scope*`); it is
/// pinned here as a regression guard, not as a red-before-green case.
#[test]
fn closure_bind_bindto_and_call_rebind_this_with_private_scope_access() {
    assert_eq!(
        out(
            br#"class Box {
    private string $tag = "orig";
    public function make() {
        return function () { return $this->tag; };
    }
}
class Other {
    private string $tag = "other-private";
}
$box = new Box();
$closure = $box->make();
$bound = Closure::bind($closure, new Other(), Other::class);
echo "bind:", $bound(), "\n";
$boundTo = $closure->bindTo(new Other(), Other::class);
echo "bindTo:", $boundTo(), "\n";
echo "call:", $closure->call(new Other()), "\n";
echo "sentinel";"#
        ),
        "bind:other-private\nbindTo:other-private\ncall:other-private\nsentinel",
    );
}

/// Verifies a `static function` declared inside a static method still has no `$this` to capture.
///
/// `php -n` 8.5.6 prints `none`. `isset($this)` is used rather than reading a property through
/// `$this` because property access on an unbound `$this` hits an unrelated, pre-existing gap in
/// this test double (`FakeOps` answers `UnsupportedConstruct` for a property read on `null`,
/// where the real runtime warns and yields null) -- orthogonal to this fix, and reading `isset()`
/// instead measures exactly the invariant that matters here: whether `$this` got bound at all.
#[test]
fn a_static_closure_inside_a_static_method_still_has_no_this() {
    assert_eq!(
        out(
            br#"class Box {
    public static function makeStatic() {
        $f = static function () { return isset($this) ? "has" : "none"; };
        return $f();
    }
}
echo Box::makeStatic(), "\n";
echo "sentinel";"#
        ),
        "none\nsentinel",
    );
}

/// Verifies a `static fn` declared inside a live INSTANCE method still refuses `$this`, even
/// though `$this` is available right there in the enclosing scope for the capture to (wrongly)
/// pick up. This is the regression the fix must not cause: `!is_static` gates the capture in
/// `eval_closure_expr` for exactly this reason.
///
/// `php -n` 8.5.6 prints `none`; see the previous test's note on why `isset($this)` rather than a
/// property read.
#[test]
fn a_static_arrow_inside_an_instance_method_still_refuses_this() {
    assert_eq!(
        out(
            br#"class Box {
    private string $tag = "T";
    public function viaStaticArrow() {
        $f = static fn () => isset($this) ? "has" : "none";
        return $f();
    }
}
echo (new Box())->viaStaticArrow(), "\n";
echo "sentinel";"#
        ),
        "none\nsentinel",
    );
}
