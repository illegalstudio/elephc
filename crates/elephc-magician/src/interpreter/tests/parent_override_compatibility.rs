//! Purpose:
//! Interpreter tests for the whole parent-class override compatibility surface: what a subclass
//! may change about an inherited method and what php refuses.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::parent_override_compatibility`.
//!
//! Key details:
//! - Fourteen shapes were measured against `php -n` 8.5.6 and thirteen already agreed. Only one
//!   diverged: a subclass could DROP the parent's `&`. The rule existed for interfaces and the
//!   parent-class path silently had none, so it is now one shared helper called from both.
//! - The accepted half is pinned beside the refused half on purpose. A checker that refuses
//!   everything passes every refusal test ever written, and contravariant parameters, covariant
//!   returns and extra optional parameters are exactly what real subclasses do.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse override fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute override fragment");
    values.output.clone()
}

/// Runs one fragment and returns the status the declaration failed with.
fn err(fragment: &[u8]) -> EvalStatus {
    let program = parse_fragment(fragment).expect("parse override fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect_err("declaration should be refused")
}

/// Verifies a subclass may not replace a by-reference parent method with a by-value one.
///
/// `php -n` 8.5.6: `Fatal error: Declaration of C::f() must be compatible with & P::f()`. The `&`
/// sits in front of the PARENT signature in that message, which is how php says which side wanted
/// it. This was the one shape of the fourteen measured here that elephc accepted.
#[test]
fn a_subclass_may_not_drop_the_parents_by_reference_return() {
    assert_eq!(
        err(
            br#"class P { public function &f() { static $x = 1; return $x; } }
class C extends P { public function f() { return 1; } }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}

/// Verifies the rule is one-directional: ADDING `&` the parent does not ask for is allowed.
///
/// `php -n` 8.5.6 runs it and prints `ok`. Without this half the fix above could be "refuse any
/// difference in `&`", which php does not do.
#[test]
fn a_subclass_may_add_a_by_reference_return_the_parent_lacks() {
    assert_eq!(
        out(
            br#"class P2 { public function f() { return 1; } }
class C2 extends P2 { public function &f() { static $x = 1; return $x; } }
echo "ok";"#
        ),
        "ok",
    );
}

/// Verifies parameter types are CONTRAVARIANT: a subclass may widen, never narrow.
///
/// `php -n` 8.5.6 refuses the narrowing with `Declaration of C3::f(int $a) must be compatible
/// with P3::f(mixed $a)` and accepts the widening. Both directions are asserted because either
/// one alone is satisfied by a checker that is simply wrong in the other direction.
#[test]
fn parameter_types_may_widen_but_not_narrow() {
    assert_eq!(
        err(
            br#"class P3 { public function f(mixed $a) {} }
class C3 extends P3 { public function f(int $a) {} }"#
        ),
        EvalStatus::RuntimeFatal,
    );
    assert_eq!(
        out(
            br#"class P4 { public function f(int $a) {} }
class C4 extends P4 { public function f(mixed $a) {} }
echo "ok";"#
        ),
        "ok",
    );
}

/// Verifies return types are COVARIANT: a subclass may narrow, never widen.
///
/// `php -n` 8.5.6 refuses the widening with `Declaration of C5::f(): mixed must be compatible
/// with P5::f(): int` and accepts the narrowing. This is the opposite direction to the parameter
/// rule above, which is why the two are pinned separately rather than as one "types must match".
#[test]
fn return_types_may_narrow_but_not_widen() {
    assert_eq!(
        err(
            br#"class P5 { public function f(): int { return 1; } }
class C5 extends P5 { public function f(): mixed { return 1; } }"#
        ),
        EvalStatus::RuntimeFatal,
    );
    assert_eq!(
        out(
            br#"class P6 { public function f(): mixed { return 1; } }
class C6 extends P6 { public function f(): int { return 1; } }
echo "ok";"#
        ),
        "ok",
    );
}

/// Verifies a subclass may ADD a return type but never DROP one.
///
/// `php -n` 8.5.6 accepts `ret_add` and refuses `ret_drop` with `Declaration of C8::f() must be
/// compatible with P8::f(): int`. An absent return type is `mixed` for this purpose, so dropping
/// one is the widening case above wearing different clothes.
#[test]
fn a_return_type_may_be_added_but_not_removed() {
    assert_eq!(
        out(
            br#"class P7 { public function f() { return 1; } }
class C7 extends P7 { public function f(): int { return 1; } }
echo "ok";"#
        ),
        "ok",
    );
    assert_eq!(
        err(
            br#"class P8 { public function f(): int { return 1; } }
class C8 extends P8 { public function f() { return 1; } }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}

/// Verifies the arity rule: never fewer parameters, and extra ones only with defaults.
///
/// `php -n` 8.5.6 refuses `C9::f($a)` against `P9::f($a, $b)` and refuses `CB::f($a, $b)` against
/// `PB::f($a)`, but accepts `CA::f($a, $b = 1)`. The middle case is the one a naive "at least as
/// many parameters" rule gets wrong.
#[test]
fn arity_may_grow_only_with_defaults_and_may_never_shrink() {
    assert_eq!(
        err(
            br#"class P9 { public function f($a, $b) {} }
class C9 extends P9 { public function f($a) {} }"#
        ),
        EvalStatus::RuntimeFatal,
    );
    assert_eq!(
        out(
            br#"class PA { public function f($a) {} }
class CA extends PA { public function f($a, $b = 1) {} }
echo "ok";"#
        ),
        "ok",
    );
    assert_eq!(
        err(
            br#"class PB { public function f($a) {} }
class CB extends PB { public function f($a, $b) {} }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}

/// Verifies the three non-signature refusals php spells with their own messages.
///
/// `php -n` 8.5.6:
/// - `Access level to CC::f() must be public (as in class PC)`;
/// - `Cannot make non static method PD::f() static in class CD`;
/// - `Cannot override final method PE::f()`.
///
/// They are grouped because they share a shape -- each is a modifier, not a signature -- and each
/// is answered by its own branch of `validate_method_parent_override`.
#[test]
fn narrowing_visibility_changing_staticness_and_overriding_final_are_all_refused() {
    assert_eq!(
        err(
            br#"class PC { public function f() {} }
class CC extends PC { protected function f() {} }"#
        ),
        EvalStatus::RuntimeFatal,
    );
    assert_eq!(
        err(
            br#"class PD { public function f() {} }
class CD extends PD { public static function f() {} }"#
        ),
        EvalStatus::RuntimeFatal,
    );
    assert_eq!(
        err(
            br#"class PE { final public function f() {} }
class CE extends PE { public function f() {} }"#
        ),
        EvalStatus::RuntimeFatal,
    );
}
