//! Purpose:
//! Interpreter tests for a method declared `self`/`static` and called as a discarded
//! statement, whose result aliases its own receiver.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - `php -n` 8.5.6 is the oracle for every literal string asserted here.
//! - The fake runtime counts references (`FakeOps`), so an over-release panics the test instead
//!   of passing silently -- see `crates/elephc-magician/src/interpreter/tests/support/mod.rs`.

use super::super::*;
use super::support::*;

/// Verifies a discarded `: self` method call that returns `$this` neither frees the receiver
/// early nor double-releases it.
///
/// `php -n` 8.5.6 prints `count:2:drop:2:after` for this fragment: both discarded `add()` calls
/// land (`$o->i` has two elements), the object is destructed exactly once by the explicit
/// `unset()`, and nothing runs after that unset before the final echo. Before the fix, evaluating
/// `$o->add("a");` as a statement released a value that was never a fresh temporary -- it was
/// `$this`, the exact cell `$o` already owns -- because `EvalExpr::MethodCall`'s result is not on
/// `eval_expr_result_aliases_storage`'s exemption list and the receiver (`$o`, a `LoadVar`) is not
/// "temporary" either, so nothing ever retained the returned alias first. The counted fake destructs
/// the object a call early and again on the very next discarded call, then panics on the second
/// release ("released a fake cell nobody owned").
#[test]
fn discarded_self_returning_method_call_keeps_the_receiver_alive() {
    let program = parse_fragment(
        br#"class EvalFluentSelfDiscard {
    public array $i = [];
    public function __destruct() { echo "drop:" . count($this->i) . ":"; }
    public function add(string $v): self { $this->i[] = $v; return $this; }
}
$o = new EvalFluentSelfDiscard();
$o->add("a");
$o->add("b");
echo "count:" . count($o->i) . ":";
unset($o);
echo "after";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "count:2:drop:2:after");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert!(values.over_releases().is_empty());
}

/// Verifies a discarded `: static` method call that returns `$this` behaves identically to the
/// `self` case above.
///
/// `php -n` 8.5.6 prints `count:2:drop:2:after`, matching the `self` fixture: the discriminator is
/// the returned VALUE aliasing the receiver, not which of the two return-type spellings names it.
#[test]
fn discarded_static_returning_method_call_keeps_the_receiver_alive() {
    let program = parse_fragment(
        br#"class EvalFluentStaticDiscard {
    public array $i = [];
    public function __destruct() { echo "drop:" . count($this->i) . ":"; }
    public function add(string $v): static { $this->i[] = $v; return $this; }
}
$o = new EvalFluentStaticDiscard();
$o->add("a");
$o->add("b");
echo "count:" . count($o->i) . ":";
unset($o);
echo "after";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "count:2:drop:2:after");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert!(values.over_releases().is_empty());
}

/// Verifies `$this->add($v);` discarded INSIDE another method of the same class keeps the
/// receiver alive exactly like a top-level discarded call.
///
/// `php -n` 8.5.6 prints `count:2:drop:2:after`. `bump()`'s own receiver is `$this` (also a
/// `LoadVar`, also not "temporary"), so the defect is reachable from inside a method body too, not
/// only from a top-level statement.
#[test]
fn discarded_this_call_inside_another_method_keeps_the_receiver_alive() {
    let program = parse_fragment(
        br#"class EvalFluentNestedThisDiscard {
    public array $i = [];
    public function __destruct() { echo "drop:" . count($this->i) . ":"; }
    public function add(string $v): self { $this->i[] = $v; return $this; }
    public function bump(string $v): void { $this->add($v); }
}
$o = new EvalFluentNestedThisDiscard();
$o->bump("x");
$o->bump("y");
echo "count:" . count($o->i) . ":";
unset($o);
echo "after";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "count:2:drop:2:after");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert!(values.over_releases().is_empty());
}

/// Verifies a discarded STATIC method call returning a freshly constructed `static` does not
/// regress: there is no receiver to alias, so the fresh temporary must still be destructed exactly
/// once, immediately, with nothing leaked.
///
/// `php -n` 8.5.6 prints `drop:after`: the fresh object from `new static()` has no other owner, so
/// discarding `C4::make();` destructs it right there, before the following echo.
#[test]
fn discarded_static_factory_call_still_destructs_its_fresh_result_once() {
    let program = parse_fragment(
        br#"class EvalFluentStaticFactoryDiscard {
    public function __destruct() { echo "drop:"; }
    public static function make(): static { return new static(); }
}
EvalFluentStaticFactoryDiscard::make();
echo "after";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "drop:after");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert!(values.over_releases().is_empty());
}
