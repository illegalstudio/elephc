//! Purpose:
//! Interpreter tests for the `get_debug_type()` builtin: PHP-8 scalar spellings (distinct from
//! `gettype()`'s legacy ones), the object arm naming a class rather than `"object"`, the
//! anonymous-class name, closures, resources (open and closed), and the catchable
//! `ArgumentCountError` php raises for the wrong arity.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_get_debug_type`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's own output on the same fragment, re-measured in
//!   this session (`scratchpad/verify_gdt.php`) rather than copied from an unverified spec.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse get_debug_type() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute get_debug_type() fragment");
    values.output.clone()
}

/// Verifies every scalar and container spelling PHP 8 uses -- `"int"`/`"float"`/`"bool"`/
/// `"null"`, never `gettype()`'s `"integer"`/`"double"`/`"boolean"`/`"NULL"` -- and that a
/// numeric string still answers `"string"`.
///
/// `php -n` 8.5.6: `null:bool:bool:int:int:float:string:string:array:array`.
#[test]
fn get_debug_type_uses_php8_scalar_spellings_not_gettypes() {
    assert_eq!(
        out(br#"echo get_debug_type(null), ":";
echo get_debug_type(true), ":";
echo get_debug_type(false), ":";
echo get_debug_type(0), ":";
echo get_debug_type(-1), ":";
echo get_debug_type(1.0), ":";
echo get_debug_type(""), ":";
echo get_debug_type("123"), ":";
echo get_debug_type([]), ":";
echo get_debug_type([1 => [2]]);"#),
        "null:bool:bool:int:int:float:string:string:array:array",
    );
}

/// Verifies the object arm names the class -- not the literal `"object"` `gettype()` would say
/// -- for a user class, `stdClass`, and a thrown `Exception`.
///
/// `php -n` 8.5.6: `Foo:stdClass:Exception`.
#[test]
fn get_debug_type_names_the_class_for_an_object() {
    assert_eq!(
        out(br#"class Foo {}
echo get_debug_type(new Foo()), ":";
echo get_debug_type(new stdClass()), ":";
echo get_debug_type(new Exception("x"));"#),
        "Foo:stdClass:Exception",
    );
}

/// Verifies a `Closure` object -- both syntaxes -- answers `"Closure"`, matching
/// `sprintf`'s own `"Closure"` literal for the same runtime tag rather than falling through to
/// `gettype()`'s missing arm.
///
/// `php -n` 8.5.6: `Closure:Closure`.
#[test]
fn get_debug_type_names_a_closure() {
    assert_eq!(
        out(br#"echo get_debug_type(function () {}), ":";
echo get_debug_type(fn() => 1);"#),
        "Closure:Closure",
    );
}

/// Verifies an anonymous class's name starts with php's own `"class@anonymous"` prefix and
/// agrees with `get_class()` on the SAME object -- the interpreter's own anonymous-class naming
/// (`class@anonymous#evalN`, `crate::parser::state::next_anonymous_class_name`) differs from
/// php's `class@anonymous\0<file>:<line>$<id>` internal spelling, and unlike php's it never
/// carries a NUL byte to truncate, so this checks internal agreement with `get_class()` rather
/// than hard-coding php's own byte count (which would fail here for a reason that has nothing to
/// do with `get_debug_type()` itself).
///
/// `php -n` 8.5.6: `get_debug_type($a) !== get_class($a)` for an anonymous class (`bool(false)`,
/// because of the NUL truncation) -- elephc's names never diverge, so here they are EQUAL.
#[test]
fn get_debug_type_names_an_anonymous_class() {
    assert_eq!(
        out(br#"$a = new class {};
$debug = get_debug_type($a);
echo str_starts_with($debug, "class@anonymous") ? "prefix-ok" : "bad-prefix", ":";
echo $debug === get_class($a) ? "agrees-with-get_class" : "bad-mismatch";"#),
        "prefix-ok:agrees-with-get_class",
    );
}

/// Verifies resource naming: an open stream is `"resource (stream)"`, and a closed one is
/// `"resource (closed)"` -- a DIFFERENT spelling from `get_resource_type()`'s `"Unknown"` for the
/// same closed handle.
///
/// `php -n` 8.5.6: `resource (stream):resource (closed)`.
#[test]
fn get_debug_type_names_an_open_and_a_closed_resource() {
    let program = parse_fragment(
        br#"echo get_debug_type($handle), ":";
echo get_debug_type($closed);"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let open = values.alloc(FakeValue::Resource(5));
    scope.set("handle".to_string(), open, ScopeCellOwnership::Borrowed);
    // A host-closed resource carries a NEGATIVE payload sentinel (see
    // `execute_program_renames_a_closed_host_resource_to_unknown` in `builtins_file_streams.rs`).
    let closed = values.alloc(FakeValue::Resource(-6));
    scope.set("closed".to_string(), closed, ScopeCellOwnership::Borrowed);
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "resource (stream):resource (closed)");
}

/// Verifies named arguments, argument unpacking, `call_user_func`, and a variable-function call
/// all reach the same registry entry, and that an undefined-variable argument still answers
/// `"null"` (the warning belongs to the argument's OWN evaluation, not to `get_debug_type`).
///
/// `php -n` 8.5.6: `int:float:float:array:null`.
#[test]
fn get_debug_type_supports_named_spread_and_dynamic_call_forms() {
    assert_eq!(
        out(br#"echo get_debug_type(value: 5), ":";
echo get_debug_type(...[1.0]), ":";
echo call_user_func("get_debug_type", 1.0), ":";
$fn = "get_debug_type";
echo $fn([]), ":";
echo get_debug_type($undef);"#),
        "int:float:float:array:null",
    );
}

/// Verifies both arity failures are a catchable `ArgumentCountError`, worded exactly as php
/// words it, not an uncatchable fatal.
///
/// `php -n` 8.5.6: `get_debug_type() expects exactly 1 argument, 0 given` then
/// `get_debug_type() expects exactly 1 argument, 2 given`.
#[test]
fn get_debug_type_rejects_wrong_arity_with_a_catchable_argument_count_error() {
    assert_eq!(
        out(br#"try {
    get_debug_type();
} catch (\ArgumentCountError $e) {
    echo $e->getMessage(), ":";
}
try {
    get_debug_type(1, 2);
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#),
        "get_debug_type() expects exactly 1 argument, 0 given:\
get_debug_type() expects exactly 1 argument, 2 given",
    );
}

/// Verifies `function_exists("get_debug_type")` answers true now that the builtin is
/// registered.
///
/// `php -n` 8.5.6: `bool(true)`.
#[test]
fn get_debug_type_is_visible_to_function_exists() {
    let program = parse_fragment(br#"return function_exists("get_debug_type");"#)
        .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
