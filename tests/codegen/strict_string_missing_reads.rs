//! Purpose:
//! Verifies that strict concatenation observes missing PHP values behind typed storage.
//!
//! Called from:
//! - Focused staged string-contract execution tests.
//!
//! Key details:
//! - Runtime-dependent indices retain typed string-array reads through optimization.
//! - Output formatting and explicit casts remain valid intentional null formatting.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// A typed string-array miss must retain its original null kind until concatenation checks it.
#[test]
fn test_strict_string_missing_typed_array_reads_keep_original_kind() {
    let source = r#"<?php
function typedStringMiss(int $index): string {
    $parts = ['a'];
    return $parts[$index] . 'x';
}
function formatStringMiss(int $index): string {
    $parts = ['a'];
    $part = $parts[$index];
    return "[$part]";
}
function castStringMiss(int $index): string {
    $parts = ['a'];
    return (string)$parts[$index] . 'x';
}
try { echo typedStringMiss($argc + 3); }
catch (TypeError $error) { echo 'T'; unset($error); }
echo '|', typedStringMiss(0), '|', formatStringMiss($argc + 3), '|', castStringMiss($argc + 3);
gc_collect_cycles();
"#;
    for selected in [false, true] {
        for ast in [false, true] {
            for ir in [false, true] {
                let output = StrictFixture::new(source).capture_staged_optimizers(
                    SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected), ast, ir);
                assert!(output.success, "strict={selected} ast={ast} ir={ir}: {} {} {}", output.status, output.stdout, output.stderr);
                assert_eq!(output.stdout, if selected { "T|ax|[]|x" } else { "x|ax|[]|x" },
                    "strict={selected} ast={ast} ir={ir}: {}", output.stderr);
                assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
            }
        }
    }
}
