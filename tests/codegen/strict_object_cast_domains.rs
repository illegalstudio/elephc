//! Purpose:
//! Exercises checked native casts through broad and structural object parameter contracts.
//!
//! Called from:
//! - Managed GMP/XML native codegen jobs and semantic-profile regressions.
//!
//! Key details:
//! - Unsupported concrete values fail in the callee without corrupting caller owners.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Generic object and first-class boundaries preserve original GMP magnitudes and reject plain objects.
#[test]
fn test_strict_object_cast_domains_gmp_native_bundle() {
    if skip_without_gmp_native("test_strict_object_cast_domains_gmp_native_bundle") { return; }
    let source = r#"<?php
function domainGmpObject(object $value): int { return intval($value); }
$cast = domainGmpObject(...);
echo domainGmpObject(gmp_init('9223372036854775807')), '|', $cast(gmp_init('-9223372036854775808')), '|';
$saved = gmp_init('9223372036854775808');
try { $cast($saved); echo 'X'; } catch (ValueError $error) { echo gmp_strval($saved), '|'; }
try { domainGmpObject(new stdClass()); echo 'X'; } catch (TypeError $error) { echo 'T'; }
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_gmp(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true), optimize);
        assert!(output.success, "{optimize}: {:?} {}", output.status, output.stderr);
        assert_eq!(output.stdout, "9223372036854775807|-9223372036854775808|9223372036854775808|T");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    }
}

/// Interface, nullable and union values share native dispatch without admitting ordinary objects.
#[test]
fn test_strict_object_cast_domains_simplexml_native_bundle() {
    if skip_without_xml_native("test_strict_object_cast_domains_simplexml_native_bundle") { return; }
    let source = r#"<?php
interface NativeCastDomainView {}
class DomainViewXml extends SimpleXMLElement implements NativeCastDomainView {
    public function __toString(): string { return '0'; }
}
class DomainViewPlain implements NativeCastDomainView {}
function domainXmlView(NativeCastDomainView $value): float { return (float)$value; }
function domainXmlNullable(?NativeCastDomainView $value): int { return (int)$value; }
function domainXmlUnion(int|NativeCastDomainView $value): int { return (int)$value; }
$value = new DomainViewXml('<n>-7.9</n>');
$cast = domainXmlView(...);
echo $cast($value), '|', domainXmlNullable(null), '|', domainXmlNullable($value), '|', domainXmlUnion(9), '|', domainXmlUnion($value), '|';
try { domainXmlView(new DomainViewPlain()); echo 'X'; } catch (TypeError $error) { echo 'T'; }
try { domainXmlUnion(new DomainViewXml('<n>junk</n>')); echo 'X'; } catch (ValueError $error) { echo 'V'; }
try { domainXmlNullable(new DomainViewXml('<n>9223372036854775808</n>')); echo 'X'; } catch (ValueError $error) { echo 'R'; }
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_xml(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true), optimize);
        assert!(output.success, "{optimize}: {:?} {}", output.status, output.stderr);
        assert_eq!(output.stdout, "-7.9|0|-7|9|-7|TVR");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    }
}

/// Checked native boolean handlers agree across call paths while permissive controls retain existing behavior.
#[test]
fn test_strict_boolval_native_object_cast_parity_bundle() {
    if skip_without_xml_native("test_strict_boolval_native_object_cast_parity_bundle") { return; }
    let source = r#"<?php
class BoolConversionXml extends SimpleXMLElement { public function __toString(): string { return 'override'; } }
function erasedExplicitBool(mixed $value): bool { return boolval($value); }
$cast = boolval(...); $name = $argc > 0 ? 'boolval' : 'BOOLVAL';
foreach (['<n/>', '<n>0</n>', '<n>1</n>', '<n a="v"/>', '<n><x/></n>'] as $xml) {
    $value = new BoolConversionXml($xml);
    echo (bool)$value ? '1' : '0'; echo boolval($value) ? '1' : '0';
    echo erasedExplicitBool($value) ? '1' : '0'; echo $cast($value) ? '1' : '0';
    echo $name($value) ? '1' : '0'; $mapped = array_map('boolval', [$value]);
    echo $mapped[0] ? '1|' : '0|';
}
"#;
    for casts in [false, true] {
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_xml(
                SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, casts), optimize);
            assert!(output.success, "casts={casts} opt={optimize}: {} {}", output.stdout, output.stderr);
            assert_eq!(output.stdout, if casts { "000000|111111|111111|111111|111111|" }
                else { "011111|111111|111111|111111|111111|" });
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        }
    }
}
