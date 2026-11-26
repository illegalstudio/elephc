//! Purpose:
//! Checks by-value returns from global Mixed and array storage.
//!
//! Called from:
//! - Runtime GC integration tests on the executable CI target matrix.
//!
//! Key details:
//! - Repeated returns retire safely and returned mutations preserve global COW values.

use crate::support::*;

/// Local aliases and ordinary global stores observe the same cell and keep value snapshots.
#[test]
fn test_named_global_alias_values_and_snapshots() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$shared = json_decode('{"old":[1]}', true);
function replaceShared(mixed $value): void {global $shared; $shared=$value;}
function aliasShared(mixed $value): array {
    global $shared;
    $alias =& $shared;
    $before=$alias;
    $alias=$value;
    $after=$shared;
    replaceShared(['rebound'=>[2]]);
    return [$before,$after,$alias,$shared];
}
$result=aliasShared(['write'=>[3]]);
echo json_encode($result);
unset($result,$shared);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, r#"[{"old":[1]},{"write":[3]},{"rebound":[2]},{"rebound":[2]}]"#);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Captured aliases keep their wrapper identity when an ordinary global setter replaces values.
#[test]
fn test_named_global_alias_closure_capture_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$shared = json_decode('{"old":[1]}', true);
$snapshot=$shared;
function replaceCapturedGlobal(mixed $value): void {global $shared; $shared=$value;}
function captureGlobalAlias(): Closure {
    global $shared;
    $alias =& $shared;
    return function (mixed $value) use (&$alias): mixed {$before=$alias; $alias=$value; return $before;};
}
$write=captureGlobalAlias();
replaceCapturedGlobal(['external'=>2]);
$before=$write(['inside'=>3]);
echo json_encode([$snapshot,$before,$shared]);
unset($write,$snapshot,$before,$shared);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, r#"[{"old":[1]},{"external":2},{"inside":3}]"#);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Both aliased and ordinary stores publish replacements before retiring user objects.
#[test]
fn test_named_global_alias_destructor_publication() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class GlobalAliasRetirement {
    public function __destruct() {global $shared; echo is_array($shared) ? 'published|' : 'old|';}
}
function replaceGlobalAlias(mixed $value): void {global $shared; $alias =& $shared; $alias=$value;}
function replaceGlobalDirect(mixed $value): void {global $shared; $shared=$value;}
$shared=new GlobalAliasRetirement();
replaceGlobalAlias(['first']);
$shared=new GlobalAliasRetirement();
replaceGlobalDirect(['second']);
unset($shared);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "published|published|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A direct caller owns its returned cell without borrowing or mutating the global owner.
#[test]
fn test_global_value_return_owners_and_copy_semantics() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$shared = json_decode('[1,2]', true);
$rows = [3, 4];
function returnOwnedGlobalMixed(): mixed { global $shared; return $shared; }
function returnOwnedGlobalRows(): array { global $rows; return $rows; }
for ($i = 0; $i < 3; $i++) {
    $copy = returnOwnedGlobalMixed(); $rowCopy = returnOwnedGlobalRows();
    unset($copy, $rowCopy);
}
$copy = returnOwnedGlobalMixed(); $copy[] = 9;
$rowCopy = returnOwnedGlobalRows(); $rowCopy[] = 8;
echo json_encode($shared), ':', json_encode($copy), '|';
echo json_encode($rows), ':', json_encode($rowCopy), '|';
unset($shared, $rows, $copy, $rowCopy);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[1,2]:[1,2,9]|[3,4]:[3,4,8]|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
