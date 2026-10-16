//! Purpose:
//! Regressions for supplied enum-interface argument types in keyed static unpacks.
//!
//! Called from:
//! - The types codegen integration harness.
//!
//! Key details:
//! - Source arrays are evaluated once before selected signature binding.
//! - Type checks stop at the first missing required slot and preserve key conversion timing.

use crate::support::*;

/// Checks one static unpack without letting an omission skip an earlier supplied type.
fn verify(unpack: &str, expected: &str) {
    let source = format!(r#"<?php
interface I {{ public function f(int $a, int $b): int; }}
enum E implements I {{ case A; public function f(int $a, int $b): int {{ return $a + $b; }} }}
function bad(): mixed {{ echo 'S'; return 'bad'; }}
set_error_handler(function(int $level, string $message): bool {{ echo 'W'; return true; }});
function invoke(I $object): int {{ return $object->f(...{unpack}); }}
try {{ invoke(E::A); }}
catch (Throwable $error) {{ echo get_class($error), ':', $error->getMessage(); }}
"#);
    assert_eq!(compile_and_run(&source), expected);
}

/// Verifies numeric zero unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_numeric_zero() {
    verify("[0 => bad()]", "STypeError:E::f(): Argument #1 ($a) must be of type int, string given");
}

/// Preserves numeric zero unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_numeric_zero_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_numeric_zero);
}

/// Verifies numeric sparse unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_numeric_sparse() {
    verify("[1 => bad()]", "STypeError:E::f(): Argument #1 ($a) must be of type int, string given");
}

/// Preserves numeric sparse unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_numeric_sparse_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_numeric_sparse);
}

/// Verifies numeric string unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_numeric_string() {
    verify("['1' => bad()]", "STypeError:E::f(): Argument #1 ($a) must be of type int, string given");
}

/// Preserves numeric string unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_numeric_string_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_numeric_string);
}

/// Verifies boolean unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_boolean() {
    verify("[true => bad()]", "STypeError:E::f(): Argument #1 ($a) must be of type int, string given");
}

/// Preserves boolean unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_boolean_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_boolean);
}

/// Verifies fractional unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_fractional() {
    verify("[1.5 => bad()]", "SWTypeError:E::f(): Argument #1 ($a) must be of type int, string given");
}

/// Preserves fractional unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_fractional_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_fractional);
}

/// Verifies named first unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_named_first() {
    verify("['a' => bad()]", "STypeError:E::f(): Argument #1 ($a) must be of type int, string given");
}

/// Preserves named first unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_named_first_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_named_first);
}

/// Verifies named second unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_named_second() {
    verify("['b' => bad()]", "SArgumentCountError:E::f(): Argument #1 ($a) not passed");
}

/// Preserves named second unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_named_second_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_named_second);
}

/// Verifies named last wins unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_named_last_wins() {
    verify(
        "['a' => bad(), 'a' => 3]",
        "SArgumentCountError:Too few arguments to function E::f(), 1 passed and exactly 2 expected",
    );
}

/// Preserves named last wins unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_named_last_wins_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_named_last_wins);
}

/// Verifies numeric valid unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_numeric_valid() {
    verify("[1 => 3]", "ArgumentCountError:Too few arguments to function E::f(), 1 passed and exactly 2 expected");
}

/// Preserves numeric valid unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_numeric_valid_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_numeric_valid);
}

/// Verifies named valid unpack binding and selected-method diagnostics.
#[test]
fn test_enum_keyed_review_named_valid() {
    verify(
        "['a' => 3]",
        "ArgumentCountError:Too few arguments to function E::f(), 1 passed and exactly 2 expected",
    );
}

/// Preserves named valid unpack behavior with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_named_valid_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_named_valid);
}

/// Direct named omissions reach the same selected branch after all caller side effects.
#[test]
fn test_enum_keyed_review_direct_named() {
    let source = r#"<?php
interface I { public function f(int $a, int $b): int; }
enum E implements I { case A; public function f(int $a, int $b): int { return $a + $b; } }
function bad(): mixed { echo 'S'; return 'bad'; }
function first(I $value): int { return $value->f(a: bad()); }
function second(I $value): int { return $value->f(b: bad()); }
try { first(E::A); } catch (Throwable $error) { echo get_class($error), '|'; }
try { second(E::A); } catch (Throwable $error) { echo get_class($error); }
"#;
    assert_eq!(compile_and_run(source), "STypeError|SArgumentCountError");
}

/// Direct named omissions also retain their ordering without EIR optimization.
#[test]
fn test_enum_keyed_review_direct_named_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_direct_named);
}

