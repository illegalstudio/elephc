//! Purpose:
//! Exercises class introspection through shared named and unpacked call planning.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_introspection_spread_`.
//!
//! Key details:
//! - Targets are statically known while argument containers can be runtime values.

use crate::support::*;

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
