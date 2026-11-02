//! Purpose:
//! Checks pending return owners when copied finalizers throw or replace the return.
//!
//! Called from:
//! - Runtime GC integration tests and the focused fileinfo CI gates.
//!
//! Key details:
//! - A pending scalar union still owns a Mixed cell before finally execution.
//! - Nested finalizers, foreach cleanup and same-frame catches must retire abandoned owners.

use crate::support::compile_and_run_with_heap_debug;

/// Finalizers transfer successful returns and retire boxed, object and overridden returns.
#[test]
fn test_fileinfo_pending_return_finally_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function pendingReturnCleanup(): void { throw new Error("cleanup"); }
function pendingScalarReturn(): string|false {
    try { return false; } finally { pendingReturnCleanup(); }
}
function pendingObjectReturn(): ?ArrayObject {
    try { return new ArrayObject([1]); } finally { pendingReturnCleanup(); }
}
function pendingNestedReturn(): string|false {
    try {
        try { return false; } finally { pendingReturnCleanup(); }
    } finally { echo "outer\n"; }
}
function pendingOverriddenReturn(): string|false {
    try {
        try { return false; } finally { return "replacement"; }
    } finally { echo "override outer\n"; }
}
function pendingCaughtReturn(): string|false {
    try { return false; } finally {
        try { pendingReturnCleanup(); } catch (Error $error) { echo "inner caught\n"; }
    }
}
function pendingSameFrameReturn(): string|false {
    for ($i = 0; $i < 3; $i++) {
        try {
            try { return false; } finally { pendingReturnCleanup(); }
        } catch (Error $error) { echo "same frame\n"; }
    }
    return "continued";
}
function pendingForeachReturn(): string|false {
    foreach ([new ArrayObject([1]), new ArrayObject([2])] as $item) {
        try { return false; } finally { pendingReturnCleanup(); }
    }
    return "empty";
}
function pendingLocalArrayReturn(): array {
    $local = [1];
    try { return $local; } finally { pendingReturnCleanup(); }
}
function pendingNullableArrayReturn(): ?array {
    $local = [1];
    try { return $local; } finally { pendingReturnCleanup(); }
}
function pendingAssocArrayReturn(): ?array {
    $local = ["value" => 1];
    try { return $local; } finally { pendingReturnCleanup(); }
}
function pendingArrayOverride(): array {
    $local = [1];
    try { return $local; } finally { return [2]; }
}
function pendingArraySameFrame(): array {
    $local = [1];
    for ($i = 0; $i < 3; $i++) {
        try {
            try { return $local; } finally { pendingReturnCleanup(); }
        } catch (Error $error) { echo "array same ", $local[0], "\n"; }
    }
    return $local;
}
for ($i = 0; $i < 2; $i++) {
    try { pendingScalarReturn(); } catch (Error $error) { echo $error->getMessage(), "\n"; }
}
try { pendingObjectReturn(); } catch (Error $error) { echo "object ", $error->getMessage(), "\n"; }
try { pendingNestedReturn(); } catch (Error $error) { echo "nested ", $error->getMessage(), "\n"; }
echo pendingOverriddenReturn(), "\n";
echo var_export(pendingCaughtReturn(), true), "\n";
echo pendingSameFrameReturn(), "\n";
try { pendingForeachReturn(); } catch (Error $error) { echo "foreach ", $error->getMessage(), "\n"; }
try { pendingLocalArrayReturn(); } catch (Error $error) { echo "array ", $error->getMessage(), "\n"; }
try { pendingNullableArrayReturn(); } catch (Error $error) { echo "nullable ", $error->getMessage(), "\n"; }
try { pendingAssocArrayReturn(); } catch (Error $error) { echo "assoc ", $error->getMessage(), "\n"; }
$replacement = pendingArrayOverride(); echo "array replacement ", $replacement[0], "\n";
$continued = pendingArraySameFrame(); echo "array final ", $continued[0], "\n";
unset($replacement, $continued);
unset($error);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "cleanup\ncleanup\nobject cleanup\nouter\nnested cleanup\noverride outer\nreplacement\ninner caught\nfalse\nsame frame\nsame frame\nsame frame\ncontinued\nforeach cleanup\narray cleanup\nnullable cleanup\nassoc cleanup\narray replacement 2\narray same 1\narray same 1\narray same 1\narray final 1\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Pending owner publication remains necessary without EIR optimization passes.
#[test]
fn test_fileinfo_pending_return_finally_owners_without_ir_opt() {
    crate::support::without_ir_opt(test_fileinfo_pending_return_finally_owners);
}
