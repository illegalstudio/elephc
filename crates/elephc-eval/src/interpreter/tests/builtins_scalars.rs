//! Purpose:
//! Interpreter tests for scalar type, resource, cast, and class-introspection builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases check runtime tag inspection and mutating scalar conversions.

use super::super::*;
use super::support::*;

/// Verifies eval type-predicate builtins inspect boxed runtime tags directly and by callable.
#[test]
fn execute_program_dispatches_type_predicate_builtins() {
    let program = parse_fragment(
            br#"echo is_int(1); echo is_integer(1); echo is_long(1);
echo is_float(1.5); echo is_double(1.5); echo is_real(1.5);
echo is_string("x"); echo is_bool(false); echo is_null(null);
echo is_array([1]); echo is_array(["a" => 1]);
echo is_iterable([1]); echo is_iterable(["a" => 1]);
echo is_iterable(1) ? "bad" : "T";
echo is_array(1) ? "bad" : "ok";
echo is_numeric(42); echo is_numeric(3.14); echo is_numeric("42");
echo is_numeric("-5"); echo is_numeric("3.14");
echo is_numeric("abc") ? "bad" : "N";
echo is_numeric(true) ? "bad" : "B";
echo is_resource(1) ? "bad" : "R";
echo is_object($object) ? "O" : "bad";
echo is_object([1]) ? "bad" : "o";
echo is_nan(fdiv(0, 0)) ? "N" : "bad";
echo is_infinite(fdiv(1, 0)) ? "I" : "bad";
echo is_infinite(fdiv(-1, 0)) ? "i" : "bad";
echo is_finite(42) ? "F" : "bad";
echo is_finite(fdiv(1, 0)) ? "bad" : "f";
echo ":"; echo call_user_func("is_string", "x");
echo call_user_func_array("is_array", [[1]]);
echo call_user_func("is_numeric", "12");
echo call_user_func("is_iterable", [1]);
echo call_user_func_array("is_iterable", ["value" => 1]) ? "bad" : "t";
echo call_user_func("is_object", $object) ? "O" : "bad";
echo call_user_func_array("is_object", ["value" => 1]) ? "bad" : "o";
echo function_exists("is_numeric"); echo function_exists("is_object"); echo function_exists("is_resource");
echo function_exists("is_double"); echo function_exists("is_nan"); echo function_exists("is_finite");
echo function_exists("is_iterable");
return function_exists("is_infinite");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("object".to_string(), object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1111111111111Tok11111NBROoNIiFf:1111tOo1111111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `is_resource()` recognizes resource-tagged runtime cells from scope.
#[test]
fn execute_program_dispatches_is_resource_true() {
    let program = parse_fragment(
        br#"echo is_resource($handle) ? "R" : "bad";
echo ":" . gettype($handle);
return call_user_func("is_resource", $handle);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let handle = values.alloc(FakeValue::Resource(6));
    scope.set("handle".to_string(), handle, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "R:resource");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval resource introspection builtins expose stream type and one-based id.
#[test]
fn execute_program_dispatches_resource_introspection_builtins() {
    let program = parse_fragment(
        br#"echo get_resource_type($handle);
echo ":" . get_resource_id($handle);
echo ":" . call_user_func("get_resource_type", $handle);
echo ":" . call_user_func_array("get_resource_id", ["resource" => $handle]);
echo ":" . function_exists("get_resource_type");
return function_exists("get_resource_id");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let handle = values.alloc(FakeValue::Resource(6));
    scope.set("handle".to_string(), handle, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "stream:7:stream:7:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval cast builtins return boxed scalar cells directly and by callable.
#[test]
fn execute_program_dispatches_cast_builtins() {
    let program = parse_fragment(
        br#"echo intval("42"); echo ":";
echo floatval("3.5"); echo ":";
echo strval(12); echo ":";
echo boolval("0") ? "bad" : "false";
echo ":"; echo call_user_func("strval", 7);
return call_user_func_array("intval", ["9"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "42:3.5:12:false:7");
    assert_eq!(values.get(result), FakeValue::Int(9));
}
/// Verifies eval `settype()` mutates direct variables and warns for callable by-value dispatch.
#[test]
fn execute_program_dispatches_settype_builtin() {
    let program = parse_fragment(
        br#"$x = 42;
echo settype($x, "string") ? gettype($x) . ":" . $x : "bad";
echo ":";
$y = "0";
echo settype(type: "bool", var: $y) ? gettype($y) . ":" . ($y ? "true" : "false") : "bad";
echo ":";
echo settype($missing, "integer") ? gettype($missing) . ":" . $missing : "bad";
echo ":";
$z = 3.8;
echo call_user_func("settype", $z, "integer") ? gettype($z) . ":" . $z : "bad";
echo ":";
return function_exists("settype");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "string:42:boolean:false:integer:0:double:3.8:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(
        values.warnings,
        ["settype(): Argument #1 ($var) must be passed by reference, value given"]
    );
}
/// Verifies eval `gettype()` maps runtime tags to PHP type names directly and by callable.
#[test]
fn execute_program_dispatches_gettype_builtin() {
    let program = parse_fragment(
        br#"echo gettype(1); echo ":";
echo gettype(1.5); echo ":";
echo gettype("x"); echo ":";
echo gettype(false); echo ":";
echo gettype(null); echo ":";
echo gettype([1]); echo ":";
echo gettype(["a" => 1]); echo ":";
echo call_user_func("gettype", true); echo ":";
echo call_user_func_array("gettype", [null]);
return function_exists("gettype");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "integer:double:string:boolean:NULL:array:array:boolean:NULL"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `get_class()` reads object class names directly and by callable.
#[test]
fn execute_program_dispatches_get_class_builtin() {
    let program = parse_fragment(
        br#"echo get_class($object); echo ":";
echo call_user_func("get_class", $object); echo ":";
return call_user_func_array("get_class", ["object" => $object]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("object".to_string(), object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "stdClass:stdClass:");
    assert_eq!(
        values.get(result),
        FakeValue::String("stdClass".to_string())
    );
}
/// Verifies eval `get_parent_class()` reads object and class-string parents by callable.
#[test]
fn execute_program_dispatches_get_parent_class_builtin() {
    let program = parse_fragment(
        br#"echo get_parent_class($object); echo ":";
echo get_parent_class("ChildClass"); echo ":";
echo call_user_func("get_parent_class", $object); echo ":";
return call_user_func_array("get_parent_class", ["object_or_class" => "ChildClass"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("object".to_string(), object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "ParentClass:ParentClass:ParentClass:");
    assert_eq!(
        values.get(result),
        FakeValue::String("ParentClass".to_string())
    );
}
