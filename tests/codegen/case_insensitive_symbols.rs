//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of case-insensitive symbols, including case insensitive keywords user functions and builtins, case insensitive class interface trait and method lookup, and case sensitive variables properties string keys and user constants.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

#[test]
fn test_case_insensitive_keywords_user_functions_and_builtins() {
    let out = compile_and_run(
        r#"<?php
FUNCTION Render(string $value): string {
    RETURN STRTOUPPER($value);
}

IF (TRUE) {
    ECHO render("ok");
}
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn test_case_insensitive_class_interface_trait_and_method_lookup() {
    let out = compile_and_run(
        r#"<?php
INTERFACE Named {
    PUBLIC FUNCTION Label(): string;
}

TRAIT Prefixer {
    PUBLIC FUNCTION Prefix(): string {
        RETURN "P";
    }
}

CLASS Greeter IMPLEMENTS named {
    USE prefixer;

    PUBLIC FUNCTION Label(): string {
        RETURN $this->PREFIX() . ":ok";
    }

    PUBLIC STATIC FUNCTION Make(): Greeter {
        RETURN NEW GREETER();
    }
}

$g = greeter::MAKE();
ECHO $g->label();
ECHO $g instanceof GREETER ? ":class" : ":no-class";
ECHO $g instanceof named ? ":iface" : ":no-iface";
"#,
    );
    assert_eq!(out, "P:ok:class:iface");
}

#[test]
fn test_case_sensitive_variables_properties_string_keys_and_user_constants() {
    let out = compile_and_run(
        r#"<?php
const AppValue = "C";

$Name = "upper";
$name = "lower";

class Box {
    public string $Code = "A";
    public string $code = "B";
}

$box = new Box();
$items = ["Key" => "value", "key" => "lower"];

echo $Name . "/" . $name . "/";
echo $box->Code . $box->code . "/";
echo $items["Key"] . ":" . $items["key"] . "/";
echo AppValue;
"#,
    );
    assert_eq!(out, "upper/lower/AB/value:lower/C");
}

#[test]
fn test_case_insensitive_function_string_callbacks() {
    let out = compile_and_run(
        r#"<?php
function FormatName(string $name): string {
    return strtoupper($name);
}

echo FUNCTION_EXISTS("formatname") ? "Y:" : "N:";
echo IS_CALLABLE("FORMATNAME") ? "C:" : "N:";
echo CALL_USER_FUNC("formatname", "ada") . ":";
echo CALL_USER_FUNC_ARRAY("FORMATNAME", ["lovelace"]);
"#,
    );
    assert_eq!(out, "Y:C:ADA:LOVELACE");
}

#[test]
fn test_case_insensitive_enum_static_method_lookup() {
    let out = compile_and_run(
        r#"<?php
enum Color: int {
    case Red = 1;
}

$picked = color::TRYFROM(1);
echo $picked === Color::Red ? "red" : "other";
"#,
    );
    assert_eq!(out, "red");
}

#[test]
fn test_case_insensitive_constructor_name_supports_promotion_and_readonly_writes() {
    let out = compile_and_run(
        r#"<?php
class User {
    public readonly string $name;

    public function __CONSTRUCT(public int $id, string $name) {
        $this->name = $name;
    }
}

$user = new User(7, "Ada");
echo $user->id . ":" . $user->name;
"#,
    );
    assert_eq!(out, "7:Ada");
}

#[test]
fn test_case_insensitive_constructor_override_skips_signature_compatibility() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function __construct(int $id) {}
}

class Child extends Base {
    public function __CONSTRUCT() {}
}

$child = new Child();
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_case_insensitive_builtin_type_names_do_not_resolve_as_classes() {
    let out = compile_and_run(
        r#"<?php
namespace App;

function apply(CALLABLE $fn, ARRAY $items): MIXED {
    return $fn($items[0]);
}

function inc($value) {
    return $value + 1;
}

echo apply(inc(...), [41]);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_class_constant_preserves_written_receiver_case() {
    let out = compile_and_run(
        r#"<?php
class FooBar {}
echo foobar::class;
"#,
    );
    assert_eq!(out, "foobar");
}
