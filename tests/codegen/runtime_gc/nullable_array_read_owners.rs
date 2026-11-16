//! Purpose:
//! Ownership regressions for nullable array reads whose offset evaluation throws.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - An owning boxed null receiver must be retired on the offset's exception path.
//! - Correct PHP output and a clean heap are separate assertions.

/// A throwing offset still runs once and leaves no receiver owner behind on the null path.
#[test]
fn test_nullable_array_read_throwing_offset_releases_receiver() {
    let out = crate::support::compile_and_run_with_heap_debug(
        r#"<?php
function absent(): ?array { return null; }
function throw_offset(): int { throw new Exception("offset"); }
function run_case(): void {
    $caught = 0;
    for ($i = 0; $i < 40; $i++) {
        try { $value = absent()[throw_offset()]; }
        catch (Exception $error) { $caught++; unset($error); }
    }
    echo $caught, "\n";
}
run_case();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "40\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Canonical null receivers preserve quiet key effects and cleanup when warning delivery throws.
#[test]
fn test_nullable_array_read_throwing_warning_releases_receiver() {
    let out = crate::support::compile_and_run_with_heap_debug(r#"<?php
function absent(): ?array { return null; }
function reject_warning(int $level, string $message): bool { throw new Exception("warning"); }
function run_case(): void {
    set_error_handler("reject_warning");
    $caught = 0;
    for ($i = 0; $i < 40; $i++) {
        try { $value = absent()[0]; }
        catch (Exception $error) { $caught++; unset($error); }
    }
    restore_error_handler();
    echo $caught, "\n";
}
run_case();
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "40\n");
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
