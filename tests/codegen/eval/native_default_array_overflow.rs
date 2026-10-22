//! Purpose:
//! Regression tests for the saturated cursor shared by native-default spreads and eval literals.
//!
//! Called from:
//! - `tests/codegen/eval.rs` through the codegen integration harness.
//!
//! Key details:
//! - Native functions are called from opaque eval so their normalized default metadata is read.
//! - Each argv case runs in a fresh process and must leave a clean heap; stdout-only bundling
//!   would lose that per-program ownership check.

use crate::support::compile_and_run_with_heap_debug_per_argv;

/// Ordinary and spread entries share final-key availability and failure cleanup.
#[test]
fn test_dynamic_eval_native_default_spread_max_key_availability() {
    let program = r#"<?php
function freeSpread(array $items = [PHP_INT_MAX - 1 => "a", ...["b"]]): array { return $items; }
function occupiedSpread(array $items = [PHP_INT_MAX => "a", ...["b"]]): array { echo "entered"; return $items; }
function exhaustedSpread(array $items = [PHP_INT_MAX - 1 => "a", ...["b", "c"]]): array { echo "entered"; return $items; }
function stringSpread(array $items = [PHP_INT_MAX => "a", ...["tag" => "b"]]): array { return $items; }
function emptySpread(array $items = [PHP_INT_MAX => "a", ...[]]): array { return $items; }
eval($argv[1]);
"#;
    let cases = [
        ("echo json_encode(freeSpread());", "{\"9223372036854775806\":\"a\",\"9223372036854775807\":\"b\"}"),
        ("try { occupiedSpread(); } catch (Error $error) { echo $error->getMessage(); unset($error); }", "Cannot add element to the array as the next element is already occupied"),
        ("try { exhaustedSpread(); } catch (Error $error) { echo $error->getMessage(); unset($error); }", "Cannot add element to the array as the next element is already occupied"),
        ("echo json_encode(stringSpread());", "{\"9223372036854775807\":\"a\",\"tag\":\"b\"}"),
        ("echo json_encode(emptySpread());", "{\"9223372036854775807\":\"a\"}"),
    ];
    let arguments = cases.iter().map(|(source, _)| vec![*source]).collect::<Vec<_>>();
    let argvs = arguments.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv(program, &argvs);
    for (output, (source, expected)) in outputs.iter().zip(cases) {
        assert!(output.success, "{source}\n{}", output.stderr);
        assert_eq!(output.stdout, expected, "{source}");
        assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{source}\n{}", output.stderr);
    }
}
/// Reflection stays lazy while native callable adapters materialize defaults on demand.
#[test]
fn test_dynamic_eval_native_default_spread_reflection_and_callables() {
    let program = r#"<?php
function nativeSpread(array $items = [PHP_INT_MAX - 1 => "a", ...["b"]]): array { return $items; }
function nativeOverflow(array $items = [PHP_INT_MAX => "a", ...["b"]]): array { echo "entered"; return $items; }
class NativeSpreadDefaults {
    public static function valid(array $items = [PHP_INT_MAX - 1 => "a", ...["b"]]): array { return $items; }
    public static function invalid(array $items = [PHP_INT_MAX => "a", ...["b"]]): array { echo "entered"; return $items; }
    public function instance(array $items = [PHP_INT_MAX - 1 => "a", ...["b"]]): array { return $items; }
    public function __construct(array $items = [PHP_INT_MAX => "a", ...["b"]]) { echo "constructed"; }
}
eval($argv[1]);
"#;
    let valid = "{\"9223372036854775806\":\"a\",\"9223372036854775807\":\"b\"}";
    let overflow = "Cannot add element to the array as the next element is already occupied";
    let cases = [
        ("echo json_encode((new ReflectionParameter('nativeSpread', 0))->getDefaultValue());", valid),
        ("try { (new ReflectionParameter('nativeOverflow', 0))->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(); unset($e); }", overflow),
        ("echo json_encode((new ReflectionParameter(['NativeSpreadDefaults', 'valid'], 0))->getDefaultValue());", valid),
        ("try { (new ReflectionParameter(['NativeSpreadDefaults', 'invalid'], 0))->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(); unset($e); }", overflow),
        ("echo json_encode((new ReflectionParameter(['NativeSpreadDefaults', 'instance'], 0))->getDefaultValue());", valid),
        ("try { (new ReflectionParameter(['NativeSpreadDefaults', '__construct'], 0))->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(); unset($e); }", overflow),
        ("$p = new ReflectionParameter('nativeOverflow', 0); echo $p->isDefaultValueAvailable() ? 'available' : 'missing'; echo json_encode(nativeOverflow([]));", "availableentered[]"),
        ("$f = nativeSpread(...); echo json_encode($f());", valid),
        ("$f = nativeOverflow(...); try { $f(); } catch (Error $e) { echo $e->getMessage(); unset($e); }", overflow),
        ("echo json_encode(call_user_func(['NativeSpreadDefaults', 'valid']));", valid),
        ("try { call_user_func(['NativeSpreadDefaults', 'invalid']); } catch (Error $e) { echo $e->getMessage(); unset($e); }", overflow),
        ("try { new NativeSpreadDefaults(); } catch (Error $e) { echo $e->getMessage(); unset($e); }", overflow),
        ("$o = new NativeSpreadDefaults([]); echo json_encode($o->instance());", "constructed{\"9223372036854775806\":\"a\",\"9223372036854775807\":\"b\"}"),
    ];
    let arguments = cases.iter().map(|(source, _)| vec![*source]).collect::<Vec<_>>();
    let argvs = arguments.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv(program, &argvs);
    for (output, (source, expected)) in outputs.iter().zip(cases) {
        assert!(output.success, "{source}\n{}", output.stderr);
        assert_eq!(output.stdout, expected, "{source}");
        assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{source}\n{}", output.stderr);
    }
}

/// Direct AOT calls and lazy Reflection factories share spread failure cleanup.
#[test]
fn test_native_default_spread_direct_calls_and_reflection() {
    let program = r#"<?php
function directValid(array $items = [PHP_INT_MAX - 1 => "a", ...["b"]]): array { return $items; }
function directInvalid(array $items = [PHP_INT_MAX => "a", ...["b"]]): array { echo "entered"; return $items; }
if ($argc > 1) {
    try { directInvalid(); } catch (Error $e) { echo $e->getMessage(), "|"; unset($e); }
    echo json_encode(directValid());
} else {
    $parameter = new ReflectionParameter('directInvalid', 0);
    try { $parameter->getDefaultValue(); } catch (Error $e) { echo $e->getMessage(); unset($e); }
    unset($parameter);
}
"#;
    let outputs = compile_and_run_with_heap_debug_per_argv(program, &[&["direct"], &[]]);
    let error = "Cannot add element to the array as the next element is already occupied";
    assert_eq!(outputs[0].stdout, format!("{error}|{{\"9223372036854775806\":\"a\",\"9223372036854775807\":\"b\"}}"));
    assert_eq!(outputs[1].stdout, error);
    for output in outputs {
        assert!(output.success, "{}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
    }
}