/// Earlier defaults cannot hide the first required omission in a named static unpack.
#[test]
fn test_enum_keyed_review_earlier_default() {
    assert_eq!(compile_and_run(r#"<?php
interface I { public function f(int $a, int $b): int; }
enum E implements I { case A; public function f(int $a = 1, int $b): int { return $a + $b; } }
function bad(): mixed { echo 'S'; return 'bad'; }
function first(I $value): int { return $value->f(...['a' => bad()]); }
function second(I $value): int { return $value->f(...['b' => bad()]); }
try { first(E::A); } catch (Throwable $error) { echo get_class($error), '|'; }
try { second(E::A); } catch (Throwable $error) { echo $error->getMessage(); }
"#), "STypeError|SE::f(): Argument #1 ($a) not passed");
}

/// Named omissions before a later required slot retain their meaning without EIR optimization.
#[test]
fn test_enum_keyed_review_earlier_default_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_earlier_default);
}

/// A projected fractional key must address the integer key produced by original construction.
#[test]
fn test_enum_keyed_review_fractional_lookup_control() {
    assert_eq!(compile_and_run(r#"<?php
set_error_handler(function(int $level, string $message): bool { echo 'W'; return true; });
function bad(): mixed { echo 'S'; return 'bad'; }
$a = [1.5 => bad()];
echo '|', $a[1], '|', $a[(int)1.5];
"#), "SW|bad|bad");
}

/// Rejected keyed containers and their projected object owners are released on every throw.
#[test]
fn test_enum_keyed_review_owners() {
    for unpack in ["[1 => new Payload()]", "['a' => new Payload()]", "['b' => new Payload()]"] {
        let mut baseline = None;
        for iterations in [1, 16] {
            let result = compile_and_run_with_heap_debug(&format!(r#"<?php
interface I {{ public function f(int $a, int $b): int; }}
enum E implements I {{ case A; public function f(int $a, int $b): int {{ return $a + $b; }} }}
class Payload {{ public function __destruct() {{ echo 'D'; }} }}
function reject(I $value): int {{ return $value->f(...{unpack}); }}
for ($i = 0; $i < {iterations}; $i++) {{
    try {{ reject(E::A); }} catch (Throwable $error) {{ echo 'T'; unset($error); }}
}}
"#));
            assert!(result.success, "{}", result.stderr);
            assert_eq!(result.stdout, "DT".repeat(iterations));
            let retained = ["live_blocks=", "live_bytes="].map(|metric| {
                result.stderr.lines().next().unwrap().split_whitespace()
                    .find_map(|field| field.strip_prefix(metric)).unwrap().parse::<usize>().unwrap()
            });
            if let Some(baseline) = baseline {
                assert_eq!(retained, baseline, "{unpack}: {}", result.stderr);
            } else { baseline = Some(retained); }
        }
    }
}

/// Repeated rejected projections retain no growing owners with EIR optimization disabled.
#[test]
fn test_enum_keyed_review_owners_no_ir_opt() {
    without_ir_opt(test_enum_keyed_review_owners);
}

/// Integer-like duplicate keys collapse before unpacking without skipping source evaluation.
#[test]
fn test_enum_keyed_tenth_review_duplicate_positional_keys() {
    let source = r#"<?php
interface I { public function f(int $a, int $b): int; }
enum E implements I { case A; public function f(int $a, int $b): int { return $a + $b; } }
function mark(int $value): int { echo $value; return $value; }
function invoke(I $value): int {
    return $value->f(...[0 => mark(1), false => mark(2), '0' => mark(3)]);
}
function complete(I $value): int {
    return $value->f(...[0 => mark(1), false => mark(2), '0' => mark(3), 1 => mark(4)]);
}
try { invoke(E::A); }
catch (ArgumentCountError $error) { echo '|', $error->getMessage(); }
echo '|', complete(E::A);
"#;
    assert_eq!(
        compile_and_run(source),
        "123|Too few arguments to function E::f(), 1 passed and exactly 2 expected|12347"
    );
}

/// Duplicate positional-key normalization is independent of EIR optimization.
#[test]
fn test_enum_keyed_tenth_review_duplicate_positional_keys_no_ir_opt() {
    without_ir_opt(test_enum_keyed_tenth_review_duplicate_positional_keys);
}

/// A final omitted named slot uses PHP's count diagnostic, not the interior-hole wording.
#[test]
fn test_enum_keyed_tenth_review_trailing_named_omission() {
    let source = r#"<?php
interface I { public function f(int $a, int $b): int; }
enum E implements I { case A; public function f(int $a, int $b): int { return $a + $b; } }
function direct(I $value): int { return $value->f(a: 1); }
function unpack(I $value): int { return $value->f(...['a' => 1]); }
foreach ([direct(...), unpack(...)] as $callable) {
    try { $callable(E::A); }
    catch (ArgumentCountError $error) { echo $error->getMessage(), '|'; }
}
"#;
    assert_eq!(
        compile_and_run(source),
        "Too few arguments to function E::f(), 1 passed and exactly 2 expected|".repeat(2)
    );
}

/// Trailing named-argument diagnostics are stable without EIR optimization.
#[test]
fn test_enum_keyed_tenth_review_trailing_named_omission_no_ir_opt() {
    without_ir_opt(test_enum_keyed_tenth_review_trailing_named_omission);
}
