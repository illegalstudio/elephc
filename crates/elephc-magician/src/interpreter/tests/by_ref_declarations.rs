//! Purpose:
//! Interpreter tests pinning PHP's return-BY-REFERENCE declaration marker -- `function &f()`,
//! `public function &__get()`, `function &() use (...)` and `fn &() => ...` -- which the eval
//! parser refused outright at the `&`.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::by_ref_declarations`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - SCOPE: this is the DECLARATION, not the binding. Called by value -- which is how almost all
//!   PHP code calls them -- a by-reference function is indistinguishable from a by-value one, and
//!   that is what these tests assert. `$r = &f();` is a different construct and is still refused;
//!   the refusal is pinned in the parser tests so a later fix has to change it deliberately.
//! - The marker is RECORDED rather than skipped, because PHP's rule is one-directional: an
//!   implementation may ADD `&` where its interface does not ask for it, but dropping one the
//!   interface requires is a fatal.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse by-ref declaration fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute by-ref declaration fragment");
    values.output.clone()
}

/// Runs one fragment and returns the status it failed with.
fn err(fragment: &[u8]) -> EvalStatus {
    let program = parse_fragment(fragment).expect("parse by-ref declaration fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect_err("fragment should not execute")
}

/// Verifies every declaration spelling parses and answers as php does when called by value.
///
/// `php -n` 8.5.6 prints `5;1;1;7;2;1`: a named `function &counter()`, a `public function &get()`
/// beside a plain one on the same class, a `function &() { }` closure, an `fn &() =>` arrow, and
/// a `public function &__get()` magic reader.
#[test]
fn every_by_ref_declaration_spelling_parses_and_answers_by_value() {
    assert_eq!(
        out(
            br#"function &counter() { static $n = 5; return $n; }
echo counter(), ";";
class Box {
    private $v = 1;
    public array $items = ["a"];
    public function &get() { return $this->v; }
    public function read() { return $this->v; }
    public function &__get($name): mixed { return $this->items; }
}
$b = new Box();
echo $b->read(), ";", $b->get(), ";";
$make = function &() { static $s = 7; return $s; };
echo $make(), ";";
$arr = [1, 2];
$pick = fn &() => $arr;
echo count($pick()), ";";
echo count($b->anything);"#
        ),
        "5;1;1;7;2;1",
    );
}

/// Verifies an implementation may ADD `&` where its interface does not require it.
///
/// `php -n` 8.5.6 prints `3`. This direction is legal, so a rule written as "the two must match"
/// would refuse code php accepts.
#[test]
fn an_implementation_may_add_a_by_ref_return_the_interface_does_not_require() {
    assert_eq!(
        out(
            br#"interface Plain { public function get(); }
class Adds implements Plain { private $v = 3; public function &get() { return $this->v; } }
echo (new Adds())->get();"#
        ),
        "3",
    );
}

/// Verifies DROPPING a `&` the interface requires is refused, as php refuses it.
///
/// `php -n` 8.5.6 fails the whole script with
/// `Fatal error: Declaration of Bad::get() must be compatible with & Ref::get()`, and it is a
/// fatal rather than a throwable, so elephc refuses the class declaration. Recording the marker
/// instead of skipping it is what makes this checkable at all: a parser that consumed the `&`
/// and threw it away would accept a by-value method standing in for a by-reference contract.
#[test]
fn dropping_a_by_ref_return_the_interface_requires_is_refused() {
    assert_eq!(
        err(
            br#"interface Ref { public function &get(); }
class Bad implements Ref { public function get() { return 1; } }
echo "declared";"#
        ),
        EvalStatus::RuntimeFatal,
    );
}
