//! Purpose:
//! Interpreter tests for the `serialize()` builtin: scalars, strings (including embedded NUL and
//! UTF-8 bytes), lists, nested/associative arrays, and -- FIRST, not last, per PLAN.md's own
//! warning that this is the literal shape of a Symfony cache-freshness payload -- an object with
//! public/protected/private declared properties, whose private slot must be NUL-mangled with the
//! DECLARING class name, not silently emitted as a plain public key.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_serialize`.
//!
//! Key details:
//! - Every expected byte string is `php -n` 8.5.6's own output on the same fragment, re-measured
//!   in this session (`scratchpad/ser_verify.php`, `scratchpad/ser_verify2.php`) rather than
//!   copied from an unverified spec.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse serialize() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute serialize() fragment");
    values.output.clone()
}

/// Verifies the property-visibility-mangling shape FIRST: a private property is NUL-mangled with
/// its DECLARING class name (`"\0Mg\0parameters"`), not emitted as a plain public key -- the
/// exact bug PLAN.md warns turns into a wrong cache-freshness decision with no crash.
///
/// `php -n` 8.5.6: `O:2:"Mg":1:{s:14:"\0Mg\0parameters";a:1:{i:0;s:7:"DEFAULT";}}`.
#[test]
fn serialize_mangles_a_private_property_with_its_declaring_class_first() {
    assert_eq!(
        out(br#"class Mg { private $parameters = ["DEFAULT"]; }
echo serialize(new Mg());"#),
        "O:2:\"Mg\":1:{s:14:\"\0Mg\0parameters\";a:1:{i:0;s:7:\"DEFAULT\";}}",
    );
}

/// Verifies all three visibilities on one class in declaration order: private mangled with its
/// class, protected mangled with `"\0*\0"`, public plain.
///
/// `php -n` 8.5.6: `O:1:"P":3:{s:4:"\0P\0a";s:1:"x";s:4:"\0*\0b";i:1;s:1:"c";N;}`.
#[test]
fn serialize_mangles_protected_with_a_star_and_leaves_public_plain() {
    assert_eq!(
        out(br#"class P { private string $a = 'x'; protected int $b = 1; public $c = null; }
echo serialize(new P());"#),
        "O:1:\"P\":3:{s:4:\"\0P\0a\";s:1:\"x\";s:4:\"\0*\0b\";i:1;s:1:\"c\";N;}",
    );
}

/// Verifies every scalar/null shape: int (positive/negative/zero), float (including the `1.0`
/// -> `d:1;` no-decimal shape and a repeating fraction at php's own `serialize_precision=-1`
/// width), bool, and null.
///
/// `php -n` 8.5.6: `i:42;i:-7;i:0;d:1.5;d:1;d:0.3333333333333333;b:1;b:0;N;`.
#[test]
fn serialize_covers_every_scalar_and_null() {
    assert_eq!(
        out(br#"echo serialize(42), serialize(-7), serialize(0),
serialize(1.5), serialize(1.0), serialize(1/3),
serialize(true), serialize(false), serialize(null);"#),
        "i:42;i:-7;i:0;d:1.5;d:1;d:0.3333333333333333;b:1;b:0;N;",
    );
}

/// Verifies strings carry an embedded NUL byte and UTF-8 bytes through UNCHANGED, with the byte
/// length (not character count) as the length prefix.
///
/// `php -n` 8.5.6: `s:3:"a" NUL "b";s:6:"héllo";` (héllo is 6 bytes: 'h','é'=2 bytes,'l','l','o').
#[test]
fn serialize_strings_preserve_embedded_nul_and_utf8_bytes() {
    assert_eq!(
        out(b"echo serialize(\"a\\0b\"), serialize(\"h\xc3\xa9llo\");"),
        "s:3:\"a\0b\";s:6:\"h\u{e9}llo\";",
    );
}

/// Verifies list arrays, associative arrays, and nesting three deep -- the shapes measured in the
/// real cold/warm 404 payloads.
///
/// `php -n` 8.5.6: `a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}a:2:{s:1:"a";i:1;s:1:"b";i:2;}
/// a:1:{s:1:"a";a:1:{s:1:"b";a:1:{s:1:"c";i:1;}}}`.
#[test]
fn serialize_covers_list_assoc_and_nested_arrays() {
    assert_eq!(
        out(br#"echo serialize([1,2,3]), serialize(["a"=>1,"b"=>2]),
serialize(["a"=>["b"=>["c"=>1]]]);"#),
        "a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}a:2:{s:1:\"a\";i:1;s:1:\"b\";i:2;}\
a:1:{s:1:\"a\";a:1:{s:1:\"b\";a:1:{s:1:\"c\";i:1;}}}",
    );
}

/// Verifies mixed int/string keys serialize in insertion order, not sorted or renumbered.
///
/// `php -n` 8.5.6: `a:2:{i:0;s:1:"a";s:1:"k";i:7;}`.
#[test]
fn serialize_mixed_keys_keep_insertion_order() {
    assert_eq!(
        out(br#"echo serialize([0=>"a","k"=>7]);"#),
        "a:2:{i:0;s:1:\"a\";s:1:\"k\";i:7;}",
    );
}

/// Verifies a dynamic (undeclared) property on a declared class serializes as a plain public key
/// after the declared one, in creation order.
///
/// `php -n` 8.5.6: `O:3:"Dyn":2:{s:3:"pub";i:1;s:5:"extra";s:7:"dynamic";}`.
#[test]
fn serialize_includes_a_dynamic_property_after_declared_ones() {
    assert_eq!(
        out(br#"class Dyn { public $pub = 1; }
$d = new Dyn();
$d->extra = "dynamic";
echo serialize($d);"#),
        "O:3:\"Dyn\":2:{s:3:\"pub\";i:1;s:5:\"extra\";s:7:\"dynamic\";}",
    );
}

/// Verifies `function_exists("serialize")` answers true now that the builtin is registered.
///
/// `php -n` 8.5.6: `bool(true)`.
#[test]
fn serialize_is_visible_to_function_exists() {
    let program =
        parse_fragment(br#"return function_exists("serialize");"#).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the catchable `ArgumentCountError` for the wrong arity, worded exactly as php words
/// it.
///
/// `php -n` 8.5.6: `serialize() expects exactly 1 argument, 0 given` then
/// `serialize() expects exactly 1 argument, 2 given`.
#[test]
fn serialize_rejects_wrong_arity_with_a_catchable_argument_count_error() {
    assert_eq!(
        out(br#"try {
    serialize();
} catch (\ArgumentCountError $e) {
    echo $e->getMessage(), ":";
}
try {
    serialize(1, 2);
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#),
        "serialize() expects exactly 1 argument, 0 given:\
serialize() expects exactly 1 argument, 2 given",
    );
}

/// An object that appears again is written as `r:<n>;` naming the 1-based index of its first
/// occurrence, where every value takes an index, an `r:` included, and array keys take none.
///
/// `php -n` 8.5.6: `a:2:{i:0;O:8:"stdClass":0:{}i:1;r:2;}`, a self-reference
/// `O:8:"stdClass":1:{s:4:"self";r:1;}`, and a later object after a back-edge
/// `a:4:{i:0;O:8:"stdClass":0:{}i:1;r:2;i:2;O:8:"stdClass":0:{}i:3;r:4;}`.
#[test]
fn serialize_writes_a_back_reference_for_an_object_seen_before() {
    assert_eq!(
        out(br#"$o = new stdClass; echo serialize([$o, $o]);"#),
        "a:2:{i:0;O:8:\"stdClass\":0:{}i:1;r:2;}",
    );
    assert_eq!(
        out(br#"$o = new stdClass; $o->self = $o; echo serialize($o);"#),
        "O:8:\"stdClass\":1:{s:4:\"self\";r:1;}",
    );
    assert_eq!(
        out(br#"$a = new stdClass; $b = new stdClass; echo serialize([$a, $a, $b, $b]);"#),
        "a:4:{i:0;O:8:\"stdClass\":0:{}i:1;r:2;i:2;O:8:\"stdClass\":0:{}i:3;r:4;}",
    );
}
