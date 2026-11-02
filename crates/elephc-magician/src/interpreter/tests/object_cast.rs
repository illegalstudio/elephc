//! Purpose:
//! Interpreter tests pinning PHP's `(object)` cast, which the eval parser refused outright until
//! this file existed: `(object)` was not one of the cast keywords it recognised, so `(object) $x`
//! was read as the parenthesised constant `object` followed by a stray token.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::object_cast`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The cast is NOT one rule. An object casts to ITSELF (`===` to the operand, not a copy), an
//!   array casts to a `stdClass` keyed by its own keys INTEGER KEYS INCLUDED, null casts to an
//!   empty `stdClass`, and every other scalar casts to a `stdClass` with the single property
//!   `scalar`. A test that only covered the associative-array case would pass against four
//!   wrong answers.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse object cast fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute object cast fragment");
    values.output.clone()
}

/// Verifies an associative array casts to a `stdClass` carrying its keys as properties.
///
/// `php -n` 8.5.6 prints `stdClass;a=1;b=two;n=2`.
#[test]
fn an_associative_array_casts_to_a_std_class_keyed_by_its_keys() {
    assert_eq!(
        out(
            br#"$assoc = ["a" => 1, "b" => "two"];
$o = (object) $assoc;
echo get_class($o), ";a=", $o->a, ";b=", $o->b, ";n=", count(get_object_vars($o));"#
        ),
        "stdClass;a=1;b=two;n=2",
    );
}

/// Verifies an INDEXED array keeps its integer keys as property names.
///
/// `php -n` 8.5.6 prints `{"0":10,"1":20};10;20`. The names are the decimal spellings of the
/// keys, reachable only through `->{"0"}`, and `json_encode` shows them as object keys rather
/// than as a JSON array -- which is what separates this from a cast that quietly dropped the
/// keys and appended values in order.
#[test]
fn an_indexed_array_casts_to_numeric_property_names() {
    assert_eq!(
        out(
            br#"$list = [10, 20];
$l = (object) $list;
echo json_encode($l), ";", $l->{"0"}, ";", $l->{"1"};"#
        ),
        "{\"0\":10,\"1\":20};10;20",
    );
}

/// Verifies scalars land under `scalar` and null produces an empty object.
///
/// `php -n` 8.5.6 prints `hello;42;0;stdClass`. `scalar` is PHP's own choice of name, not an
/// invented one, and `(object) null` is an object with NO properties rather than one holding a
/// null `scalar`.
#[test]
fn a_scalar_casts_under_the_scalar_property_and_null_casts_to_an_empty_object() {
    assert_eq!(
        out(
            br#"$s = (object) "hello";
$i = (object) 42;
$n = (object) null;
echo $s->scalar, ";", $i->scalar, ";", count(get_object_vars($n)), ";", get_class($n);"#
        ),
        "hello;42;0;stdClass",
    );
}

/// Verifies casting an object yields THAT object, not a copy of it.
///
/// `php -n` 8.5.6 prints `same;9;P`. Writing through the cast result changes the original, and
/// the class survives the cast -- a cast that built a fresh `stdClass` from the properties would
/// print `copy;5;stdClass` and pass any test that only compared property values.
#[test]
fn casting_an_object_returns_the_same_instance() {
    assert_eq!(
        out(
            br#"class P { public $x = 5; }
$p = new P();
$same = (object) $p;
$same->x = 9;
echo ($same === $p) ? "same" : "copy", ";", $p->x, ";", get_class($same);"#
        ),
        "same;9;P",
    );
}

/// Verifies the array-literal spelling Symfony uses parses and stays writable afterwards.
///
/// `php -n` 8.5.6 prints `3;0;7;{"regexMark":3,"regex":[],"mark":7}`. This is the shape in
/// `CompiledUrlMatcherDumper::compileStaticRoutes()` -- `(object) ['regexMark' => 0, ...]` --
/// where the cast operand is an array LITERAL rather than a variable, which is the spelling the
/// parser used to reject at the `=>` inside it.
#[test]
fn an_array_literal_operand_casts_and_stays_writable() {
    assert_eq!(
        out(
            br#"$state = (object) ["regexMark" => 0, "regex" => [], "mark" => 7];
$state->regexMark = 3;
echo $state->regexMark, ";", count($state->regex), ";", $state->mark, ";", json_encode($state);"#
        ),
        "3;0;7;{\"regexMark\":3,\"regex\":[],\"mark\":7}",
    );
}
