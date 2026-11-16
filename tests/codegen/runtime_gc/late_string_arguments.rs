//! Purpose:
//! Covers string argument and property-name leases whose storage widens after an earlier use.
//!
//! Called from:
//! - The executable runtime ownership CI suite.
//!
//! Key details:
//! - Alias-capable, scalar-return and non-last-argument calls share exact heap checks.
//! - A concrete-storage control preserves the caller value after each caught exception.

use crate::support::*;

/// Late-widened property names keep one owner across successful stores and caught type refusals.
#[test]
fn test_late_property_name_owners_bundle() {
    let source = r#"<?php
class LatePropertyNameBox { public int $value = 1; }
function latePropertyNameRhs(): int { throw new RuntimeException('rhs'); }
function latePropertyNameStore(bool $reject, bool $throwRhs = false): void {
    $object = new LatePropertyNameBox();
    $name = 'value';
    try { $object->{$name} = $throwRhs ? latePropertyNameRhs() : ($reject ? 'bad' : 3); }
    catch (TypeError $error) { echo 'bad|'; unset($error); }
    catch (RuntimeException $error) { echo 'rhs|'; unset($error); }
    $code = $reject ? '$widened = 1;' : '';
    eval($code);
    echo $name, '|';
    unset($name, $object, $code);
}
for ($i = 0; $i < 3; $i++) {
    latePropertyNameStore(false); latePropertyNameStore(true); latePropertyNameStore(false, true);
}
"#;
    let (output, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    let expected = "value|bad|value|rhs|value|".repeat(3);
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&output), &[&expected]);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

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
