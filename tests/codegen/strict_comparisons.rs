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
        assert_eq!(output.stdout, "Label with a preserved leading zero: 02\nLabel: 10\nLabel: 9\n");
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
