//! Purpose:
//! Interpreter tests pinning `declare(strict_types=1)` -- the directive itself and the coercion
//! rules it changes.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::strict_types`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - `declare` was in the reserved-word list with no statement behind it, so the whole directive
//!   parsed as a CALL and died on the `=` inside it -- an assignment to a constant.
//! - Parsing it is not the point; what it CHANGES is. The same fragment is run twice, once with
//!   the directive and once without, and the two must differ. A test that only checked the
//!   strict half would pass against an interpreter that always threw.
//! - Strict mode turns every scalar coercion off with ONE exception php keeps: an int where a
//!   float is declared still widens.

use super::super::*;
use super::support::*;
use crate::errors::EvalParseError;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse strict types fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute strict types fragment");
    values.output.clone()
}

/// Verifies the same calls coerce without the directive and throw with it.
///
/// `php -n` 8.5.6 prints `6;6;5;7;end` without `declare(strict_types=1)` and
/// `TypeError;6;5;RetTypeError;end` with it. Both halves are asserted because the difference IS
/// the feature: `takesInt("5")` coerces or throws, `takesFloat(5)` widens in BOTH, and a return
/// type is checked the same way an argument is.
#[test]
fn the_directive_turns_scalar_coercion_off() {
    let body = br#"function takesInt(int $n): int { return $n + 1; }
try { echo takesInt("5"), ";"; } catch (\TypeError $e) { echo "TypeError;"; }
echo takesInt(5), ";";
function takesFloat(float $f): float { return $f; }
echo takesFloat(5), ";";
function retInt($v): int { return $v; }
try { echo retInt("7"), ";"; } catch (\TypeError $e) { echo "RetTypeError;"; }
echo "end";"#;
    let lenient = [&b""[..], body].concat();
    let strict = [&b"declare(strict_types=1);\n"[..], body].concat();
    assert_eq!(out(&lenient), "6;6;5;7;end");
    assert_eq!(out(&strict), "TypeError;6;5;RetTypeError;end");
}

/// Verifies php's message for an argument rejected under strict types.
///
/// `php -n` 8.5.6 says
/// `takesInt(): Argument #1 ($n) must be of type int, string given, called in FILE on line N`.
/// A fragment has no call-site file, so the suffix php appends there is absent and the message
/// is the prefix alone -- which is why the test asserts the whole string rather than a prefix.
#[test]
fn a_rejected_argument_names_the_callable_the_position_and_both_types() {
    assert_eq!(
        out(
            br#"declare(strict_types=1);
function takesInt(int $n): int { return $n + 1; }
try { takesInt("5"); } catch (\TypeError $e) { echo $e->getMessage(); }"#
        ),
        "takesInt(): Argument #1 ($n) must be of type int, string given",
    );
}

/// Verifies php's message for a return value rejected under strict types.
///
/// `php -n` 8.5.6 says `retInt(): Return value must be of type int, string returned`, with no
/// call site in it at all.
#[test]
fn a_rejected_return_value_names_the_callable_and_both_types() {
    assert_eq!(
        out(
            br#"declare(strict_types=1);
function retInt($v): int { return $v; }
try { retInt("7"); } catch (\TypeError $e) { echo $e->getMessage(); }"#
        ),
        "retInt(): Return value must be of type int, string returned",
    );
}

/// Verifies a METHOD argument is named `Class::method()` the way php names it.
///
/// `php -n` 8.5.6 says `K::m(): Argument #1 ($n) must be of type int, string given, called in
/// FILE on line N`.
#[test]
fn a_rejected_method_argument_names_the_class_and_the_method() {
    assert_eq!(
        out(
            br#"declare(strict_types=1);
class K { public function m(int $n): void {} }
try { (new K())->m("5"); } catch (\TypeError $e) { echo $e->getMessage(); }"#
        ),
        "K::m(): Argument #1 ($n) must be of type int, string given",
    );
}

/// Verifies a type mismatch is CATCHABLE even without the directive.
///
/// `php -n` 8.5.6 prints `caught;still running`: a non-coercible argument is a TypeError, not a
/// fatal, in lenient mode too. Three expectations in this suite said the opposite and were
/// corrected against php alongside this work.
#[test]
fn a_non_coercible_argument_is_catchable_without_the_directive() {
    assert_eq!(
        out(
            br#"class Box { public function read(int $id) { return $id; } }
$b = new Box();
try { $b->read("not numeric"); } catch (\TypeError $e) { echo "caught;"; }
echo "still running";"#
        ),
        "caught;still running",
    );
}

/// Verifies the ONE `declare` shape php refuses: `strict_types` in block mode.
///
/// This test used to assert that `declare(ticks=1)` was refused, on the reasoning that accepting
/// a directive and doing nothing would be a silent divergence. Measuring it settled the point the
/// other way: `php -n` 8.5.6 accepts `ticks`, and it does not refuse an unknown directive either
/// -- `declare(foo=1);` is a warning and the script runs on. Refusing was the larger divergence,
/// because a file php parses did not parse at all. What php DOES refuse is a block body on
/// `strict_types`: `Fatal error: strict_types declaration must not use block mode`. The rest of
/// the surface is pinned in `interpreter/tests/declare_directives.rs`.
#[test]
fn strict_types_is_refused_in_block_mode() {
    let error = parse_fragment(
        br#"declare(strict_types=1) { $a = 1; }"#,
    )
    .expect_err("strict_types must not use block mode");
    assert_eq!(error.error(), EvalParseError::UnsupportedConstruct);
}
