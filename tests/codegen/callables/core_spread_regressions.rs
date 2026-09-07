//! Purpose:
//! Exercises class introspection through shared named and unpacked call planning.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_introspection_spread_`.
//!
//! Key details:
//! - Targets are statically known while argument containers can be runtime values.

use crate::support::*;

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
