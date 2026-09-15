//! Purpose:
//! Covers boxed array flipping across callback, key normalization and ownership boundaries.
//!
//! Called from:
//! - The runtime GC codegen integration module.
//!
//! Key details:
//! - Warning handlers can replace the source, reenter the builtin or throw.
//! - Every fixture checks the resulting values and the final heap balance.

use crate::support::*;

/// Logical tags preserve duplicate ordering, numeric-string keys and independently owned strings.
#[test]
fn test_boxed_array_flip_mixed_keys_values_and_growth() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function flip(array $values): array { return \ArRaY_FlIp($values); }
$source = ["first" => 8, 4 => "8", "next" => str_repeat("n", 2)];
$result = flip($source);
unset($source);
echo count($result), ":", $result[8], ":", $result["nn"], "|";
$many = [];
for ($i = 0; $i < 40; $i++) { $many[] = $i + 100; }
$flipped = flip($many);
echo count($flipped), ":", $flipped[100], ":", $flipped[139], "|";
echo count(flip([]));
unset($result, $many, $flipped);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:4:next|40:0:39|0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A handler replacing the caller's source does not invalidate the retained iteration snapshot.
#[test]
fn test_boxed_array_flip_warning_reentrancy_and_source_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function flip(array $values): array { return array_flip($values); }
function flipWarningSource(): array {
    return ["before" => 7, "invalid" => false, "after" => str_repeat("z", 3)];
}
$source = flipWarningSource();
set_error_handler(function (int $level, string $message) use (&$source): bool {
    echo $level, ":", str_contains($message, "Can only flip") ? "warning" : "bad", "|";
    $source = ["changed"];
    echo flip(["inner" => 9])[9], "|";
    return true;
});
$result = flip($source);
restore_error_handler();
echo $source[0], "|", $result[7], ":", $result["zzz"];
unset($source, $result);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:warning|inner|changed|before:after", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Throws retire both the partial flipped hash and the retained source before reaching catch.
#[test]
fn test_boxed_array_flip_warning_throw_releases_partial_result() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function source(): array { return [str_repeat("k", 4) => 5, "bad" => null, "last" => "value"]; }
function flip(array $values): array { return array_flip($values); }
set_error_handler(function (int $level, string $message): bool { throw new RuntimeException("flip"); });
for ($i = 0; $i < 3; $i++) {
    try { flip(source()); } catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
}
restore_error_handler();
unset($error);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "flip|flip|flip|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Discarded calls still dispatch warnings for bool, float, null, object and nested array entries.
#[test]
fn test_boxed_array_flip_discarded_call_keeps_warning_effects() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function source(): array { return [1, false, 1.5, null, new stdClass(), ["nested"]]; }
$warnings = 0;
set_error_handler(function (int $level, string $message) use (&$warnings): bool { $warnings++; return true; });
array_flip(source());
restore_error_handler();
echo $warnings;
unset($warnings);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "5", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
