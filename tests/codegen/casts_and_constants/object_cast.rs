//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of PHP's `(object)` cast:
//! array-to-stdClass key projection, the `scalar` property for non-array scalars, the empty
//! object for `null`, and the identity an object source keeps.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Expected output is verbatim reference PHP for the same program (issue #836).

use super::*;

/// The issue #836 reproduction: an array literal casts to a stdClass keyed by the array's
/// keys, and the property reads back. The parser used to reject the cast outright with
/// `Expected ']'`.
#[test]
fn test_object_cast_of_array_literal_exposes_keys_as_properties() {
    let out = compile_and_run(
        r#"<?php
$value = (object) ['name' => 'Laravel'];
echo $value->name;
"#,
    );
    assert_eq!(out, "Laravel");
}

/// Every key of a multi-entry array becomes a property, and the values keep their PHP types.
#[test]
fn test_object_cast_of_array_keeps_every_key_and_value_type() {
    let out = compile_and_run(
        r#"<?php
$value = (object) ['name' => 'Laravel', 'version' => 12, 'ratio' => 1.5, 'on' => true];
echo $value->name, "|", $value->version, "|", $value->ratio, "|";
var_dump($value->on);
"#,
    );
    assert_eq!(out, "Laravel|12|1.5|bool(true)\n");
}

/// An integer-keyed array produces the numeric-STRING property names PHP produces, which are
/// reachable through the `{'0'}` form.
#[test]
fn test_object_cast_of_integer_keyed_array_produces_numeric_string_names() {
    let out = compile_and_run(
        r#"<?php
$value = (object) ['x', 'y'];
echo $value->{'0'}, $value->{'1'};
"#,
    );
    assert_eq!(out, "xy");
}

/// A cast of an array held in a variable goes through the same conversion as a literal.
#[test]
fn test_object_cast_of_array_variable() {
    let out = compile_and_run(
        r#"<?php
$source = ['k' => 'v'];
$value = (object) $source;
echo $value->k;
"#,
    );
    assert_eq!(out, "v");
}

/// A declared `array` uses boxed packed-or-associative storage, but both shapes still take the
/// statically non-object helper and produce a concrete stdClass result.
#[test]
fn test_object_cast_of_declared_array_keeps_stdclass_result_type() {
    let out = compile_and_run(
        r#"<?php
function cast_declared_array(array $source): mixed {
    return ((object) $source)->name;
}

echo cast_declared_array(['name' => 'typed']);
"#,
    );
    assert_eq!(out, "typed");
}

/// Every non-array, non-null, non-object source lands on php-src's literal `scalar` property.
#[test]
fn test_object_cast_of_scalars_uses_the_scalar_property() {
    let out = compile_and_run(
        r#"<?php
echo ((object) 42)->scalar, "|", ((object) 'text')->scalar, "|";
var_dump(((object) true)->scalar);
var_dump(((object) 1.5)->scalar);
"#,
    );
    assert_eq!(out, "42|text|bool(true)\nfloat(1.5)\n");
}

/// `(object) null` is an EMPTY stdClass, not an object carrying a null `scalar` property.
#[test]
fn test_object_cast_of_null_is_an_empty_stdclass() {
    let out = compile_and_run(
        r#"<?php
$value = (object) null;
var_dump($value instanceof stdClass, get_object_vars($value));
"#,
    );
    assert_eq!(out, "bool(true)\narray(0) {\n}\n");
}

/// `(object)` is the IDENTITY on an object: the same instance comes back, so `===` holds and a
/// write through the cast result is visible through the source.
#[test]
fn test_object_cast_of_an_object_returns_the_same_instance() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; }
$box = new Box();
$cast = (object) $box;
var_dump($cast === $box);
$cast->n = 7;
echo $box->n;
"#,
    );
    assert_eq!(out, "bool(true)\n7");
}

/// A runtime-typed source reaches the dynamic helper, which must keep the identity arm for an
/// object payload and still convert every other payload.
#[test]
fn test_object_cast_of_a_mixed_source_handles_both_arms() {
    let out = compile_and_run(
        r#"<?php
class Tag { public string $s = 'tag'; }
function pick(int $i): mixed {
    if ($i === 0) { return new Tag(); }
    if ($i === 1) { return ['k' => 'arr']; }
    return 9;
}
$a = (object) pick(0);
$b = (object) pick(1);
$c = (object) pick(2);
echo $a->s, "|", $b->k, "|", $c->scalar;
"#,
    );
    assert_eq!(out, "tag|arr|9");
}

/// The cast result is a real stdClass to the class-introspection surface.
#[test]
fn test_object_cast_result_is_a_stdclass() {
    let out = compile_and_run(
        r#"<?php
$value = (object) ['name' => 'Laravel'];
var_dump($value instanceof stdClass);
echo get_class($value);
"#,
    );
    assert_eq!(out, "bool(true)\nstdClass");
}

/// `(array)` of an object cast round-trips the entries back to the original array.
#[test]
fn test_object_cast_round_trips_through_an_array_cast() {
    let out = compile_and_run(
        r#"<?php
$value = (object) ['name' => 'Laravel', 'version' => 12];
$back = (array) $value;
echo $back['name'], "|", $back['version'], "|", count($back);
"#,
    );
    assert_eq!(out, "Laravel|12|2");
}

/// The cast's source is evaluated EXACTLY ONCE: the lowering stores it in a synthetic local
/// before the helper call, so a source with a side effect cannot run twice.
#[test]
fn test_object_cast_evaluates_its_source_once() {
    let out = compile_and_run(
        r#"<?php
$calls = 0;
function source(): array {
    global $calls;
    $calls = $calls + 1;
    return ['k' => 'v'];
}
$value = (object) source();
echo $value->k, "|", $calls;
"#,
    );
    assert_eq!(out, "v|1");
}

/// The cast works inside a function body, where the prelude helper is reached through an
/// ordinary call rather than from top-level code.
#[test]
fn test_object_cast_inside_a_function() {
    let out = compile_and_run(
        r#"<?php
function wrap(array $values): stdClass {
    return (object) $values;
}
echo wrap(['name' => 'Laravel'])->name;
"#,
    );
    assert_eq!(out, "Laravel");
}

/// A `(object)` cast whose ONLY occurrence is inside a compile-time-autoloaded class file must
/// still get the prelude helpers it is lowered to.
///
/// The prelude is detected syntactically, and an autoloaded class file is not part of the AST
/// until `autoload::run`. Injecting before that pass — where every other prelude sits — left
/// this program with a call to an undeclared helper and failed code generation with
/// `unsupported EIR backend feature: language construct __elephc_cast_object`.
#[test]
fn test_object_cast_inside_an_autoloaded_class_gets_the_prelude() {
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Maker.php",
                "<?php\nnamespace App;\nclass Maker {\n    public function make(array $values): \\stdClass { return (object) $values; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$maker = new App\\Maker();\necho $maker->make([\"name\" => \"Laravel\"])->name;\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Laravel");
}

/// The cast spelling is case-insensitive, as every PHP cast is.
#[test]
fn test_object_cast_spelling_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
$value = (OBJECT) ['name' => 'Laravel'];
echo $value->name;
"#,
    );
    assert_eq!(out, "Laravel");
}
