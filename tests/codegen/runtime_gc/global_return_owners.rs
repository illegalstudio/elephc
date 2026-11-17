//! Purpose:
//! Checks by-value returns from global Mixed and array storage.
//!
//! Called from:
//! - Runtime GC integration tests on the executable CI target matrix.
//!
//! Key details:
//! - Repeated returns retire safely and returned mutations preserve global COW values.

use crate::support::*;

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
