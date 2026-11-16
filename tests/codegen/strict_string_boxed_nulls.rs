//! Purpose:
//! Verifies logical missing values retain their kind through native Mixed boundaries.
//!
//! Called from:
//! - Focused staged string-contract execution tests after direct typed-read coverage.
//!
//! Key details:
//! - Both caller argument boxing and callee return boxing are observable boundaries.
//! - Genuine empty strings stay valid and every program checks heap ownership.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Direct and boxed kind predicates agree while empty strings and colliding integer bits stay valid.
#[test]
fn test_strict_string_logical_null_kind_and_boxing_controls() {
    let source = r#"<?php
function describeLogicalText(mixed $value): void {
    echo is_null($value) ? 'N' : 'V';
    echo is_string($value) ? 'S' : '-';
}
function returnLogicalMissing(int $index): mixed { $parts = ['a']; return $parts[$index]; }
$parts = ['a']; $index = $argc + 3;
echo is_string($parts[$index]) ? 'S' : 'N'; echo '|';
describeLogicalText($parts[$index]); echo '|';
describeLogicalText(returnLogicalMissing($index)); echo '|';
describeLogicalText(''); echo '|';
describeLogicalText(9223372036854775806); echo '|';
echo is_int(9223372036854775806) ? 'I' : 'X';
echo '|', gettype($parts[$index]);
$needle = $argc > 0 ? '' : 'absent';
echo '|', is_string(strstr('', $needle)) ? 'E' : 'X', ':', gettype(strstr(str_repeat('a', $argc), 'a'));
gc_collect_cycles();
"#;
    for selected in [false, true] {
        for ast in [false, true] {
            for ir in [false, true] {
                let output = StrictFixture::new(source).capture_staged_optimizers(
                    SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected), ast, ir);
                assert!(output.success, "strict={selected} ast={ast} ir={ir}: {} {} {}", output.status, output.stdout, output.stderr);
                assert_eq!(output.stdout, "N|N-|N-|VS|V-|I|NULL|E:string",
                    "strict={selected} ast={ast} ir={ir}: {}", output.stderr);
                assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
            }
        }
    }
}

/// A string-array miss cannot become a string merely because a native call boxes it.
#[test]
fn test_strict_string_boxed_nulls_keep_kind_across_native_calls() {
    let source = r#"<?php
function joinBoxedText(mixed $value): string { return $value . 'x'; }
function boxedMissingText(int $index): mixed { $parts = ['a']; return $parts[$index]; }
function joinMissingArgument(int $index): string { $parts = ['a']; return joinBoxedText($parts[$index]); }
try { echo joinMissingArgument($argc + 3); }
catch (TypeError $error) { echo 'T'; unset($error); }
echo '|';
try { echo boxedMissingText($argc + 3) . 'x'; }
catch (TypeError $error) { echo 'T'; unset($error); }
echo '|', joinBoxedText(''), '|', joinBoxedText('a');
gc_collect_cycles();
"#;
    for selected in [false, true] {
        for ast in [false, true] {
            for ir in [false, true] {
                let output = StrictFixture::new(source).capture_staged_optimizers(
                    SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected), ast, ir);
                assert!(output.success, "strict={selected} ast={ast} ir={ir}: {} {} {}", output.status, output.stdout, output.stderr);
                assert_eq!(output.stdout, if selected { "T|T|x|ax" } else { "x|x|x|ax" },
                    "strict={selected} ast={ast} ir={ir}: {}", output.stderr);
                assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
            }
        }
    }
}
