//! Purpose:
//! Executes exact scalar ordering and switch matching through the native backend.
//!
//! Called from:
//! - Focused staged comparison integration tests.
//!
//! Key details:
//! - Both optimizer modes must retain catchable guards, source effects and clean heaps.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Shared entry and result ownership remains balanced in the accepted permissive runtime.
#[test]
fn test_strict_comparison_opaque_permissive_sort_owners() {
    let source = include_str!("../fixtures/strict/comparator_sort_owners.php");
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(
            SemanticProfile::PERMISSIVE, optimize);
        assert!(output.success, "opt={optimize}: {} {}", output.stdout, output.stderr);
        assert_eq!(output.stdout, "1,2;1,2;1,2;label-1,label-2", "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "opt={optimize}: {}", output.stderr);
    }
}

/// Isolates accepted and rejected search argument owners, including a permissive baseline.
#[test]
fn test_strict_comparison_native_search_owner_paths() {
    let mut failures = Vec::new();
    for selected in [false, true] {
        for (label, source) in [
            ("search strings", "<?php echo array_search('1', ['0', '1'], true); gc_collect_cycles();"),
            ("membership strings", "<?php echo in_array('1', ['0', '1'], true); gc_collect_cycles();"),
            ("failed direct", "<?php function ownerMode(): bool { return false; } try { in_array(1, [1, '1'], ownerMode()); } catch (TypeError $error) { unset($error); } gc_collect_cycles();"),
            ("failed descriptor", "<?php function ownerMode(): bool { return false; } $search = in_array(...); try { $search(1, [1, '1'], ownerMode()); } catch (TypeError $error) { unset($error); } try { $search(1, [1, '1']); } catch (TypeError $error) { unset($error); } gc_collect_cycles();"),
        ] {
            let output = StrictFixture::new(source).capture_staged_optimizers(
                SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, selected), false, false);
            if !output.success || !output.stderr.contains("HEAP DEBUG: leak summary: clean") {
                failures.push(format!("{label}, strict={selected}: {} {}", output.stdout, output.stderr));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Builtin mode failures retain argument effects, first-class calls and temporary ownership.
#[test]
fn test_strict_comparison_native_builtin_modes_and_effects_bundle() {
    let source = r#"<?php
function strictComparisonFalse(int $count): bool { return $count < 0; }
function strictComparisonNeedle(): int { echo 'A'; return 1; }
function strictComparisonHaystack(): array { echo 'B'; return [1, '1']; }
function strictComparisonMode(): bool { echo 'C'; return false; }
class StrictComparisonOwnedNeedle { public function __destruct() { echo 'D'; } }
try { in_array(strictComparisonNeedle(), strictComparisonHaystack(), strictComparisonMode()); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
try { in_array(strict: strictComparisonMode(), needle: strictComparisonNeedle(), haystack: strictComparisonHaystack()); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
try { in_array(new StrictComparisonOwnedNeedle(), [1], strictComparisonFalse($argc)); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
$search = in_array(...);
try { $search(1, [1], strictComparisonFalse($argc)); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
try { $search(1, [1]); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
try { $search(1, [1], strictComparisonFalse($argc)); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
$freshSearch = in_array(...);
try { $freshSearch(1, [1]); echo 'X'; }
catch (TypeError $error) { echo 'T'; unset($error); }
$automatic = array_unique(...);
try { $automatic([1, 1]); echo 'X'; }
catch (TypeError $error) { echo 'U'; unset($error); }
echo '|', in_array(1, ['1'], true) ? 'X' : 'N', $search(1, [1], true) ? 'Y' : 'X';
echo array_search('1', ['0', '1'], true);
gc_collect_cycles();
"#;
    for (ast, ir) in [(false, false), (true, false), (false, true), (true, true)] {
        let output = StrictFixture::new(source).capture_staged_optimizers(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true), ast, ir);
        assert!(output.success, "ast={ast} ir={ir}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "ABCTCABTDTTTTTU|NY1", "ast={ast} ir={ir}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "ast={ast} ir={ir}: {}", output.stderr);
    }
}

/// Temporary switch subjects and labels retire before case bodies without losing live-variable effects.
#[test]
fn test_strict_comparison_native_switch_temporary_subjects_and_labels() {
    let source = r#"<?php
class StrictSwitchTemp { public function __destruct() { echo 'D'; } }
function strictSwitchOwnedSubject(): StrictSwitchTemp { return new StrictSwitchTemp(); }
function strictSwitchOwnedText(string $suffix): string { return 'subject-' . $suffix; }
function strictSwitchInitialValue(): mixed { return 1; }
switch (strictSwitchOwnedSubject()) {
    case new StrictSwitchTemp(): echo 'X'; break;
    default: echo 'B';
}
echo '|';
switch (strictSwitchOwnedText(strval($argc))) { case 'never': echo 'X'; break; default: echo 'S'; }
$value = strictSwitchInitialValue();
switch ($value) { case ($value = '2'): echo 'L'; break; default: echo 'X'; }
gc_collect_cycles();
"#;
    for (ast, ir) in [(false, false), (true, false), (false, true), (true, true)] {
        let output = StrictFixture::new(source).capture_staged_optimizers(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true), ast, ir);
        assert!(output.success, "ast={ast} ir={ir}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "DDB|SL", "ast={ast} ir={ir}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "ast={ast} ir={ir}: {}", output.stderr);
    }
}

/// The runnable label example sorts numeric-looking text through an explicit user comparator.
#[test]
fn test_strict_comparison_native_label_example() {
    let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/strict-comparisons/main.php")).unwrap();
    for optimize in [false, true] {
        let output = StrictFixture::new(&source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "Label with a preserved leading zero: 02\nLabel: 10\nLabel: 9\nExact label found: 02\n");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Original-kind guards precede conversion hooks and compare numeric-looking strings as bytes.
#[test]
fn test_strict_comparison_native_scalar_order_switch_and_owners_bundle() {
    let source = r#"<?php
function strictPair(mixed $left, mixed $right): void {
    echo $left <=> $right, '/', $left < $right ? 1 : 0, '/', $left >= $right ? 1 : 0, ';';
}
function strictLess(mixed $left, mixed $right): void { echo $left < $right ? 'X' : 'X'; }
function strictDiscard(mixed $left, mixed $right): void { $left < $right; echo 'X'; }
function strictLater(): string { echo 'E'; return '9'; }
function strictOwned(): string { return 'owned-' . $GLOBALS['argc']; }
function strictMutate(mixed &$value): string { $value = '9'; return '9'; }
function strictSnapshot(mixed $value): void {
    $alias =& $value;
    try { $alias < strictMutate($value); echo 'X'; } catch (TypeError $error) { echo 'T'; unset($error); }
    echo $value;
}
function strictSwitch(mixed $value): void {
    switch ($value) { case '1': echo 'S'; break; case 1: echo 'I'; break; case false: echo 'f'; break; default: echo 'D'; }
}
class StrictComparisonText { public function __toString(): string { echo 'X'; return '9'; } }
echo '10' < '9' ? 'L' : 'X', '10' <=> '9', ';';
strictPair('10', '9'); strictPair('02', '2'); strictPair("a\0b", "a\0c");
strictPair(2, 3); strictPair(2.0, 3.0); strictPair(true, false); strictPair(null, null);
try { strictLess(1, 2.0); } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictLess(true, 1); } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictLess('1', 1); } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictLess(new StrictComparisonText(), '9'); } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictLess(strictOwned(), 1); } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictLess(1, strictLater()); } catch (TypeError $error) { echo 'T'; unset($error); }
try { strictDiscard(1, '9'); } catch (TypeError $error) { echo 'T'; unset($error); }
strictSnapshot(7);
echo '|'; strictSwitch('1'); strictSwitch(1); strictSwitch(0); strictSwitch(false); strictSwitch(null);
echo NAN < NAN ? 'X' : 'N', NAN >= NAN ? 'X' : 'N', NAN <=> NAN;
gc_collect_cycles();
"#;
    let expected = format!("L-1;{}1/0/1;0/0/1;TTTTTETTT9|SIDfDNN1", "-1/1/0;".repeat(5));
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, expected, "opt={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
