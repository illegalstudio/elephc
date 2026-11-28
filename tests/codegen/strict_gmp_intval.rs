//! Purpose:
//! Exercises checked GMP builtin narrowing with real managed GMP values.
//!
//! Called from:
//! - Focused native strict cast and ownership regressions.
//!
//! Key details:
//! - Both optimizer settings retain original source checks and permissive controls.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Public direct, named, unpacked and callable forms reject machine-word truncation before return.
#[test]
fn test_strict_gmp_intval_original_magnitudes_native_bundle() {
    if skip_without_gmp_native("test_strict_gmp_intval_original_magnitudes_native_bundle") { return; }
    let source = r#"<?php
function checkedGmpInteger(mixed $value): int { return gmp_intval($value); }
$cast = gmp_intval(...);
echo checkedGmpInteger(gmp_init('9223372036854775807')), '|', $cast('-9223372036854775808'), '|';
echo gmp_intval(num: '0xff'), '|', gmp_intval(...['num' => '0o17']), '|', gmp_intval('0b101'), '|';
foreach (['9223372036854775808', '-9223372036854775809', '0x8000000000000000', str_repeat('9', 90)] as $text) {
    try { checkedGmpInteger($text); echo 'X'; } catch (ValueError $error) { echo 'V'; }
}
$saved = gmp_init('9223372036854775808');
try { $cast($saved); echo 'X'; } catch (ValueError $error) { echo '|', gmp_strval($saved); }
echo '|';
try { call_user_func('gmp_intval', '9223372036854775808'); echo 'X'; } catch (ValueError $error) { echo 'C'; }
try { checkedGmpInteger('12tail'); echo 'X'; } catch (ValueError $error) { echo 'P'; }
try { checkedGmpInteger([]); echo 'X'; } catch (TypeError $error) { echo 'T'; }
"#;
    for strict in [false, true] { for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_gmp(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, strict), optimize);
        assert!(output.success, "strict={strict} optimize={optimize}: {:?} {}", output.status, output.stderr);
        let expected = if strict { "9223372036854775807|-9223372036854775808|255|15|5|VVVV|9223372036854775808|CPT" }
            else { "9223372036854775807|-9223372036854775808|255|15|5|XXXXX|XPT" };
        assert_eq!(output.stdout, expected, "strict={strict} optimize={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    } }
}

/// Weak floats are checked before integer adaptation; invalid domains preserve PHP error priority.
#[test]
fn test_strict_gmp_intval_original_floats_native_bundle() {
    if skip_without_gmp_native("test_strict_gmp_intval_original_floats_native_bundle") { return; }
    let source = r#"<?php
function checkedGmpFloat(mixed $value): int { return gmp_intval($value); }
echo checkedGmpFloat(7.0), '|', checkedGmpFloat(-9223372036854775808.0), '|';
foreach ([9223372036854775808.0, -9223372036854777856.0, INF, NAN] as $value) {
    try { checkedGmpFloat($value); echo 'X'; } catch (ValueError $error) { echo 'V'; } catch (TypeError $error) { echo 'T'; }
}
"#;
    for strict in [false, true] { for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_gmp(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, strict), optimize);
        assert!(output.success, "strict={strict} optimize={optimize}: {:?} {}", output.status, output.stderr);
        assert_eq!(output.stdout, if strict { "7|-9223372036854775808|VVVV" } else { "7|-9223372036854775808|TTTT" });
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    } }
}

/// PHP strict_types rejects original noncontract kinds before the checked numeric value guard.
#[test]
fn test_strict_gmp_intval_php_strict_domain_priority_native_bundle() {
    if skip_without_gmp_native("test_strict_gmp_intval_php_strict_domain_priority_native_bundle") { return; }
    let source = r#"<?php declare(strict_types=1);
function strictGmpInteger(mixed $value): int { return gmp_intval($value); }
foreach ([7.0, 9223372036854775808.0, INF, NAN, true, null] as $value) {
    try { strictGmpInteger($value); echo 'X'; } catch (TypeError $error) { echo 'T'; } catch (ValueError $error) { echo 'V'; }
}
echo '|', strictGmpInteger('9223372036854775807');
"#;
    for strict in [false, true] { for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_gmp(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, strict), optimize);
        assert!(output.success, "strict={strict} optimize={optimize}: {:?} {}", output.status, output.stderr);
        assert_eq!(output.stdout, "TTTTTT|9223372036854775807");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    } }
}
