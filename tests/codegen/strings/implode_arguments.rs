//! Purpose:
//! Pins PHP's argument diagnostics for the one- and two-argument implode/join overloads.
//!
//! Called from:
//! - The codegen integration test harness through `strings`.
//!
//! Key details:
//! - Mixed parameters prevent constant folding from hiding the runtime type guards.
//! - Dynamic eval exercises both direct and evaluated-argument builtin dispatch.

use crate::support::*;

/// Weak separators accept native and inherited Stringables before reporting the missing array.
#[test]
fn test_implode_argument_errors_weak_overloads() {
    let out = compile_and_run(r#"<?php
class ImplodeArgumentObject {}
class ImplodeArgumentStringable { public function __toString(): string { echo "converted:"; return "glue"; } }
class ImplodeArgumentException extends RuntimeException {}
class ImplodeArgumentError extends Error {}
class ImplodeArgumentExceptionOverride extends Exception {
    public function __toString(): string { echo "throwable-converted:"; return "glue"; }
}
function one(mixed $value): string { return implode($value); }
function aliasOne(mixed $value): string { return join($value); }
function two(mixed $value): string { return implode(",", $value); }
foreach ([new ImplodeArgumentObject(), fn() => 1] as $value) {
    try { one($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { aliasOne($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
foreach ([1, 1.5, true, false, "s", null] as $value) {
    try { one($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { aliasOne(new ImplodeArgumentStringable()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
foreach ([new Exception(), new Error(), new RuntimeException(), new TypeError(), new ImplodeArgumentException(), new ImplodeArgumentError()] as $value) {
    try { aliasOne($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { implode(separator: $value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { call_user_func("implode", $value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { aliasOne(new ImplodeArgumentExceptionOverride()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo one(["a", "b"]), ":", two(["c", "d"]);
"#);
    let overload = "implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\n";
    let alias_overload = "join(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\n";
    let mut expected = format!("{overload}join(): Argument #1 ($separator) must be of type array|string, ImplodeArgumentObject given\n{overload}join(): Argument #1 ($separator) must be of type array|string, Closure given\n{}converted:{alias_overload}", overload.repeat(6));
    expected.push_str(&format!("{alias_overload}{overload}{overload}").repeat(6));
    expected.push_str(&format!("throwable-converted:{alias_overload}"));
    expected.push_str("ab:c,d");
    assert_eq!(out, expected);
}

/// Strict callers reject throwable Stringables without running their conversion hooks.
#[test]
fn test_implode_argument_errors_strict_overloads() {
    let out = compile_and_run(r#"<?php
declare(strict_types=1);
class StrictImplodeArgumentException extends RuntimeException {}
class StrictImplodeArgumentError extends Error {}
class StrictImplodeArgumentOverride extends Exception {
    public function __toString(): string { echo "unexpected-conversion:"; return "glue"; }
}
function one(mixed $value): string { return implode($value); }
function aliasOne(mixed $value): string { return join($value); }
foreach ([1, 1.5, true, false, null] as $value) {
    try { one($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { aliasOne($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { one("separator"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
foreach ([new Exception(), new Error(), new RuntimeException(), new TypeError(), new StrictImplodeArgumentException(), new StrictImplodeArgumentError(), new StrictImplodeArgumentOverride()] as $value) {
    try { aliasOne($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { implode(separator: $value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
echo one(["ok"]);
"#);
    let mut expected = String::new();
    for kind in ["int", "float", "true", "false", "null"] {
        expected.push_str("implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\n");
        expected.push_str(&format!("join(): Argument #1 ($separator) must be of type array|string, {kind} given\n"));
    }
    expected.push_str("implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\n");
    for kind in ["Exception", "Error", "RuntimeException", "TypeError", "StrictImplodeArgumentException", "StrictImplodeArgumentError", "StrictImplodeArgumentOverride"] {
        expected.push_str(&format!("join(): Argument #1 ($separator) must be of type array|string, {kind} given\nimplode(): Argument #1 ($separator) must be of type array|string, {kind} given\n"));
    }
    expected.push_str("ok");
    assert_eq!(out, expected);
}

/// Resources, callable descriptors, and native object handles retain their actual PHP kind.
#[test]
fn test_implode_argument_errors_boxed_resources_and_callables() {
    let out = compile_and_run(r#"<?php
function one(mixed $value): string { return implode($value); }
function two(mixed $value): string { return implode(",", $value); }
$stream = fopen(__FILE__, "r");
$context = hash_init("sha256");
foreach ([$stream, fn() => 1, $context] as $value) {
    try { one($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { two($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
fclose($stream);
try { two($stream); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo "alive";
"#);
    let mut expected = String::new();
    for kind in ["resource", "Closure", "HashContext"] {
        expected.push_str(&format!("implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nimplode(): Argument #2 ($array) must be of type ?array, {kind} given\n"));
    }
    expected.push_str("implode(): Argument #2 ($array) must be of type ?array, resource given\nalive");
    assert_eq!(out, expected);
}

/// Native joins honor eval Stringable overrides, including compact Throwable subclasses.
#[test]
fn test_implode_argument_errors_eval_bundle() {
    let out = compile_and_run(r#"<?php
function bridgeJoin(mixed $value): string { return join($value); }
function bridgeJoinTwo(mixed $value): string { return join(",", $value); }
class ImplodeNativeStringableBase {
    public function __toString(): string { echo "native-converted:"; return "base"; }
}
$code = $argc > 0 ? '
class EvalImplodeArgumentObject {}
class EvalImplodeArgumentStringable { public function __toString(): string { echo "eval-converted:"; return "glue"; } }
class EvalImplodeArgumentOverride extends ImplodeNativeStringableBase {
    public function __toString(): string { echo "override-converted:"; return "override"; }
}
class EvalImplodeArgumentThrowingOverride extends ImplodeNativeStringableBase {
    public function __toString(): string { echo "override-throwing:"; throw new RuntimeException("override-throw"); }
}
class EvalImplodeArgumentExceptionOverride extends Exception {
    public function __toString(): string { echo "exception-override:"; return "exception"; }
}
class EvalImplodeArgumentErrorOverride extends Error {
    public function __toString(): string { echo "error-override:"; throw new RuntimeException("error-override-throw"); }
}
foreach ([1, true, false, "s", new EvalImplodeArgumentObject(), null, function() { return 1; }] as $value) {
    try { implode(",", $value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { implode(new EvalImplodeArgumentObject()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { call_user_func("implode", ",", 42); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { call_user_func_array("implode", ["separator" => ",", "array" => false]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo implode(["a", "b"]), ":", implode("-", ["c", "d"]), ":alive\n";
return [new EvalImplodeArgumentObject(), new EvalImplodeArgumentStringable(), new EvalImplodeArgumentOverride(), new EvalImplodeArgumentThrowingOverride(), new EvalImplodeArgumentExceptionOverride(), new EvalImplodeArgumentErrorOverride()];
' : '';
$objects = eval($code);
try { bridgeJoin($objects[0]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { bridgeJoinTwo($objects[0]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { bridgeJoin($objects[1]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { bridgeJoin($objects[2]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { bridgeJoin($objects[3]); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
try { bridgeJoin($objects[4]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { bridgeJoin($objects[5]); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
echo "bridged";
"#);
    let mut expected = String::new();
    for kind in ["int", "true", "false", "string", "EvalImplodeArgumentObject"] {
        expected.push_str(&format!("implode(): Argument #2 ($array) must be of type ?array, {kind} given\n"));
    }
    expected.push_str("implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nimplode(): Argument #2 ($array) must be of type ?array, Closure given\nimplode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nimplode(): Argument #2 ($array) must be of type ?array, int given\nimplode(): Argument #2 ($array) must be of type ?array, false given\nab:c-d:alive\njoin(): Argument #1 ($separator) must be of type array|string, EvalImplodeArgumentObject given\njoin(): Argument #2 ($array) must be of type ?array, EvalImplodeArgumentObject given\neval-converted:join(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\noverride-converted:join(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\noverride-throwing:override-throw\n");
    expected.push_str("exception-override:join(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nerror-override:error-override-throw\nbridged");
    assert_eq!(out, expected);
}

/// Named and unpacked calls retain ordinary separator binding instead of the positional fast path.
#[test]
fn test_implode_argument_errors_named_and_unpacked() {
    let out = compile_and_run(r#"<?php
class ImplodeNamedArgumentObject {}
function probe(mixed $value): void {
    try { implode(separator: $value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { implode(...[$value]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { call_user_func("implode", $value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
probe(new ImplodeNamedArgumentObject());
"#);
    assert_eq!(out, "implode(): Argument #1 ($separator) must be of type array|string, ImplodeNamedArgumentObject given\n".repeat(3));
}
