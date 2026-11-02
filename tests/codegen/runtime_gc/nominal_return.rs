//! Purpose: verify ownership across nullable nominal return guards.
//! Called from: the runtime GC integration suite.
//! Key details: guards must preserve the existing return ownership convention.

use crate::support::*;

#[test]
fn test_nullable_object_array_coalesce_assignment_condition() {
    let out = compile_and_run(r#"<?php
class ConditionalObject {
    public function label(): string { return 'set'; }
}
function candidates(bool $empty): array {
    if ($empty) { return []; }
    return [new ConditionalObject()];
}
function inspectCandidate(bool $empty): string {
    $probe = candidates($empty)[0] ?? null;
    $prefix = ((bool) $probe ? 'true' : 'false').'/'.(empty($probe) ? 'empty' : 'full').'/';
    if (!$candidate = candidates($empty)[0] ?? null) {
        return $prefix.'none';
    }
    return $prefix.$candidate->label();
}
echo inspectCandidate($argc === 1), ':', inspectCandidate($argc !== 1);
"#);
    assert_eq!(out, "false/empty/none:true/full/set");
}

#[test]
fn test_nullable_nominal_return_borrowed_mixed_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class NominalReturnValue {}
function narrow(mixed $value, int $depth): ?NominalReturnValue {
    if ($depth > 0) { return narrow($value, $depth - 1); }
    return $value;
}
function exercise(): void {
    $value = narrow(new NominalReturnValue(), 2);
    echo $value instanceof NominalReturnValue ? "object" : "bad";
    unset($value);
    echo narrow(null, 2) === null ? ":null" : ":bad";
}
exercise();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "object:null");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

#[test]
fn test_nullable_nominal_return_owned_mixed_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class NominalReturnValue {}
function makeValue(): mixed { return new NominalReturnValue(); }
function narrow(int $depth): ?NominalReturnValue {
    if ($depth > 0) { return narrow($depth - 1); }
    return makeValue();
}
function exercise(): void {
    $value = narrow(2);
    echo $value instanceof NominalReturnValue ? "object" : "bad";
    unset($value);
}
exercise();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "object");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

#[test]
fn test_nullable_nominal_return_reassigned_mixed_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class NominalReturnValue {}
function makeValue(): mixed { return new NominalReturnValue(); }
function narrow(mixed $value, int $depth): ?NominalReturnValue {
    if ($depth > 0) { return narrow($value, $depth - 1); }
    $value = makeValue();
    return $value;
}
function exercise(): void {
    $value = narrow(null, 2);
    echo $value instanceof NominalReturnValue ? "object" : "bad";
    unset($value);
}
exercise();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "object");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
