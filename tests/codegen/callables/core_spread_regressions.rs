//! Purpose:
//! Exercises class introspection through shared named and unpacked call planning.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_introspection_spread_`.
//!
//! Key details:
//! - Targets are statically known while argument containers can be runtime values.

use crate::support::*;

/// Declared array returns preserve sparse numeric keys and named arguments across direct/CUF/FCC calls.
#[test]
fn test_core_introspection_spread_boxed_return_keys_and_callables() {
    let source = r#"<?php
class ReturnedSpread { public int $bar = 7; public function m(): void {} }
function returnedSpreadNames(string $name, bool $named): array {
    if ($named) { return ['class' => $name]; }
    return [42 => $name];
}
function returnedSpreadObject(): array { return ['object_or_class' => new ReturnedSpread()]; }
$vars = get_class_vars(...);
$methods = get_class_methods(...);
echo get_class_vars(...returnedSpreadNames(ReturnedSpread::class, false))['bar'], ':';
echo call_user_func('get_class_vars', ...returnedSpreadNames(ReturnedSpread::class, true))['bar'], ':';
echo $vars(...returnedSpreadNames(ReturnedSpread::class, true))['bar'], ':';
echo implode(',', $methods(...returnedSpreadObject()));
"#;
    assert_eq!(compile_and_run(source), "7:7:7:m");
    assert_eq!(compile_and_run_tagged(source), "7:7:7:m");
}

