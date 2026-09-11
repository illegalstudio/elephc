//! Purpose:
//! Verifies shared mb_split behavior in compiled PHP, opaque eval, and callable dispatch.
//!
//! Called from:
//! - The codegen suite with the managed Oniguruma fixture.
//!
//! Key details:
//! - Fixture output comes from PHP 8.5.10 and preserves binary fields and exact diagnostics.
//! - Retained arrays must survive subsequent calls and copy-on-write mutation.

use crate::support::*;

/// Keeps eval input runtime-dependent while a separate native call selects the managed regex provider.
fn program(source: &str, eval: bool) -> String {
    if !eval { return source.to_owned(); }
    let body = source.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php mb_ereg_search_getpos(); $source = $argc > 0 ? '{body}' : ''; eval($source);")
}

/// Matches PHP for signed limits, captures, encoded fields, Stringable effects, and shared cache invalidation.
#[test]
fn test_mbstring_regex_split_public_calls() {
    let source = include_str!("fixtures/mbstring_split_public.php");
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(source, eval)), include_str!("fixtures/mbstring_split_public.out"), "eval={eval}");
    }
}

/// Preserves diagnostic order and catchable dynamic signature errors before or after subject validation.
#[test]
fn test_mbstring_regex_split_runtime_errors() {
    let source = include_str!("fixtures/mbstring_split_errors.php");
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(source, eval));
        assert!(output.success, "eval={eval}: {}\n{}", output.stderr, output.stdout);
        assert_eq!(output.stdout, include_str!("fixtures/mbstring_split_errors.out"), "eval={eval}");
        assert_eq!(output.stderr, concat!(
            "Warning: mb_split(): mbregex compile err: premature end of char-class\n",
            "Warning: mb_split(): mbregex search failure in mbsplit(): no support in this configuration\n",
            "Deprecated: Implicit conversion from float 2.5 to int loses precision\n",
            "Deprecated: mb_split(): Passing null to parameter #3 ($limit) of type int is deprecated\n"), "eval={eval}");
    }
}

/// Exposes the pinned version through static aliases and opaque eval without requiring a regex call.
#[test]
fn test_mbstring_regex_version_constant() {
    let source = r#"<?php
declare(strict_types=1);
namespace RegexVersion;
use const MB_ONIGURUMA_VERSION as EngineVersion;
echo EngineVersion, ":", \MB_ONIGURUMA_VERSION, "\n";
$code = $argc > 0 ? 'namespace RegexVersion; echo MB_ONIGURUMA_VERSION, ":", constant("MB_ONIGURUMA_VERSION"), ":", defined("MB_ONIGURUMA_VERSION") ? "yes" : "no", "\n";' : '';
eval($code);
"#;
    assert_eq!(compile_and_run(source), "6.9.10:6.9.10\n6.9.10:6.9.10:yes\n");
}

/// Reassigns guarded array-or-false results in loops, branches, and after an early-return guard.
#[test]
fn test_mbstring_regex_reassign_guarded_union() {
    let source = r#"<?php
mb_ereg_search_init("aab", "a");
$result = mb_ereg_search_regs();
$iterations = 0;
while (is_array($result)) {
    echo (string)$result[0];
    $iterations++;
    if ($iterations > 3) { echo "stuck"; break; }
    $result = mb_ereg_search_regs();
}
var_dump($result);
$fields = mb_split(",", "a,b");
if (is_array($fields)) { $fields = mb_split(",", chr(255)); }
var_dump($fields);
function guarded_fields(array|false $fields): void {
    if (!is_array($fields)) { return; }
    $fields = mb_split(",", chr(255));
    var_dump($fields);
}
guarded_fields(mb_split(",", "c,d"));
"#;
    for eval in [false, true] {
        let output = compile_and_run_with_heap_debug(&program(source, eval));
        assert!(output.success, "eval={eval}: {}\n{}", output.stderr, output.stdout);
        assert_eq!(output.stdout, "aabool(false)\nbool(false)\nbool(false)\n", "eval={eval}");
    }
}

/// Keeps field-array copies independent of later splits and mutation under heap debugging.
#[test]
fn test_mbstring_regex_split_array_ownership() {
    let source = r#"<?php
for ($i = 0; $i < 32; $i++) {
    $fields = mb_split(",", "a,b,,c,");
    $copy = $fields;
    if (is_array($fields) && is_array($copy)) {
        $fields[0] = "changed";
        mb_split("b", "another subject");
        echo implode(":", $copy), "\n";
    }
}
"#;
    for eval in [false, true] {
        let output = compile_and_run_with_heap_debug(&program(source, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, "a:b::c:\n".repeat(32), "eval={eval}");
    }
}
