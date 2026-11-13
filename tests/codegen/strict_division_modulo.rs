//! Purpose:
//! Tests checked AOT division/remainder conversions, callbacks and storage atomicity.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Optimizer parity and heap debugging protect newly explicit arithmetic policies.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// AOT dynamic numeric operations preserve callbacks, catch clauses and failed reference writes.
#[test]
fn test_strict_aot_division_modulo_callbacks_failures_and_owned_results() {
    let source = r#"<?php
function checkedDiv(mixed $left, mixed $right): float { return $left / $right; }
function checkedMod(mixed $left, mixed $right): int { return $left % $right; }
function discardedFloatMod(float $left): void {
    try { $left % 2; } catch (RuntimeException $error) { echo 'Q'; }
}
function referenceMod(mixed &$value, mixed $right): void { $value %= $right; }
function referenceDiv(mixed &$value, mixed $right): void { $value /= $right; }
try { checkedDiv('bad', 2); } catch (TypeError $error) { echo 'D'; }
try { checkedDiv(1, 0); } catch (DivisionByZeroError $error) { echo 'Z'; }
try { checkedMod(1, 0); } catch (DivisionByZeroError $error) { echo 'M'; }
echo '|', checkedMod(PHP_INT_MIN, -1), ':', checkedDiv('7.5', 2);
$observed = 1;
set_error_handler(function (int $severity, string $message): bool { global $observed; $observed = 9; echo $severity === 8192 ? 'D' : 'W'; return true; });
echo '|', checkedMod('1e20', 2), ':', $observed, '|';
try { checkedMod('5.5x', 'bad'); } catch (TypeError $error) { echo 'T'; }
restore_error_handler();
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('discarded modulo'); });
echo '|'; discardedFloatMod(7.5); restore_error_handler();
$value = $argc > 0 ? 7 : 8;
try { referenceMod($value, 0); } catch (DivisionByZeroError $error) { echo '|R', $value; }
try { referenceDiv($value, 0); } catch (DivisionByZeroError $error) { echo ':S', $value; }
referenceMod($value, 3); echo ':U', $value; referenceDiv($value, 2); echo ':V', $value;
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "DZM|0:3.75|D1:9|WDT|Q|R7:S7:U1:V0.5", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Default float modulo still produces PHP integers when assigned to an inferred local.
#[test]
fn test_strict_aot_division_modulo_default_float_remainder_result_kind() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function floatRemainderKind(float $left, float $right): mixed {
    $result = $left % $right; echo is_int($result) ? 'I' : 'F'; return $result;
}
set_error_handler(function (int $severity, string $message): bool { return true; });
echo floatRemainderKind(7.5, 2.0); restore_error_handler();
"#);
    assert!(output.success, "{} {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "I1");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
