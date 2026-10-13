//! Purpose:
//! Tests runtime PHP string parameter validation on direct and variadic boundaries.
//!
//! Called from:
//! - The codegen integration suite's string_parameter_boundaries filter.
//!
//! Key details:
//! - Invalid payloads throw TypeError rather than inherit explicit-cast behavior.
//! - The call site's strict_types controls validation, not the callee declaration.
//! - Normal and exceptional preparation retain unchanged heap-clean assertions.

use crate::support::*;

/// Checks one string-binding or return fixture's visible effects and balanced heap ownership.
fn assert_boundary(body: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(body);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Enforces invocation-site strictness on fixed, named and variadic dynamic string bindings.
#[test]
fn string_parameter_boundaries_dynamic_strict_fixed_named_and_variadic_reject_without_conversion() {
    assert_boundary(r#"<?php
declare(strict_types=1);
class Text { public function __toString(): string { echo 'unexpected'; return 'text'; } }
function text(string $value): string { return $value; }
function texts(string ...$values): string { return $values[0]; }
$callback = rtrim('text ');
$variadic = rtrim('texts ');
try { call_user_func_array($callback, [42]); }
catch (TypeError $error) { echo 'scalar|'; unset($error); }
try { call_user_func($callback, new Text()); }
catch (TypeError $error) { echo 'object|'; unset($error); }
try { call_user_func_array($callback, ['value' => true]); }
catch (TypeError $error) { echo 'named|'; unset($error); }
try { call_user_func_array($variadic, ['valid', 42]); }
catch (TypeError $error) { echo 'variadic|'; unset($error); }
echo call_user_func_array($callback, ['value' => 'ok']);
unset($callback); unset($variadic);
"#, "scalar|object|named|variadic|ok");
}

/// Rejects strict scalar coercions while preserving PHP's int-to-float widening.
#[test]
fn string_parameter_boundaries_dynamic_strict_scalar_contracts_allow_only_float_widening() {
    assert_boundary(r#"<?php
declare(strict_types=1);
function integer(int $value): int { return $value; }
function boolean(bool $value): bool { return $value; }
function floating(float $value): float { return $value; }
$integer = rtrim('integer ');
$boolean = rtrim('boolean ');
$floating = rtrim('floating ');
try { call_user_func_array($integer, ['42']); }
catch (TypeError $error) { echo 'string|'; unset($error); }
try { call_user_func_array($integer, [42.5]); }
catch (TypeError $error) { echo 'float|'; unset($error); }
try { call_user_func_array($boolean, [1]); }
catch (TypeError $error) { echo 'bool|'; unset($error); }
$value = call_user_func_array($floating, [42]);
echo gettype($value), '|', $value;
unset($value); unset($integer); unset($boolean); unset($floating);
"#, "string|float|bool|double|42");
}

/// Carries strict invocation policy through an otherwise unknown callable parameter.
#[test]
fn string_parameter_boundaries_strict_unknown_closure_uses_invocation_policy() {
    assert_boundary(r#"<?php
declare(strict_types=1);
function invoke(callable $callback, mixed $value): mixed { return $callback($value); }
$callback = function(string $value): string { return $value; };
try { invoke($callback, 42); }
catch (TypeError $error) { echo 'rejected|'; unset($error); }
echo invoke($callback, 'valid');
unset($callback);
"#, "rejected|valid");
}

/// Checks strict nullable/union defaults and invalid named variadic entries.
#[test]
fn string_parameter_boundaries_dynamic_strict_nullable_union_defaults_and_named_tail() {
    assert_boundary(r#"<?php
declare(strict_types=1);
function optional(?string $value = null): mixed { return $value; }
function choice(int|string $value): mixed { return $value; }
function texts(string ...$values): int { return count($values); }
$optional = rtrim('optional ');
$choice = rtrim('choice ');
$texts = rtrim('texts ');
echo call_user_func_array($optional, []) === null ? 'default|' : 'bad|';
echo call_user_func_array($optional, [null]) === null ? 'null|' : 'bad|';
echo call_user_func_array($optional, ['text']), '|';
try { call_user_func_array($optional, [42]); }
catch (TypeError $error) { echo 'nullable|'; unset($error); }
echo call_user_func_array($choice, [42]), '|', call_user_func_array($choice, ['42']), '|';
try { call_user_func_array($choice, [true]); }
catch (TypeError $error) { echo 'union|'; unset($error); }
try { call_user_func_array($texts, ['first' => 'valid', 'second' => 42]); }
catch (TypeError $error) { echo 'tail'; unset($error); }
unset($optional); unset($choice); unset($texts);
"#, "default|null|text|nullable|42|42|union|tail");
}

/// Transfers exactly one boxed null owner for omitted nullable and Mixed defaults.
#[test]
fn string_parameter_boundaries_omitted_nullable_and_mixed_null_defaults_transfer_one_owner() {
    assert_boundary(r#"<?php
declare(strict_types=1);
function optional(?string $value = null): mixed { return $value; }
function untyped(mixed $value = null): mixed { return $value; }
$optional = rtrim('optional ');
$untyped = rtrim('untyped ');
echo call_user_func_array($optional, []) === null ? 'nullable|' : 'bad|';
echo call_user_func_array($untyped, []) === null ? 'mixed' : 'bad';
unset($optional); unset($untyped);
"#, "nullable|mixed");
}

/// Retires an omitted default owner when a later named parameter fails binding.
#[test]
fn string_parameter_boundaries_omitted_null_default_releases_on_later_named_binding_failure() {
    assert_boundary(r#"<?php
declare(strict_types=1);
function optional(?string $first = null, string $second = 'valid'): mixed { return $first; }
$callback = rtrim('optional ');
try { call_user_func_array($callback, ['second' => 42]); }
catch (TypeError $error) { echo 'rejected'; unset($error); }
unset($callback);
"#, "rejected");
}

/// Keeps one managed cell owner for an omitted nullable reference default.
#[test]
fn string_parameter_boundaries_omitted_ref_null_default_has_one_cell_owner() {
    assert_boundary(r#"<?php
declare(strict_types=1);
function optional(?string &$value = null): void {
    echo $value === null ? 'null|' : 'bad|';
    $value = rtrim('changed ');
    echo $value;
}
$callback = rtrim('optional ');
call_user_func_array($callback, []);
unset($callback);
"#, "null|changed");
}

/// Preserves PHP internal weak callback binding for array_map in strict source.
#[test]
fn string_parameter_boundaries_internal_array_map_remains_weak_in_strict_source() {
    assert_boundary(r#"<?php
declare(strict_types=1);
function boxed(mixed $value): mixed { return $value; }
function text(string $value): string { return $value; }
$values = array_map('text', [boxed(42), boxed(true)]);
echo $values[0], '|', $values[1];
unset($values);
"#, "42|1");
}

/// Requires all input-element side effects before the first array_map callback.
#[test]
fn string_parameter_boundaries_array_map_evaluates_complete_input_before_callbacks() {
    assert_boundary(r#"<?php
function source(int $value): mixed { echo 'source', $value, '|'; return $value; }
function text(string $value): string { echo 'callback', $value, '|'; return $value; }
$values = array_map('text', [source(1), source(2)]);
echo $values[0], $values[1];
unset($values);
"#, "source1|source2|callback1|callback2|12");
}

/// Runs finally after implicit return conversion fails but before the outer catch.
#[test]
fn string_parameter_boundaries_return_failure_runs_finally_before_outer_catch() {
    assert_boundary(r#"<?php
class Text { public function __toString(): string { echo 'convert|'; throw new RuntimeException('cause'); } }
function return_text(mixed $value): string {
    try { return $value; }
    finally { echo 'finally|'; }
}
try { return_text(new Text()); }
catch (TypeError $error) {
    $previous = $error->getPrevious();
    echo $previous->getMessage();
    unset($previous); unset($error);
}
"#, "convert|finally|cause");
}

/// Keeps the original Stringable exception as previous on a method return TypeError.
#[test]
fn string_parameter_boundaries_method_return_failure_keeps_previous() {
    assert_boundary(r#"<?php
class Text { public function __toString(): string { throw new RuntimeException('cause'); } }
class Factory { public function text(mixed $value): string { return $value; } }
$factory = new Factory();
try { $factory->text(new Text()); }
catch (TypeError $error) {
    $previous = $error->getPrevious();
    echo $previous->getMessage();
    unset($previous); unset($error);
}
unset($factory);
"#, "cause");
}

/// Keeps the original Stringable exception as previous on a closure return TypeError.
#[test]
fn string_parameter_boundaries_closure_return_failure_keeps_previous() {
    assert_boundary(r#"<?php
class Text { public function __toString(): string { throw new RuntimeException('cause'); } }
$callback = function(mixed $value): string { return $value; };
try { $callback(new Text()); }
catch (TypeError $error) {
    $previous = $error->getPrevious();
    echo $previous->getMessage();
    unset($previous); unset($error);
}
unset($callback);
"#, "cause");
}

/// Converts runtime false to the empty string on weak parameter and return boundaries.
#[test]
fn string_parameter_boundaries_runtime_false_uses_empty_string() {
    assert_boundary(r#"<?php
function consume(string $value): string { return $value; }
function return_text(mixed $value): string { return $value; }
$flag = $argc > 1;
echo '[', consume($flag), ']|[', return_text($flag), ']';
"#, "[]|[]");
}

/// Rejects strict non-string returns without calling Stringable or creating a previous cause.
#[test]
fn string_parameter_boundaries_strict_return_rejects_without_stringable_invocation() {
    assert_boundary(r#"<?php
declare(strict_types=1);
class Text { public function __toString(): string { echo 'unexpected'; return 'text'; } }
function return_text(mixed $value): string { return $value; }
try { return_text(42); }
catch (TypeError $error) { echo $error->getMessage(), '|'; unset($error); }
try { return_text(new Text()); }
catch (TypeError $error) { echo $error->getPrevious() === null ? 'object|' : 'previous|'; unset($error); }
echo return_text('valid');
"#, "return_text(): Return value must be of type string, int returned|object|valid");
}

/// Transfers one previous-exception lease independently of its wrapping error.
#[test]
fn string_parameter_boundaries_previous_getter_transfers_exactly_one_object_lease() {
    assert_boundary(r#"<?php
$cause = new RuntimeException('cause');
$error = new TypeError('wrapper', 0, $cause);
unset($cause);
$previous = $error->getPrevious();
echo get_class($previous), '|', $previous->getMessage();
unset($previous); unset($error);
"#, "RuntimeException|cause");
}

/// Wraps an implicit Stringable return failure while preserving the original class/message/code.
#[test]
fn string_parameter_boundaries_implicit_return_wraps_original_exception() {
    assert_boundary(r#"<?php
function boxed(mixed $value): mixed { return $value; }
class Text { public function __toString(): string { throw new RuntimeException('original', 17); } }
function return_text(mixed $value): string { return $value; }
$value = boxed(new Text());
try { return_text($value); }
catch (TypeError $error) {
    $previous = $error->getPrevious();
    echo get_class($error), '|', get_class($previous), '|', $previous->getMessage(), '|', $previous->getCode();
    unset($previous); unset($error);
}
unset($value);
"#, "TypeError|RuntimeException|original|17");
}

/// Rejects an array return with no fabricated previous exception.
#[test]
fn string_parameter_boundaries_implicit_return_rejects_array_without_previous() {
    assert_boundary(r#"<?php
function return_text(mixed $value): string { return $value; }
try { return_text(['a' => 1]); }
catch (TypeError $error) { echo $error->getMessage(), '|', $error->getPrevious() === null ? 'none' : 'previous'; unset($error); }
"#, "return_text(): Return value must be of type string, array returned|none");
}

/// Converts a valid implicit Stringable return exactly once and accepts weak scalars.
#[test]
fn string_parameter_boundaries_valid_implicit_return_converts_once() {
    assert_boundary(r#"<?php
class Text { public function __toString(): string { echo 'once|'; return 'text'; } }
function return_text(mixed $value): string { return $value; }
echo return_text(new Text()), '|', return_text(42);
"#, "once|text|42");
}

/// Leaves an explicit string cast's original exception unwrapped.
#[test]
fn string_parameter_boundaries_explicit_return_cast_preserves_original_exception() {
    assert_boundary(r#"<?php
class Text { public function __toString(): string { throw new RuntimeException('original'); } }
function return_text(mixed $value): string { return (string)$value; }
try { return_text(new Text()); }
catch (RuntimeException $error) { echo get_class($error); unset($error); }
"#, "RuntimeException");
}

/// Preserves earlier conversion effects before later invalid indexed or named parameter binding.
#[test]
fn string_parameter_boundaries_dynamic_descriptor_converts_before_later_invalid_parameter() {
    assert_boundary(r#"<?php
class Text { public function __toString(): string { echo 'converted|'; return 'value'; } }
function consume(string $first, string $second): int { return 0; }
$callback = rtrim('consume ');
try { call_user_func_array($callback, [new Text(), []]); }
catch (TypeError $error) { echo 'indexed|'; unset($error); }
try { call_user_func_array($callback, ['second' => [], 'first' => new Text()]); }
catch (TypeError $error) { echo 'named'; unset($error); }
unset($callback);
"#, "converted|indexed|converted|named");
}

/// Preserves earlier conversion effects before a later invalid variadic element.
#[test]
fn string_parameter_boundaries_dynamic_variadic_converts_before_later_invalid_element() {
    assert_boundary(r#"<?php
class Text { public function __toString(): string { echo 'converted|'; return 'value'; } }
function consume(string ...$values): int { return 0; }
$callback = rtrim('consume ');
try { call_user_func_array($callback, [new Text(), []]); }
catch (TypeError $error) { echo 'indexed|'; unset($error); }
try { call_user_func_array($callback, ['first' => new Text(), 'second' => []]); }
catch (TypeError $error) { echo 'named'; unset($error); }
unset($callback);
"#, "converted|indexed|converted|named");
}

/// Rejects non-string-compatible boxed payloads on fixed and variadic boundaries.
#[test]
fn string_parameter_boundaries_reject_array_null_resource_and_non_stringable_object() {
    assert_boundary(r#"<?php
function boxed(mixed $value): mixed { return $value; }
function consume(string $value): int { return 0; }
function variadic(string ...$values): int { return 0; }
$array = boxed(['key' => 1]);
$null = boxed(null);
$resource = boxed(fopen('/dev/null', 'r'));
$object = boxed(new stdClass());
try { consume($array); } catch (TypeError $error) { echo 'array|'; unset($error); }
try { variadic($array); } catch (TypeError $error) { echo 'variadic|'; unset($error); }
try { consume($null); } catch (TypeError $error) { echo 'null|'; unset($error); }
try { consume($resource); } catch (TypeError $error) { echo 'resource|'; unset($error); }
try { consume($object); } catch (TypeError $error) { echo 'object'; unset($error); }
fclose($resource);
unset($array); unset($null); unset($resource); unset($object);
"#, "array|variadic|null|resource|object");
}

/// Accepts weak scalar bindings and invokes Stringable conversion exactly once.
#[test]
fn string_parameter_boundaries_weak_scalars_and_stringable_convert_once() {
    assert_boundary(r#"<?php
function boxed(mixed $value): mixed { return $value; }
class Text { public function __toString(): string { echo 'once|'; return 'text'; } }
function consume(string $value): string { return $value; }
echo consume(boxed(42)), '|', consume(boxed(true)), '|', consume(boxed(new Text()));
"#, "42|1|once|text");
}

/// Rejects scalar/Stringable arguments under strict call-site policy before conversion.
#[test]
fn string_parameter_boundaries_strict_call_site_rejects_scalar_and_stringable_without_conversion() {
    assert_boundary(r#"<?php
declare(strict_types=1);
function boxed(mixed $value): mixed { return $value; }
class Text { public function __toString(): string { echo 'unexpected'; return 'text'; } }
function consume(string $value): string { return $value; }
try { consume(boxed(42)); } catch (TypeError $error) { echo 'scalar|'; unset($error); }
try { consume(boxed(new Text())); } catch (TypeError $error) { echo 'object|'; unset($error); }
echo consume(boxed('valid'));
"#, "scalar|object|valid");
}
