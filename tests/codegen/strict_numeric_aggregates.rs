//! Purpose:
//! Exercises checked sum/product values, callable contracts and exception ownership.
//!
//! Called from:
//! - Focused native codegen integration tests.
//!
//! Key details:
//! - Staged lowering exercises the new kernel before advertising the complete overflow flag.
//! - Each optimizer mode verifies exact values and a balanced native heap.

use crate::support::*;

/// Integer reductions throw through every call route without losing sources or partial owners.
#[test]
fn test_strict_numeric_aggregates_staged_native_values_and_ownership() {
    let source = r#"<?php
function strict_array_total(mixed $items): mixed { return array_sum($items); }
function strict_array_product(mixed $items): mixed { return array_product($items); }
$maximum = $argc > 0 ? PHP_INT_MAX : 0;
try { strict_array_total([$maximum, 1]); } catch (ArithmeticError $error) { echo 'S'; }
try { strict_array_product([$maximum, 2]); } catch (ArithmeticError $error) { echo 'P'; }
$sum = array_sum(...); $product = array_product(...);
try { $sum(['max' => $maximum, 'one' => 1]); } catch (ArithmeticError $error) { echo 'F'; }
try { $product([$maximum, 2]); } catch (ArithmeticError $error) { echo 'G'; }
try { call_user_func('array_sum', [$maximum, 1]); } catch (ArithmeticError $error) { echo 'C'; }
try { call_user_func('array_product', [$maximum, 2]); } catch (ArithmeticError $error) { echo 'D'; }
try { call_user_func_array('array_sum', [[$maximum, 1]]); } catch (ArithmeticError $error) { echo 'A'; }
try { call_user_func_array('array_product', [[$maximum, 2]]); } catch (ArithmeticError $error) { echo 'B'; }
$name = $argc > 0 ? 'array_sum' : 'array_product';
try { $name([$maximum, 1]); } catch (ArithmeticError $error) { echo 'N'; }
$text = $argc > 0 ? '9223372036854775807' : '0';
try { strict_array_total([$text . '', 1]); } catch (ArithmeticError $error) { echo 'T'; }
try { strict_array_product([$text . '', 2]); } catch (ArithmeticError $error) { echo 'M'; }
$items = [$maximum, 1]; $copy = $items;
try { array_sum($items); } catch (ArithmeticError $error) { echo 'O'; }
echo ':', $items[0], ':', $copy[1];
echo ':', array_sum([]), ':', array_product([]), ':', array_sum([-9223372036854775808, 0]);
echo ':', array_product([-2, 4611686018427387904]);
echo ':', is_float(strict_array_total([1.0, $maximum, $maximum]));
echo ':', is_float(strict_array_product([1.0, $maximum, 2]));
echo ':', strict_array_total(['9007199254740993', 0]), ':', strict_array_product([2, true, null]);
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('aggregate warning'); });
try { array_sum([1, []]); } catch (RuntimeException $error) { echo ':E'; } finally { echo 'Z'; }
restore_error_handler();
try { strict_array_total(1); } catch (TypeError $error) { echo 'X'; }
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "SPFGCDABNTMO:9223372036854775807:1:0:1:-9223372036854775808:-9223372036854775808:1:1:9007199254740993:0:EZX", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Callback fusion fallbacks retire every partial mapped array when checked arithmetic throws.
#[test]
fn test_strict_numeric_aggregates_staged_callback_fusions_and_ownership() {
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
    for (expression, label) in [
        ("array_reduce($items, fn($carry, $item) => $carry + $item, 0)", "R"),
        ("array_sum(array_map(fn($item) => $item + 1, $items))", "U"),
        ("array_sum(array_map(fn(int $item): int => $item + 1, $items))", "V"),
    ] {
        let source = format!("<?php $maximum = $argc > 0 ? PHP_INT_MAX : 0; $items = [$maximum, 1]; try {{ {expression}; }} catch (ArithmeticError $error) {{ echo '{label}'; }}");
        for optimize in [false, true] {
            let output = StrictFixture::new(&source).capture_staged_lowering(profile, optimize);
            assert!(output.success, "{label} opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, label, "opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{label} opt={optimize}: {}", output.stderr);
        }
    }
}

/// PHP callback exceptions retire previously transferred Mixed cells and their owned strings.
#[test]
fn test_strict_numeric_aggregates_permissive_mixed_map_exception_owners() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function guardedNumericMap(int $value): mixed {
    if ($value === 2) { throw new RuntimeException('second item'); }
    return 'mapped:' . $value;
}
function guardedStringMap(string $value): mixed {
    if ($value === 'b') { throw new RuntimeException('second string'); }
    return $value . '!';
}
$numbers = [1, 2]; $strings = ['a', 'b'];
try { array_map(guardedNumericMap(...), $numbers); } catch (RuntimeException $error) { echo 'I'; }
try { array_map(guardedStringMap(...), $strings); } catch (RuntimeException $error) { echo 'S'; }
"#);
    assert!(output.success, "{} {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "IS");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Default mixed-literal folding preserves overwritten value effects and PHP key/append identity.
#[test]
fn test_strict_literal_array_plan_default_key_and_value_effects() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function literalPlanValue(string $tag, int $value): int { echo $tag; return $value; }
$items = [1.0 => literalPlanValue('a', 10), '1' => literalPlanValue('b', 20), '01' => literalPlanValue('c', 30), literalPlanValue('d', 40)];
foreach ($items as $key => $value) { echo '|', gettype($key), ':', $key, '=', $value; }
$fractional = @([1.5 => 10, 20]);
foreach ($fractional as $key => $value) { echo '|', $key, '=', $value; }
"#);
    assert!(output.success, "{} {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "abcd|integer:1=20|string:01=30|integer:2=40|1=10|2=20");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
