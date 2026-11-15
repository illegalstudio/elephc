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

/// Dynamic builtin inputs reach exact guards before ABI coercion or Stringable hooks.
#[test]
fn test_strict_conversion_builtin_original_dynamic_inputs_bundle() {
    let source = r#"<?php
function strictBuiltinLength(mixed $value): void { echo strlen($value), 'A'; }
function strictBuiltinRepeat(mixed $value): void { echo str_repeat('a', $value), 'A'; }
function strictBuiltinValue(mixed $value): mixed { return $value; }
function strictBuiltinLater(): int { echo 'E'; return 2; }
function strictBuiltinNamed(mixed $value): void { echo strlen(string: $value); }
function strictBuiltinSpread(array $values): void { echo strlen(...$values); }
function strictBuiltinCall(mixed $value): void { $call = strlen(...); echo $call($value); }
function strictBuiltinOrder(mixed $value): void { echo str_repeat($value, strictBuiltinLater()); }
function strictBuiltinNamedOrder(mixed $value): void { echo str_repeat(times: strictBuiltinLater(), string: $value); }
function strictBuiltinJson(mixed $value): void { echo json_decode($value); }
class StrictBuiltinText { public function __toString(): string { echo 'S'; return 'converted'; } }
try { strictBuiltinLength(7); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinLength(true); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinLength(null); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinLength(new StrictBuiltinText()); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinRepeat(2.0); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictBuiltinNamed(7); echo 'X'; } catch (TypeError $error) { echo 'N'; unset($error); }
try { strictBuiltinSpread([7]); echo 'X'; } catch (TypeError $error) { echo 'P'; unset($error); }
try { strictBuiltinCall(7); echo 'X'; } catch (TypeError $error) { echo 'C'; unset($error); }
try { strictBuiltinOrder(new StrictBuiltinText()); echo 'X'; } catch (TypeError $error) { echo 'O'; unset($error); }
try { strictBuiltinNamedOrder(new StrictBuiltinText()); echo 'X'; } catch (TypeError $error) { echo 'M'; unset($error); }
try { strictBuiltinJson(new StrictBuiltinText()); echo 'X'; } catch (TypeError $error) { echo 'J'; unset($error); }
echo '|'; strictBuiltinLength(strval(7));
echo '|', substr('abc', 0, null), substr('abc', 1, strictBuiltinValue(null));
gc_collect_cycles();
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "TTTTTNPCEOEMJ|1A|abcbc", "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

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