/// Runtime key validation rejects unknown, repeated and out-of-order arguments without losing side effects.
#[test]
fn test_core_introspection_spread_boxed_named_errors_and_order() {
    let out = compile_and_run(r#"<?php
class RejectedSpread { public int $x = 1; }
function returnedSpreadArgs(int $kind): array {
    if ($kind === 0) { return ['unknown' => RejectedSpread::class]; }
    if ($kind === 1) { return ['class' => RejectedSpread::class]; }
    if ($kind === 2) { return ['class' => RejectedSpread::class, 7 => RejectedSpread::class]; }
    return [];
}
function laterSpreadName(): string { echo 'side:'; return RejectedSpread::class; }
try { get_class_vars(...returnedSpreadArgs(0)); } catch (Error $e) { echo 'unknown|'; }
try { get_class_vars(...returnedSpreadArgs(1), ...returnedSpreadArgs(1)); }
catch (Error $e) { echo 'duplicate|'; }
try { get_class_vars(...returnedSpreadArgs(2)); } catch (Error $e) { echo 'order|'; }
try { get_class_vars(...returnedSpreadArgs(1), class: laterSpreadName()); }
catch (Error $e) { echo 'named|'; }
echo get_class_vars(...returnedSpreadArgs(3), class: laterSpreadName())['x'];
"#);
    assert_eq!(out, "unknown|duplicate|order|side:named|side:1");
}

/// The shared builtin normalizer also binds reordered boxed names outside class introspection.
#[test]
fn test_core_introspection_spread_boxed_shared_parameter_binding() {
    let out = compile_and_run(r#"<?php
function boxedReorderedArgs(): array { return ['times' => 3, 'string' => 'ok']; }
function boxedDefaultArgs(): array { return [27 => 'default']; }
echo str_repeat(...boxedReorderedArgs()), '|', strlen(...boxedDefaultArgs());
"#);
    assert_eq!(out, "okokok|7");
}

/// Temporary unpack sources and copied parameter cells are released after repeated builtin calls.
#[test]
fn test_core_introspection_spread_boxed_sources_release_after_binding() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function freshSpreadString(int $value): array { return ['string' => 'value' . $value]; }
$total = 0;
for ($i = 0; $i < 20; $i++) { $total += strlen(...freshSpreadString($i)); }
echo $total;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "130", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Keeps each callable surface small enough for the per-test CI deadline in both representations.
fn assert_mixed_spread_case(body: &str, expected: &str) {
    let source = format!(r#"<?php
class MixedSpread {{ public int $bar = 7; public function m(): void {{}} }}
function inspectMixedSpread(string $name): void {{
    $names = [$name];
    $objects = [new MixedSpread()];
    {body}
}}
inspectMixedSpread(MixedSpread::class);
"#);
    eprintln!("mixed spread fixture: boxed representation");
    assert_eq!(compile_and_run(&source), expected);
    eprintln!("mixed spread fixture: tagged representation");
    assert_eq!(compile_and_run_tagged(&source), expected);
}

/// Direct class introspection ignores empty literal unpacks beside runtime arrays.
#[test]
fn test_core_introspection_spread_mixed_static_dynamic_direct() {
    assert_mixed_spread_case(r#"
    echo get_class_vars(...[], ...$names)['bar'], ':';
    echo implode(',', get_class_methods(...[], ...$names)), ':';
    echo implode(',', get_class_methods(...$objects, ...[]));
"#, "7:m:m");
}

/// CUF specializes class introspection after removing empty literal unpacks.
#[test]
fn test_core_introspection_spread_mixed_static_dynamic_cuf() {
    assert_mixed_spread_case(r#"
    echo call_user_func('get_class_vars', ...[], ...$names)['bar'], ':';
    echo implode(',', call_user_func('get_class_methods', ...[], ...$objects));
"#, "7:m");
}

/// First-class class introspection preserves runtime object and class-name element types.
#[test]
fn test_core_introspection_spread_mixed_static_dynamic_fcc() {
    assert_mixed_spread_case(r#"
    $vars = get_class_vars(...);
    $methods = get_class_methods(...);
    echo $vars(...[], ...$names, ...[])['bar'], ':';
    echo implode(',', $methods(...[], ...$objects, ...[]));
"#, "7:m");
}

/// A nested dynamic unpack must retain its runtime length rather than the literal node count.
#[test]
fn test_core_introspection_spread_mixed_static_dynamic_nested() {
    assert_mixed_spread_case(r#"
    echo get_class_vars(...[...$names])['bar'];
"#, "7");
}

/// Multiple runtime unpack sources are evaluated once in source order before arity rejection.
#[test]
fn test_core_introspection_spread_dynamic_tail_order_and_bounds() {
    let out = compile_and_run(r#"<?php
class OrderedSpread { public int $bar = 7; public function m(): void {} }
function spreadSource(string $mark, string $name): array { echo $mark; return [$name]; }
function inspectSpreadBounds(string $name): void {
    $names = [$name];
    $empty = $names;
    array_pop($empty);
    try { get_class_vars(...[], ...$empty); } catch (ArgumentCountError $e) { echo 'D'; }
    try { call_user_func('get_class_methods', ...[], ...$empty); } catch (ArgumentCountError $e) { echo 'C'; }
    $vars = get_class_vars(...);
    try { $vars(...[], ...$empty); } catch (ArgumentCountError $e) { echo 'F'; }
    try {
        get_class_vars(...[], ...spreadSource('1', $name), ...spreadSource('2', $name));
    } catch (ArgumentCountError $e) { echo 'A'; }
    try {
        call_user_func('get_class_methods', ...spreadSource('3', $name), ...spreadSource('4', $name));
    } catch (ArgumentCountError $e) { echo 'B'; }
    $methods = get_class_methods(...);
    try { $methods(...$names, ...[$name]); } catch (ArgumentCountError $e) { echo 'M'; }
    echo ':', get_class_vars(...$empty, ...$names)['bar'];
}
inspectSpreadBounds(OrderedSpread::class);
"#);
    assert_eq!(out, "DCF12A34BM:7");
}

/// Multiple literal unpacks and CUF named arguments use the target introspection signature.
#[test]
fn test_core_introspection_spread_multiple_literals_and_cuf_named() {
    let out = compile_and_run(r#"<?php
class MultiSpreadVars { public int $value = 7; public function method(): void {} }
echo get_class_vars(...[], ...[MultiSpreadVars::class])['value'], ':';
echo call_user_func('get_class_vars', ...[], ...[MultiSpreadVars::class])['value'], ':';
$vars = get_class_vars(...);
echo $vars(...[MultiSpreadVars::class], ...[])['value'], ':';
echo call_user_func('get_class_vars', class: MultiSpreadVars::class)['value'], ':';
echo implode(',', call_user_func('get_class_methods', object_or_class: new MultiSpreadVars()));
"#);
    assert_eq!(out, "7:7:7:7:method");
}

/// Empty spreads followed by named arguments must not bypass static specialization.
#[test]
fn test_core_introspection_spread_followed_by_named() {
    let out = compile_and_run(r#"<?php
class SpreadNamed { public int $x = 4; public function m(): void {} }
echo get_class_vars(...[], class: SpreadNamed::class)['x'], ':';
echo implode(',', get_class_methods(...[], object_or_class: new SpreadNamed()));
"#);
    assert_eq!(out, "4:m");
}

/// CUF and CUFA specialize known targets even when their arguments come from variables.
#[test]
fn test_core_introspection_spread_cuf_cufa_dynamic_arrays() {
    let out = compile_and_run(r#"<?php
class SpreadCall { public int $x = 4; public function m(): void {} }
$objects = [new SpreadCall()];
$names = [SpreadCall::class];
echo call_user_func('get_class_vars', ...$names)['x'], ':';
echo call_user_func_array('get_class_vars', $names)['x'], ':';
echo implode(',', call_user_func('get_class_methods', ...$objects)), ':';
echo implode(',', call_user_func_array('get_class_methods', $objects)), ':';
$vars = get_class_vars(...);
$methods = get_class_methods(...);
echo $vars(...$names)['x'], ':', implode(',', $methods(...$objects));
"#);
    assert_eq!(out, "4:4:m:m:4:m");
}

/// Runtime spreads preserve builtin arity errors across direct, CUF, CUFA, and FCC calls.
#[test]
fn test_core_introspection_spread_rejects_surplus_arguments() {
    let out = compile_and_run(r#"<?php
class SpreadArity { public int $x = 4; public function m(): void {} }
$names = [SpreadArity::class, SpreadArity::class];
try { get_class_methods(...$names); } catch (ArgumentCountError $e) { echo 'D'; }
try { call_user_func('get_class_vars', ...$names); } catch (ArgumentCountError $e) { echo 'C'; }
try { call_user_func_array('get_class_methods', $names); } catch (ArgumentCountError $e) { echo 'A'; }
$vars = get_class_vars(...);
try { $vars(...$names); } catch (ArgumentCountError $e) { echo 'F'; }
try { strlen(...$names); } catch (ArgumentCountError $e) { echo 'S'; }
function userExtra(string $first): string { return $first; }
echo ':', userExtra(...$names);
"#);
    assert_eq!(out, "DCAFS:SpreadArity");
}
