//! Purpose:
//! Verifies descriptor callback argument owners are balanced on normal and exceptional returns.
//!
//! Called from:
//! - The runtime GC codegen integration suite on every executable target.
//!
//! Key details:
//! - Callable parameters force the shared descriptor wrapper instead of a direct callback call.
//! - Predicates avoid allocating a partial mapped result, isolating the argument-array owner.

use crate::support::*;

/// Repeated callback throws release wrapper arguments and leave later descriptor calls usable.
#[test]
fn test_core_descriptor_callback_throw_releases_argument_arrays() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function checkOwnedCallback(callable $callback): bool {
    return array_all([1, 2, 3], $callback);
}
function throwOwnedCallback(int $value): bool {
    if ($value === 2) { throw new RuntimeException("stop"); }
    return true;
}
function acceptOwnedCallback(int $value): bool { return $value > 0; }
$callback = throwOwnedCallback(...);
$caught = 0;
for ($i = 0; $i < 40; $i++) {
    try { checkOwnedCallback($callback); }
    catch (RuntimeException $error) {
        if ($error->getMessage() === "stop") { $caught++; }
        unset($error);
    }
}
echo $caught, ":", checkOwnedCallback(acceptOwnedCallback(...)) ? "ok" : "bad";
unset($callback);
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "40:ok", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested descriptor boundaries restore the outer catch and preserve a callback's exception chain.
#[test]
fn test_core_nested_descriptor_callback_throws_preserve_exception_state() {
    let source = r#"<?php
function checkNestedCallback(callable $callback): bool {
    return array_all([1, 2], $callback);
}
function innerNestedCallback(int $value): bool {
    throw new RuntimeException("inner");
}
function outerNestedCallback(int $value): bool {
    try { checkNestedCallback(innerNestedCallback(...)); }
    catch (RuntimeException $error) {
        if ($value === 2) { throw new LogicException("outer", 0, $error); }
    }
    return true;
}
function successfulNestedCallback(int $value): bool { return true; }
try { checkNestedCallback(outerNestedCallback(...)); }
catch (LogicException $error) {
    echo $error->getMessage(), ":", $error->getPrevious()->getMessage(), "|";
    echo checkNestedCallback(successfulNestedCallback(...)) ? "ok|" : "bad|";
    try { throw $error; }
    catch (LogicException $same) { echo $same === $error ? "same" : "lost"; }
}
"#;
    assert_eq!(compile_and_run(source), "outer:inner|ok|same");
}

/// Releasing successful callback arguments does not consume a returned string's independent owner.
#[test]
fn test_core_descriptor_callback_return_survives_argument_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function returnOwnedCallback(string $value): string { return $value; }
function mapOwnedCallback(callable $callback): void {
    $source = [str_repeat("x", 24), str_repeat("y", 24)];
    $result = array_map($callback, $source);
    unset($source);
    echo strlen($result[0]), ":", strlen($result[1]), "|";
    unset($result);
}
mapOwnedCallback(returnOwnedCallback(...));
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "24:24|", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
