//! Purpose:
//! Verifies method and descriptor callback argument owners are balanced across call boundaries.
//!
//! Called from:
//! - The runtime GC codegen integration suite on every executable target.
//!
//! Key details:
//! - Callable parameters force the shared descriptor wrapper instead of a direct callback call.
//! - Predicates avoid allocating a partial mapped result, isolating the argument-array owner.

use crate::support::*;

/// Lexical calls retire boxed scalar/default arguments without disturbing by-reference writeback.
#[test]
fn test_core_parent_method_call_releases_boxed_arguments_and_preserves_writeback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ParentArgumentOwners {
    public function forward(mixed $value, ?Exception $previous = null): mixed { return $value; }
    public function update(mixed $value, mixed &$output): mixed { $output = $value; return $value; }
}
class ChildArgumentOwners extends ParentArgumentOwners {
    public function exercise(): void {
        $value = parent::forward(41);
        $output = 0;
        $updated = parent::update(42, $output);
        if ($value !== 41 || $updated !== 42 || $output !== 42) { echo "bad"; }
    }
}
$owner = new ChildArgumentOwners();
for ($i = 0; $i < 40; $i++) { $owner->exercise(); }
unset($owner);
echo "ok";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ok", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Mixed, nullable-interface and receiver-bound callable dispatch retire their argument boxes.
#[test]
fn test_core_indirect_method_calls_release_boxed_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface ArgumentOwnerInterface { public function forward(mixed $value): mixed; }
class IndirectArgumentOwner implements ArgumentOwnerInterface {
    public function forward(mixed $value): mixed { return $value; }
}
function callMixedArgumentOwner(mixed $owner): mixed { return $owner->forward(41); }
function callNullableArgumentOwner(?ArgumentOwnerInterface $owner): mixed { return $owner->forward(42); }
$owner = new IndirectArgumentOwner();
for ($i = 0; $i < 40; $i++) {
    $mixed = callMixedArgumentOwner($owner);
    $nullable = callNullableArgumentOwner($owner);
    $callable = ($owner->forward(...))(43);
    if ($mixed !== 41 || $nullable !== 42 || $callable !== 43) { echo "bad"; }
    unset($mixed, $nullable, $callable);
}
unset($owner);
echo "ok";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ok", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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

/// Empty and allocated callback strings keep exactly one detached result owner until map cleanup.
#[test]
fn test_core_descriptor_callback_empty_and_heap_string_results_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function detachedCallbackString(string $value): string { return $value; }
function mappedCallbackStrings(callable $callback): void {
    $source = ["", str_repeat("x", 48)];
    $result = array_map($callback, $source);
    unset($source);
    echo strlen($result[0]), ":", strlen($result[1]);
    unset($result);
}
mappedCallbackStrings(detachedCallbackString(...));
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0:48", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Descriptor argument strings survive later scalar conversions and release defaults after each call.
#[test]
fn test_core_descriptor_string_argument_coercions_and_defaults_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function joinStringArguments(string $left = "left", string $right = "right"): string {
    return $left . ":" . $right;
}
function exerciseStringArguments(callable $callback): void {
    $numbers = [123, 456];
    $strings = ["", str_repeat("x", 24)];
    $defaults = [];
    $named = ["right" => "R", "left" => "L"];
    echo call_user_func_array($callback, $numbers), "|";
    echo strlen(call_user_func_array($callback, $strings)), "|";
    echo call_user_func_array($callback, $defaults), "|";
    echo call_user_func_array($callback, $named);
}
exerciseStringArguments(joinStringArguments(...));
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "123:456|25|left:right|L:R", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing descriptor callback releases its converted string lease without consuming the caller's array.
#[test]
fn test_core_descriptor_string_arguments_are_released_on_throw() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function throwStringArgument(string $value): bool {
    throw new RuntimeException("stop");
}
function exerciseThrowingStringArguments(callable $callback): void {
    $arguments = [str_repeat("x", 24)];
    $caught = 0;
    for ($i = 0; $i < 12; $i++) {
        try { call_user_func_array($callback, $arguments); }
        catch (RuntimeException $error) { $caught++; unset($error); }
    }
    echo $caught, ":", strlen($arguments[0]);
}
exerciseThrowingStringArguments(throwStringArgument(...));
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "12:24", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
