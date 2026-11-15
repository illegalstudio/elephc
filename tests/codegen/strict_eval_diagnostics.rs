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

/// Nullable and scalar-union property conversions preserve declaration types and alias identity.
#[test]
fn test_strict_settype_nullable_property_contracts_bundle() {
    let source = include_str!("../fixtures/strict/settype_nullable_properties.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "N|0|9|A9|18|S|22|19|F|TW|R9", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Object property conversion failures retain original instances, constraints and projection owners.
#[test]
fn test_strict_settype_object_property_contracts_bundle() {
    let source = include_str!("../fixtures/strict/settype_object_properties.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "SC|IS|item|N|ObjectConversionItem|{\"value\":{\"name\":\"item\"},\"optional\":null,\"choice\":\"item\"}W|R", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Array property conversions preserve their container contract and heterogeneous reference replacements.
#[test]
fn test_strict_settype_array_property_contracts_bundle() {
    let source = include_str!("../fixtures/strict/settype_array_properties.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "1,2|A1|3|0W|S:textW|kept|Ptext:kept|C1|Mtext,tail:changed|Vtext,tail|Btext:kept|J{\"values\":[\"text\",\"tail\"],\"named\":{\"other\":\"changed\"},\"optional\":[]}W|Rtext", "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Default-profile mutation reloads canonical Mixed property references and preserves value copies.
#[test]
fn test_managed_mixed_array_mutation_without_strict_bundle() {
    let source = r#"<?php
class ManagedMixedArrayReceiver { public mixed $values = [1]; }
$box = new ManagedMixedArrayReceiver();
$values =& $box->values; $copy = $box->values; $projection = (array)$box;
$push = array_push(...); $push($values, 2);
echo implode(',', $values), '|', implode(',', $copy), '|', implode(',', $box->values), '|', implode(',', $projection['values']);
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&[optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1,2|1|1,2|1,2", "{optimization}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Iterable reference conversions retain their array or Traversable contract and live aliases.
#[test]
fn test_strict_settype_iterable_property_contracts_bundle() {
    let source = include_str!("../fixtures/strict/settype_iterable_properties.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    let expected = format!("{}1,2|T1,2|I3|ArrayIterator|next|next|5|0|R5",
        r#"{"values":[1,2],"optional":null}|"#.repeat(3));
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, expected, "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Raw Iterable metadata renders and hydrates real CLI properties without semantic flags.
#[test]
fn test_raw_iterable_property_metadata_without_strict_bundle() {
    let source = r#"<?php
class PlainIterableProperty { public iterable $values = [1, 2]; public iterable $missing; }
$box = new PlainIterableProperty();
$projection = (array)$box;
echo json_encode($box), '|', json_encode($projection), '|';
$copy = unserialize(serialize($box));
echo json_encode($copy), '|';
$box->values = new ArrayIterator([3]);
$projection = (array)$box;
echo get_class($projection['values']);
class BorrowedIterableProperty {
    public function __construct(public iterable &$values) {}
}
function rawIterablePropertyValues(): iterable { return [4]; }
$source = rawIterablePropertyValues();
$borrowed = new BorrowedIterableProperty($source);
echo '|', json_encode($borrowed), '|', json_encode((array)$borrowed);
"#;
    let expected = format!(r#"{}ArrayIterator|{{"values":[4]}}|{{"values":[4]}}"#, r#"{"values":[1,2]}|"#.repeat(3));
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&[optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{optimization}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Boxed static references validate assignments inside the callee and preserve argument-time selection.
#[test]
fn test_strict_settype_static_boxed_property_contracts_bundle() {
    let source = include_str!("../fixtures/strict/settype_static_boxed_properties.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "7|18|integer:8|A8WP|textW|RtextWP|nextW|TnextDB|S27DB|L31", "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Eval exports share the native static binding while retaining independent wrapper owners.
#[test]
fn test_strict_static_reference_exports_preserve_identity_and_constraints_bundle() {
    let source = include_str!("../fixtures/strict/static_reference_exports.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "I:7:1:7:19:19:8:R8:guarded:0:23:P:0ADB:2:0:8", "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// The versioned replacement ABI preserves the ordinary unconstrained displaced-owner contract.
#[test]
fn test_eval_reference_replacement_permissive_boundary_bundle() {
    let source = r#"<?php
extern class ReplacementCell { public int $tag; public int $value; public int $high; }
extern class ReplacementOutput { public ptr $cell; }
extern function __elephc_eval_value_int(int $value): ptr;
extern function __elephc_eval_value_reference_new(ptr $value): ptr;
extern function __elephc_eval_value_reference_replace_v2(ptr $reference, ptr $value, int $flags, ptr $previous, ptr $error): int;
extern function __elephc_eval_value_release_v3(ptr $value, ptr $error): int;
extern function malloc(int $size): ptr;
extern function free(ptr $value): void;
$previous = malloc(8); $error = malloc(8); ptr_set($error, 0);
$initial = __elephc_eval_value_int(1);
$reference = __elephc_eval_value_reference_new($initial);
$next = __elephc_eval_value_int(2);
echo __elephc_eval_value_reference_replace_v2($reference, $next, 0, $previous, $error);
$stored = ptr_cast<ReplacementOutput>($previous);
$old = ptr_cast<ReplacementCell>($stored->cell); echo ':', $old->tag, ':', $old->value;
__elephc_eval_value_release_v3($stored->cell, $error);
__elephc_eval_value_release_v3($reference, $error);
__elephc_eval_value_release_v3($initial, $error);
__elephc_eval_value_release_v3($next, $error);
free($previous); free($error);
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(
            elephc_semantics::SemanticProfile::PERMISSIVE, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "0:0:1", "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Concrete static declarations retain checked live references without losing their logical types.
#[test]
fn test_strict_settype_static_typed_property_contracts_bundle() {
    let source = include_str!("../fixtures/strict/settype_static_typed_properties.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "integer:7|A7|double:8|string:19|B|N|12|OstdClass|2,3|I4WP|25W|R25|31:31|19|U",
            "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Unbound static writes coerce declarations and publish initialization before cleanup reentry.
#[test]
fn test_strict_static_write_publication_and_initialization_bundle() {
    let source = include_str!("../fixtures/strict/static_write_publication.php");
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Casts, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "double:7|T7|U|textDtext|EF|N:inner|outerOouter",
            "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
