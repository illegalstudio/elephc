//! Purpose:
//! End-to-end tests for `spl_object_id()` and `spl_object_hash()` on boxed and callable values.
//!
//! Called from:
//! - `cargo test --test codegen_tests_oop` through the SPL test module.
//!
//! Key details:
//! - A Closure keeps its own handle, an invokable object converted to a callable keeps the
//!   object's handle, and every non-object raises PHP's argument `TypeError`.
//! - Expected output is PHP 8.6's (with `spl_object_hash()`'s 8.6 deprecation silenced).

use crate::support::*;

/// Boxed objects, closures, first-class callables and invokable receivers keep PHP's identity,
/// and every temporary box is retired.
#[test]
fn test_spl_object_identity_of_boxed_and_callable_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class IdentityInvokable { public function __invoke(): int { return 1; } }
function identity_id(mixed $value): int { return spl_object_id($value); }
function identity_hash(mixed $value): string { return spl_object_hash($value); }
function identity_callable(callable $c): mixed { return $c; }
function identity_run(): void {
    $object = new ArrayObject();
    $closure = function () { return 1; };
    $arrow = fn() => 2;
    $first = strlen(...);
    $invokable = new IdentityInvokable();
    echo identity_id($object) === spl_object_id($object) ? "object\n" : "object-mismatch\n";
    echo identity_id($closure) === identity_id($closure) ? "closure-stable\n" : "closure-unstable\n";
    echo identity_id($closure) === spl_object_id($closure) ? "closure-static\n" : "closure-static-mismatch\n";
    echo identity_id($closure) !== identity_id($arrow) ? "distinct\n" : "same\n";
    echo identity_id($first) > 0 ? "first-class\n" : "no-id\n";
    echo identity_id(identity_callable($invokable)) === spl_object_id($invokable) ? "invokable\n" : "invokable-mismatch\n";
    echo identity_hash($closure) === spl_object_hash($closure) ? "hash\n" : "hash-mismatch\n";
    echo identity_hash($object) === spl_object_hash($object) ? "object-hash\n" : "object-hash-mismatch\n";
    unset($object, $closure, $arrow, $first, $invokable);
}
identity_run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "object\nclosure-stable\nclosure-static\ndistinct\nfirst-class\ninvokable\nhash\nobject-hash\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The boxed identity checks are independent of EIR optimization.
#[test]
fn test_spl_object_identity_of_boxed_and_callable_values_no_ir_opt() {
    without_ir_opt(test_spl_object_identity_of_boxed_and_callable_values);
}

/// Every non-object, including an implicit string or array callable, raises PHP's TypeError,
/// naming a boolean by its value.
#[test]
fn test_spl_object_id_rejects_boxed_non_objects() {
    let out = compile_and_run(r#"<?php
function errors_id(mixed $value): int { return spl_object_id($value); }
function errors_callable(callable $c): mixed { return $c; }
class ErrorsTarget { public function run(): void {} }
function errors_run(): void {
    $resource = fopen('php://memory', 'r');
    $values = [1, 1.5, "text", [1], null, true, false, $resource,
        errors_callable('strlen'), errors_callable([new ErrorsTarget(), 'run'])];
    foreach ($values as $value) {
        try { errors_id($value); echo "accepted\n"; }
        catch (TypeError $error) { echo $error->getMessage(), "\n"; }
    }
    fclose($resource);
}
errors_run();
"#);
    let prefix = "spl_object_id(): Argument #1 ($object) must be of type object, ";
    let expected = ["int", "float", "string", "array", "null", "true", "false", "resource", "string", "array"]
        .iter()
        .map(|given| format!("{prefix}{given} given\n"))
        .collect::<String>();
    assert_eq!(out, expected);
}
