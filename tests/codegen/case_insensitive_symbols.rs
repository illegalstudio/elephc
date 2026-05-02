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
echo CALL_USER_FUNC("formatname", "ada");
"#,
    );
    assert_eq!(out, "Y:ADA");
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
fn test_class_constant_preserves_written_receiver_case() {
    let out = compile_and_run(
        r#"<?php
class FooBar {}
echo foobar::class;
"#,
    );
    assert_eq!(out, "foobar");
}
