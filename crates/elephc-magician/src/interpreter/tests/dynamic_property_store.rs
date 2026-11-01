//! Purpose:
//! Interpreter tests pinning that a DYNAMIC property — one created by `$object->name = ...`
//! rather than declared — is visible to every reader, not just the one that happens to look in
//! the store it landed in.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::dynamic_property_store`.
//!
//! Key details:
//! - An eval-declared object keeps its properties in `ElephcEvalContext`, while every
//!   enumerator (`print_r`, `var_dump`, `json_encode`, `get_object_vars`, the reflection
//!   listings) walks the runtime object's own slots. A property written to only one of the two
//!   is silently missing from whichever half does not look there.
//! - The readers are pinned TOGETHER on purpose: the failure mode this file exists for is not
//!   any one of them being wrong, it is two of them disagreeing about the same object.
//! - Every expected string is `php -n` 8.5.6's output on the same fragment. PHP also emits
//!   `Deprecated: Creation of dynamic property C::$p is deprecated` for a class without
//!   `#[AllowDynamicProperties]`; that goes to the warning channel, not to stdout, so it does
//!   not appear in these expectations.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse dynamic property fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute dynamic property fragment");
    values.output.clone()
}

/// Verifies a plain runtime object and an interpreter-declared one answer alike.
///
/// `php -n` 8.5.6 prints `std=A;own=B;issetstd=y;issetown=y;varsstd=1;varsown=1`. The two
/// halves used to disagree in opposite directions: a `stdClass` property read back empty while
/// `isset()` and `get_object_vars()` saw it, and an interpreter-declared class read back
/// correctly while `isset()` and `get_object_vars()` did not see it at all.
#[test]
fn a_dynamic_property_answers_alike_on_both_object_kinds() {
    assert_eq!(
        out(
            br#"$o = new stdClass(); $o->p = "A"; class Own {} $u = new Own(); $u->q = "B";
echo "std=", $o->p, ";own=", $u->q, ";issetstd=", isset($o->p)?"y":"n", ";issetown=", isset($u->q)?"y":"n", ";varsstd=", count(get_object_vars($o)), ";varsown=", count(get_object_vars($u));"#
        ),
        "std=A;own=B;issetstd=y;issetown=y;varsstd=1;varsown=1",
    );
}

/// Verifies `isset`, `empty`, `unset` and `property_exists` agree about a dynamic property.
///
/// `php -n` 8.5.6 prints `Iemip`: set, not empty, a missing name is not set, and after
/// `unset()` the property is neither set nor reported as existing. That last letter is the one
/// that catches a half-finished removal, because `unset()` has to clear BOTH stores.
#[test]
fn isset_empty_and_unset_agree_about_a_dynamic_property() {
    assert_eq!(
        out(
            br#"class D4 { public $declared = "d"; } $o = new D4(); $o->dyn = "v";
echo isset($o->dyn) ? "I" : "i";
echo empty($o->dyn) ? "E" : "e";
echo isset($o->missing) ? "M" : "m";
unset($o->dyn);
echo isset($o->dyn) ? "I" : "i";
echo property_exists($o, "dyn") ? "P" : "p";"#
        ),
        "Iemip",
    );
}

/// Verifies `json_encode()` serialises a declared property AND a dynamic one.
///
/// `php -n` 8.5.6 prints `{"declared":"d","dyn2":"w"}`. It used to print `{}`: the encoder
/// walks the object's slots, and an interpreter-declared object had written neither its
/// declared default nor its dynamic property there.
#[test]
fn json_encode_serialises_declared_and_dynamic_properties() {
    assert_eq!(
        out(
            br#"class D5 { public $declared = "d"; } $o = new D5(); $o->dyn2 = "w"; echo json_encode($o);"#
        ),
        "{\"declared\":\"d\",\"dyn2\":\"w\"}",
    );
}

/// Verifies `print_r()` shows both properties with their values.
///
/// `php -n` 8.5.6 prints the object block below. It used to print the declared name with an
/// EMPTY value and omit the dynamic property entirely, which is the same split seen from the
/// other side: the name came from class metadata, the value from an unwritten slot.
#[test]
fn print_r_shows_declared_and_dynamic_property_values() {
    assert_eq!(
        out(
            br#"class D6 { public $declared = "d"; } $o = new D6(); $o->dyn2 = "w"; print_r($o);"#
        ),
        "D6 Object\n(\n    [declared] => d\n    [dyn2] => w\n)\n",
    );
}

/// Verifies a dynamic property survives a read through the reflection API too.
///
/// `php -n` 8.5.6 prints `D:value:H`. This is the same object seen through
/// `ReflectionObject`, which is what a container does, and it has to agree with the plain
/// reads above.
#[test]
fn reflection_sees_the_same_dynamic_property_as_plain_access() {
    assert_eq!(
        out(
            br#"class D8 { public $declared = "d"; } $o = new D8(); $o->dynamic = "value";
$ref = new ReflectionObject($o);
$p = $ref->getProperty("dynamic");
echo $p->isDynamic() ? "D" : "d"; echo ":";
echo $p->getValue($o); echo ":";
echo $ref->hasProperty("dynamic") ? "H" : "h";"#
        ),
        "D:value:H",
    );
}
