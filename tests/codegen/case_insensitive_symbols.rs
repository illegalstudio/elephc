//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of case-insensitive symbols, including case insensitive keywords user functions and builtins, case insensitive class interface trait and method lookup, and case sensitive variables properties string keys and user constants.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Tests that PHP keywords, user-defined functions, and built-in functions are all
/// resolved case-insensitively. Uppercase keyword tokens (FUNCTION, RETURN, IF, ECHO)
/// and mixed-case callables (STRTOUPPER) must map to their canonical lowercase forms.
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

/// Tests that interface names, trait names, class names, method names, and static
/// method calls are resolved case-insensitively while preserving the PHP rule that
/// `instanceof` uses case-insensitive class-name resolution.
/// Mixed-case declarations (INTERFACE Named, TRAIT Prefixer, CLASS Greeter) must
/// still be found via their canonical lowercase names.
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

/// Tests that variables, object properties, array string keys, and user-defined constants
/// are all case-sensitive. `$Name` and `$name` must be distinct variables, `$box->Code`
/// and `$box->code` must refer to different properties, `["Key"]` and `["key"]` must be
/// distinct array entries, and const `AppValue` must not alias `appvalue`.
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

/// Tests that `function_exists()`, `is_callable()`, `call_user_func()`, and
/// `call_user_func_array()` all resolve function names case-insensitively.
/// Uppercase names like "FORMATNAME" must still locate the canonical lowercase function.
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

/// Tests that enum case static methods (TRYFROM) are resolved case-insensitively.
/// Uppercase `color::TRYFROM` must locate the canonical `Color::tryFrom`.
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

/// Tests that constructor name `__CONSTRUCT` is resolved case-insensitively and
/// supports PHP 8 constructor property promotion. Readonly property written via the
/// promoted parameter must retain its value after construction.
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

/// Tests that an uppercase `__CONSTRUCT` in a child class overrides a base-class
/// constructor without requiring signature compatibility (following PHP's relaxed
/// override rules for constructors). Regression test for previously rejected cases
/// where signature mismatches caused spurious compile errors on case-insensitive constructors.
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

/// Tests that builtin type names (CALLABLE, ARRAY, MIXED) used as type hints in a
/// namespace do not resolve as class names. Functions using these builtin types must
/// still be callable and must not be treated as missing class definitions.
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

/// Tests that `::class` constant resolution preserves the written receiver case.
/// Uppercase `foobar::class` must return the string "foobar" (lowercased by PHP semantics),
/// not "Foobar". Regression test for receiver-case preservation in class constant lookup.
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
