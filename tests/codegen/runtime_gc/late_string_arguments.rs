//! Purpose:
//! Covers string argument leases whose caller storage widens after an earlier call.
//!
//! Called from:
//! - The executable runtime ownership CI suite.
//!
//! Key details:
//! - Alias-capable, scalar-return and non-last-argument calls share exact heap checks.
//! - A concrete-storage control preserves the caller value after each caught exception.

use crate::support::*;

const SOURCE: &str = r#"<?php
function lateLeaseAlias(string $value): string {
    if ($value !== '') { throw new RuntimeException('alias'); }
    return $value;
}
function lateLeaseFresh(string $value): int {
    if ($value !== '') { throw new RuntimeException('fresh'); }
    return 0;
}
function lateLeasePair(string $value, int $tail): string {
    if ($value !== '') { throw new RuntimeException('pair' . $tail); }
    return $value;
}
$text = str_repeat('u', 72);
if ($argc === 1) { try { lateLeaseAlias($text); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; } }
elseif ($argc === 2) { try { lateLeaseFresh($text); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; } }
else { try { lateLeasePair($text, 1); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; } }
unset($error);
$code = $argc > 9 ? 'echo "never";' : '';
eval($code);
echo strlen($text), '|';
unset($text, $code);
"#;

/// Every argument position retires its cast copy when a later eval widens the caller slot.
#[test]
fn test_late_widened_string_argument_unwind_bundle() {
    assert_string_argument_leases(SOURCE);
}

/// Borrowed concrete strings remain owned by the caller throughout the catch body.
#[test]
fn test_concrete_string_argument_unwind_bundle() {
    assert_string_argument_leases(&SOURCE.replace("eval($code);", ""));
}

/// Compiles each storage configuration once and checks all three call shapes independently.
fn assert_string_argument_leases(source: &str) {
    let runs = compile_and_run_with_heap_debug_per_argv(source, &[&[], &["x"], &["x", "x"]]);
    assert_eq!(runs.len(), 3);
    for (out, expected) in runs.iter().zip(["alias|72|", "fresh|72|", "pair1|72|"]) {
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}
