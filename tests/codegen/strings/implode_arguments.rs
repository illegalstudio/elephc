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

/// Weak separators coerce scalars, while object and callable operands name argument one.
#[test]
fn test_implode_argument_errors_weak_overloads() {
    let out = compile_and_run(r#"<?php
class ImplodeArgumentObject {}
class ImplodeArgumentStringable { public function __toString(): string { echo "converted:"; return "glue"; } }
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
echo one(["a", "b"]), ":", two(["c", "d"]);
"#);
    let overload = "implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\n";
    assert_eq!(out, format!("{overload}join(): Argument #1 ($separator) must be of type array|string, ImplodeArgumentObject given\n{overload}join(): Argument #1 ($separator) must be of type array|string, Closure given\n{}converted:join(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nab:c,d", overload.repeat(6)));
}

/// Strict callers retain their binding mode at the join's runtime guard.
#[test]
fn test_implode_argument_errors_strict_overloads() {
    let out = compile_and_run(r#"<?php
declare(strict_types=1);
function one(mixed $value): string { return implode($value); }
function aliasOne(mixed $value): string { return join($value); }
foreach ([1, 1.5, true, false, null] as $value) {
    try { one($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { aliasOne($value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { one("separator"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo one(["ok"]);
"#);
    let mut expected = String::new();
    for kind in ["int", "float", "true", "false", "null"] {
        expected.push_str("implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\n");
        expected.push_str(&format!("join(): Argument #1 ($separator) must be of type array|string, {kind} given\n"));
    }
    expected.push_str("implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nok");
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

/// Eval catches invalid array operands through both builtin dispatch paths and keeps running.
#[test]
fn test_implode_argument_errors_eval_bundle() {
    let out = compile_and_run(r#"<?php
function bridgeJoin(mixed $value): string { return join($value); }
function bridgeJoinTwo(mixed $value): string { return join(",", $value); }
$code = $argc > 0 ? '
class EvalImplodeArgumentObject {}
class EvalImplodeArgumentStringable { public function __toString(): string { echo "eval-converted:"; return "glue"; } }
foreach ([1, true, false, "s", new EvalImplodeArgumentObject(), null, function() { return 1; }] as $value) {
    try { implode(",", $value); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { implode(new EvalImplodeArgumentObject()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { call_user_func("implode", ",", 42); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { call_user_func_array("implode", ["separator" => ",", "array" => false]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo implode(["a", "b"]), ":", implode("-", ["c", "d"]), ":alive\n";
return [new EvalImplodeArgumentObject(), new EvalImplodeArgumentStringable()];
' : '';
$objects = eval($code);
try { bridgeJoin($objects[0]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { bridgeJoinTwo($objects[0]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { bridgeJoin($objects[1]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo "bridged";
"#);
    let mut expected = String::new();
    for kind in ["int", "true", "false", "string", "EvalImplodeArgumentObject"] {
        expected.push_str(&format!("implode(): Argument #2 ($array) must be of type ?array, {kind} given\n"));
    }
    expected.push_str("implode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nimplode(): Argument #2 ($array) must be of type ?array, Closure given\nimplode(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nimplode(): Argument #2 ($array) must be of type ?array, int given\nimplode(): Argument #2 ($array) must be of type ?array, false given\nab:c-d:alive\njoin(): Argument #1 ($separator) must be of type array|string, EvalImplodeArgumentObject given\njoin(): Argument #2 ($array) must be of type ?array, EvalImplodeArgumentObject given\neval-converted:join(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given\nbridged");
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
