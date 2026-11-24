//! Purpose:
//! End-to-end coverage for `array_merge()` with a single operand that is not a packed literal, and
//! for array builtins beside `[]` in the arms of `?:`, `match`, `??` and `&&`/`||`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs are php 8.5's (`php -n`); the run is under heap debug and must end clean.

use crate::support::*;

/// `array_merge($operand)` with one boxed `array` operand (a method, function, static or property result, a local) renumbers its integer keys like php; the native one-operand helper took the boxed cell for a packed array and the call returned a string or crashed.
#[test]
fn test_array_merge_with_one_boxed_operand_matches_php() {
    let source = r####"<?php
class X {
    public array $p = [3, 4];
    public function l(int $a): array { return [$a, 'k' => $a + 1, 9 => $a + 2]; }
    public static function s(int $a): array { return [5 => $a]; }
}
function u(int $a): array { return [7 => $a]; }
function run(X $x, int $e): void {
    echo json_encode(array_merge($x->l($e))), "\n";
    echo json_encode(array_merge($x->p)), "\n";
    echo json_encode(array_merge(u($e))), "\n";
    echo json_encode(array_merge(X::s($e))), "\n";
    $l = $x->l($e);
    echo json_encode(array_merge($l)), "\n";
    $s = ['a' => 1, 5 => 2];
    echo json_encode(array_merge($s)), "\n";
    echo json_encode(array_merge()), "\n";
}
run(new X, $argc + 1);"####;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"0\":2,\"k\":3,\"1\":4}\n[3,4]\n[2]\n[2]\n{\"0\":2,\"k\":3,\"1\":4}\n{\"a\":1,\"0\":2}\n[]\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An array builtin over a method result in the non-literal arm of `?:`, `match`, `??`, `&&` and `||`, beside `[]`, returns the array php returns, for `array_merge` (one and two operands), `array_values`, `array_filter`, `array_unique`, `array_reverse`, `array_keys` and `array_slice` (`elephc-vjho`).
#[test]
fn test_array_builtin_beside_an_empty_literal_arm_matches_php() {
    let source = r####"<?php
class X {
    public function l(int $a): array { return [$a, $a]; }
    public function n(int $a): ?array { return $a > 1 ? [$a] : null; }
}
function t(X $x, int $e): array { return $e === 1 ? [] : array_merge($x->l($e)); }
function t2(X $x, int $e): array { return $e === 1 ? [] : array_merge($x->l($e), $x->l($e)); }
function t3(X $x, int $e): array { return $e === 1 ? [] : array_values($x->l($e)); }
function t4(X $x, int $e): array { return $e === 1 ? [] : array_filter($x->l($e)); }
function t5(X $x, int $e): array { return $e === 1 ? [] : array_unique($x->l($e)); }
function t6(X $x, int $e): array { return $e === 1 ? [] : array_reverse($x->l($e)); }
function t7(X $x, int $e): array { return $e === 1 ? [] : array_keys($x->l($e)); }
function t8(X $x, int $e): array { return $e === 1 ? [] : array_slice($x->l($e), 1); }
function m(X $x, int $e): array { return match ($e) { 1 => [], default => array_merge($x->l($e)) }; }
function c(X $x, int $e): array { return $x->n($e) ?? array_merge($x->l($e)); }
function c2(X $x, int $e): array { return [] ?: array_merge($x->l($e)); }
function a(X $x, int $e): array { return $e === 1 && array_merge($x->l($e)) ? [1] : array_merge($x->l($e)); }
function o(X $x, int $e): array { return ($e === 1 || array_merge($x->l($e))) ? array_merge($x->l($e)) : []; }
$x = new X;
foreach (['t', 't2', 't3', 't4', 't5', 't6', 't7', 't8', 'm', 'c', 'c2', 'a', 'o'] as $f) {
    echo $f, ' ', json_encode($f($x, 1)), ' ', json_encode($f($x, 2)), "\n";
}"####;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "t [] [2,2]\nt2 [] [2,2,2,2]\nt3 [] [2,2]\nt4 [] [2,2]\nt5 [] [2]\nt6 [] [2,2]\nt7 [] [0,1]\nt8 [] [2]\nm [] [2,2]\nc [1,1] [2]\nc2 [1,1] [2,2]\na [1] [2,2]\no [1,1] [2,2]\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
