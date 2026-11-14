//! Purpose:
//! Exercises exact scalar parameter boundaries and caller evaluation order natively.
//!
//! Called from:
//! - Focused staged codegen integration tests.
//!
//! Key details:
//! - Both optimizer settings must preserve catchable failures and balanced owners.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Calls guard original scalar values after evaluating later arguments and before entering the callee.
#[test]
fn test_strict_conversion_parameters_native_order_kinds_and_owners() {
    let source = r#"<?php
function parameterFloat(float $value): void { echo is_float($value) ? 'F' : 'X'; echo ':', $value; }
function parameterInteger(int $value): void { echo is_int($value) ? 'I' : 'X'; }
function parameterString(string $value): void { echo $value; }
function parameterBool(bool $value): void { echo $value ? 'B' : 'b'; }
function parameterValue(mixed $value): mixed { return $value; }
function parameterLater(): int { echo 'E'; return 1; }
function parameterPair(float $first, int $second): void { echo 'X'; }
function parameterTail(float $first, mixed ...$rest): void { echo 'X'; }
function parameterDiscard(float $value): void {}
function parameterMembers(int|false|null $value): void { echo $value === false ? 'f' : ($value === null ? 'n' : 'i'); }
function parameterOwned(): string { echo 'L'; return 'temporary-' . $GLOBALS['argc']; }
parameterFloat(parameterValue(4.25)); parameterInteger(parameterValue(2));
parameterString(parameterValue('S')); parameterBool(parameterValue(false));
$result = 23;
try { parameterPair(parameterValue('bad'), parameterLater()); $result = 0; } catch (TypeError $e) { echo 'T'; }
try { parameterFloat(parameterValue(2)); } catch (TypeError $e) { echo 'N'; }
try { parameterInteger(parameterValue('2')); } catch (TypeError $e) { echo 'D'; }
try { parameterString(parameterValue(2)); } catch (TypeError $e) { echo 'S'; }
try { parameterBool(parameterValue(1)); } catch (TypeError $e) { echo 'B'; }
try { parameterPair(second: parameterLater(), first: parameterValue('bad')); } catch (TypeError $e) { echo 'M'; }
try { parameterTail(parameterValue('bad'), parameterOwned()); } catch (TypeError $e) { echo 'V'; }
$call = parameterFloat(...);
try { $call(parameterValue(2)); } catch (TypeError $e) { echo 'C'; }
try { call_user_func('parameterFloat', parameterValue(2)); } catch (TypeError $e) { echo 'U'; }
try { parameterDiscard(parameterValue(2)); } catch (TypeError $e) { echo 'P'; }
parameterMembers(parameterValue(1)); parameterMembers(parameterValue(false)); parameterMembers(parameterValue(null));
try { parameterMembers(parameterValue(true)); } catch (TypeError $e) { echo 'Q'; }
echo ':', $result;
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "F:4.25ISbETNDSBEMLVCUPifnQ:23", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
