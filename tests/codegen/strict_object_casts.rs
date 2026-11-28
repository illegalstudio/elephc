//! Purpose:
//! Exercises checked native-object casts with the real managed GMP and XML libraries.
//!
//! Called from:
//! - Focused native package jobs and staged semantic-profile implementation verification.
//!
//! Key details:
//! - Source domains are dynamic; separate checked EIR tests cover the selected-profile checker.
//! - Both AST/EIR optimizer settings must preserve failure owners and permissive controls.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Original GMP magnitudes are checked before legacy get_si truncation across numeric call surfaces.
#[test]
fn test_strict_object_casts_gmp_native_original_magnitudes_bundle() {
    if skip_without_gmp_native("test_strict_object_casts_gmp_native_original_magnitudes_bundle") { return; }
    let source = r#"<?php
function originalGmpInt(GMP $value): int { return (int)$value; }
function originalErasedGmpInt(mixed $value): int { return intval($value); }
echo originalGmpInt(gmp_init('9223372036854775807')), '|', originalErasedGmpInt(gmp_init('-9223372036854775808')), '|';
$cast = intval(...);
foreach (['9223372036854775808', '-9223372036854775809'] as $text) {
    $value = gmp_init($text);
    try { originalGmpInt($value); echo 'X'; } catch (ValueError $e) { echo 'V'; }
    try { originalErasedGmpInt($value); echo 'X'; } catch (ValueError $e) { echo 'E'; }
    try { $cast($value); echo 'X'; } catch (ValueError $e) { echo 'F'; }
    echo '|';
}
try { (int)gmp_init('9223372036854775808'); echo 'X'; } catch (ValueError $e) { echo 'T'; }
mixed $saved = gmp_init('9223372036854775808');
try { settype($saved, 'int'); echo 'X'; } catch (ValueError $e) { echo gmp_strval($saved), '|'; }
echo is_infinite(floatval(gmp_init(str_repeat('9', 400)))) ? 'I' : 'X';
echo floatval(gmp_init('9007199254740995')) === 9007199254740994.0 ? 'R' : 'X';
"#;
    for strict in [false, true] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, strict);
            let output = StrictFixture::new(source).capture_staged_gmp(profile, optimize);
            assert!(output.success, "{strict} {optimize}: {:?} {}", output.status, output.stderr);
            assert_eq!(output.stdout, if strict { "9223372036854775807|-9223372036854775808|VEF|VEF|T9223372036854775808|IR" }
                else { "9223372036854775807|-9223372036854775808|XXX|XXX|XXIR" }, "{strict} {optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{strict} {optimize}: {}", output.stderr);
        }
    }
}

/// Native XML text bypasses public string overrides and retains temporary receiver owners on failure.
/// The permissive erased-float control retains its preexisting string-override behavior.
#[test]
fn test_strict_object_casts_simplexml_native_original_strings_bundle() {
    if skip_without_xml_native("test_strict_object_casts_simplexml_native_original_strings_bundle") { return; }
    let source = r#"<?php
class StrictNativeXmlCast extends SimpleXMLElement { public function __toString(): string { return '0'; } }
function originalXmlInt(SimpleXMLElement $value): int { return (int)$value; }
function originalErasedXmlFloat(mixed $value): float { return floatval($value); }
echo originalXmlInt(new StrictNativeXmlCast('<n>9223372036854775807</n>')), '|';
$cast = intval(...);
foreach (['12tail', '', '9223372036854775808'] as $text) {
    $value = new StrictNativeXmlCast('<n>' . $text . '</n>');
    try { originalXmlInt($value); echo 'X'; } catch (ValueError $e) { echo 'V'; }
    try { $cast($value); echo 'X'; } catch (ValueError $e) { echo 'F'; }
    echo '|';
}
try { originalErasedXmlFloat(new StrictNativeXmlCast('<n>junk</n>')); echo 'X'; } catch (ValueError $e) { echo 'T'; }
echo '|', originalErasedXmlFloat(new StrictNativeXmlCast('<n>1.5</n>'));
"#;
    for strict in [false, true] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, strict);
            let output = StrictFixture::new(source).capture_staged_xml(profile, optimize);
            assert!(output.success, "{strict} {optimize}: {:?} {}", output.status, output.stderr);
            assert_eq!(output.stdout, if strict { "9223372036854775807|VF|VF|VF|T|1.5" }
                else { "9223372036854775807|XX|XX|XX|X|0" }, "{strict} {optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{strict} {optimize}: {}", output.stderr);
        }
    }
}
