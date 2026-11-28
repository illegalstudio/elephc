//! Purpose:
//! Verifies string contracts at specialized native property update boundaries.
//!
//! Called from:
//! - Focused staged native-view string integration tests.
//!
//! Key details:
//! - Erased receivers exercise runtime native-class dispatch rather than static source rejection.
//! - Explicit formatting remains available before an ordinary property write.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Native XML compound concatenation checks original operands before mutating the document.
#[test]
fn test_strict_string_native_xml_property_update_contracts() {
    if skip_without_xml_native("test_strict_string_native_xml_property_update_contracts") { return; }
    let source = r#"<?php
function appendNativeText(mixed $view, mixed $tail): void { $view->child .= $tail; }
$xml = new SimpleXMLElement('<root><child>a</child></root>');
try { appendNativeText($xml, 7); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
echo (string)$xml->child, '|';
try { appendNativeText($xml, 'x'); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
echo (string)$xml->child, '|';
$xml->child = (string)$xml->child . 'x';
echo (string)$xml->child;
gc_collect_cycles();
"#;
    for selected in [false, true] {
        for optimized in [false, true] {
            let output = StrictFixture::new(source).capture_staged_xml(
                SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected), optimized);
            assert!(output.success, "strict={selected} opt={optimized}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, if selected { "Ta|Ta|ax" } else { "Xa7|Xa7x|a7xx" },
                "strict={selected} opt={optimized}: {}", output.stderr);
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "strict={selected} opt={optimized}: {}", output.stderr);
        }
    }
}

/// Failed dynamic updates preserve absent children while ordinary objects and native errors keep their paths.
fn verify_strict_string_native_xml_dynamic_updates_are_atomic(selected: bool, optimized: bool) {
    if skip_without_xml_native("strict string native XML dynamic update") { return; }
    let source = r#"<?php
function nativeUpdateName(): string { echo 'N'; return 'missing'; }
function nativeUpdateTail(): string { echo 'R'; return 'x'; }
function updateNativeMissing(mixed $view): mixed { return $view->{nativeUpdateName()} .= nativeUpdateTail(); }
function updateNativeNamed(mixed $view, mixed $name, mixed $tail): mixed { return $view->$name .= $tail; }
class OrdinaryStringUpdate { public mixed $value = 'a'; }
$xml = new SimpleXMLElement('<root/>');
try { updateNativeMissing($xml); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
echo ':', count($xml->children()), '|';
try { updateNativeNamed($xml, '', 'x'); echo 'X'; }
catch (ValueError $error) { echo 'V'; unset($error); }
echo '|';
$ordinary = new OrdinaryStringUpdate();
echo updateNativeNamed($ordinary, 'value', 'x'), ':', $ordinary->value;
gc_collect_cycles();
"#;
    let output = StrictFixture::new(source).capture_staged_xml(
        SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected), optimized,
    );
    assert!(output.success, "strict={selected} opt={optimized}: {} {} {}",
        output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, if selected { "NRT:0|V|ax:ax" } else { "NRX:1|V|ax:ax" },
        "strict={selected} opt={optimized}: {}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "strict={selected} opt={optimized}: {}", output.stderr);
}

/// Native dynamic string writes preserve their permissive behavior without optimization.
#[test]
fn test_strict_string_native_xml_dynamic_updates_permissive_unoptimized() {
    verify_strict_string_native_xml_dynamic_updates_are_atomic(false, false);
}

/// Native dynamic string writes preserve their permissive behavior with optimization.
#[test]
fn test_strict_string_native_xml_dynamic_updates_permissive_optimized() {
    verify_strict_string_native_xml_dynamic_updates_are_atomic(false, true);
}

/// Strict native dynamic string writes preserve their atomicity without optimization.
#[test]
fn test_strict_string_native_xml_dynamic_updates_strict_unoptimized() {
    verify_strict_string_native_xml_dynamic_updates_are_atomic(true, false);
}

/// Strict native dynamic string writes preserve their atomicity with optimization.
#[test]
fn test_strict_string_native_xml_dynamic_updates_strict_optimized() {
    verify_strict_string_native_xml_dynamic_updates_are_atomic(true, true);
}
