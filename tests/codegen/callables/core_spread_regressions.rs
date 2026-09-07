//! Purpose:
//! Exercises class introspection through shared named and unpacked call planning.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_introspection_spread_`.
//!
//! Key details:
//! - Targets are statically known while argument containers can be runtime values.

use crate::support::*;

/// Empty literal unpacks do not become arguments beside runtime class-name or object arrays.
#[test]
fn test_core_introspection_spread_mixed_static_dynamic_matrix() {
    let source = r#"<?php
class MixedSpread { public int $bar = 7; public function m(): void {} }
function inspectMixedSpread(string $name): void {
    $names = [$name];
    $objects = [new MixedSpread()];
    echo get_class_vars(...[], ...$names)['bar'], ':';
    echo implode(',', get_class_methods(...[], ...$names)), ':';
    echo implode(',', get_class_methods(...$objects, ...[])), ':';
    echo call_user_func('get_class_vars', ...[], ...$names)['bar'], ':';
    echo implode(',', call_user_func('get_class_methods', ...[], ...$objects)), ':';
    $vars = get_class_vars(...);
    $methods = get_class_methods(...);
    echo $vars(...[], ...$names, ...[])['bar'], ':';
    echo implode(',', $methods(...[], ...$objects, ...[])), ':';
    echo get_class_vars(...[...$names])['bar'];
}
inspectMixedSpread(MixedSpread::class);
"#;
    assert_eq!(compile_and_run(source), "7:m:m:7:m:7:m:7");
    assert_eq!(compile_and_run_tagged(source), "7:m:m:7:m:7:m:7");
}

/// Multiple runtime unpack sources are evaluated once in source order before arity rejection.
#[test]
fn test_core_introspection_spread_dynamic_tail_order_and_bounds() {
    let out = compile_and_run(r#"<?php
class OrderedSpread { public int $bar = 7; public function m(): void {} }
function spreadSource(string $mark, string $name): array { echo $mark; return [$name]; }
function inspectSpreadBounds(string $name): void {
    $names = [$name];
    $empty = array_slice($names, 1);
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
