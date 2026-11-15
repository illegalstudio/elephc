//! Purpose:
//! Verifies that eval warning callbacks return PHP exceptions below Rust frames.
//!
//! Called from:
//! - Focused native codegen tests.
//!
//! Key details:
//! - Native status probes and real opaque eval cover the shared diagnostic boundary.

use crate::support::*;

/// Handled and throwing PHP warning callbacks return versioned native status with balanced owners.
#[test]
fn test_strict_eval_diagnostics_native_warning_status_and_ownership() {
    let source = r#"<?php
extern function __elephc_eval_warning_v2(string $message, int $length): int;
extern function __elephc_eval_value_take_pending_throwable_v2(): ptr;
extern function __elephc_eval_value_release_v3(ptr $cell, ptr $error): int;
extern function malloc(int $size): ptr;
extern function free(ptr $memory): void;
$message = "Warning: protected diagnostic\n";
$error = malloc(8); ptr_set($error, 0);
set_error_handler(function (int $severity, string $message): bool { echo $severity; return true; });
echo ':', __elephc_eval_warning_v2($message, strlen($message));
restore_error_handler();
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('warning callback'); });
echo ':', __elephc_eval_warning_v2($message, strlen($message));
$thrown = __elephc_eval_value_take_pending_throwable_v2();
echo ':', __elephc_eval_value_release_v3($thrown, $error);
restore_error_handler(); free($error);
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(elephc_semantics::SemanticProfile::PERMISSIVE, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, ":20:2:0", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// A warning raised by a real interpreter read reaches the surrounding PHP catch normally.
#[test]
fn test_strict_eval_diagnostics_real_eval_throwing_warning_handler() {
    let output = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('eval warning'); });
$source = $argc > 0 ? 'echo $missingDiagnosticValue;' : '';
try { eval($source); } catch (RuntimeException $error) { echo 'caught'; }
restore_error_handler();
"#);
    assert!(output.success, "{} {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "caught");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Real eval retires callback arguments after handled diagnostics and protected handler throws.
#[test]
fn test_strict_eval_diagnostics_handler_argument_owners_bundle() {
    let source = r#"<?php
$source = $argc > 0 ? '
set_error_handler(function (int $level, string $message): bool { echo $level, "W"; return true; });
trigger_error("first", E_USER_WARNING); echo "|";
trigger_error("second", E_USER_WARNING); echo "|";
restore_error_handler();
set_error_handler(function (): bool { throw new ValueError("stop"); });
$value = "kept";
try { $value = trigger_error("third", E_USER_WARNING); echo "X"; } catch (ValueError $error) { echo "V", $value; }
restore_error_handler();
' : '';
eval($source);
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Bools, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "512W|512W|Vkept", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Prepared and runtime-selected settype calls write original places and preserve COW and catches.
#[test]
fn test_strict_settype_reference_places_bundle() {
    let source = include_str!("../fixtures/strict/settype_places.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "7|S|8|9:9.9|junk|junk:junk|before:laterW|Array|Vcallback:original", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// A native-capable erased receiver keeps ordinary array/property references and XML view semantics.
#[test]
fn test_strict_settype_erased_native_receiver_reference_contexts_bundle() {
    let source = include_str!("../fixtures/strict/settype_erased_receivers.php");    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_xml(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "9:9.9|8:8|7.9|FD|E|10|7.9|FND|NE|N11.9:12", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Known builtin targets retain dynamic property destinations without requiring XML capability.
#[test]
fn test_strict_settype_known_dynamic_property_places_bundle() {
    let source = include_str!("../fixtures/strict/settype_known_dynamic_places.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "13|14|15|16|17.9|17.9|junk|junk", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Checked scalar conversion preserves a typed property, its aliases and later direct writes.
#[test]
fn test_strict_settype_typed_property_contracts_bundle() {
    let source = include_str!("../fixtures/strict/settype_typed_properties.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "18|18|A18|20.9|20|13|17|1|23", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
