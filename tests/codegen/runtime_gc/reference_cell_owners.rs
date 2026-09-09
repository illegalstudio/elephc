//! Purpose:
//! Verifies reference-cell ownership across alias creation, rebinding and retirement.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Heap-backed cells survive their original variable while aliases remain.
//! - Borrowed indexed-element addresses never become independently owned heap allocations.

use crate::support::*;

/// Transitive aliases keep a heap cell alive until its last binding releases the payload.
#[test]
fn test_core_reference_cell_aliases_outlive_original_binding() {
    let source = r#"<?php
class ReferenceCellPayload {
    public int $id = 9;
    public function __destruct() { echo 'd', $this->id, '|'; }
}
function retainedCellAliases(): void {
    $original = [new ReferenceCellPayload()];
    $first = &$original;
    $last = &$first;
    unset($original, $first);
    echo $last[0]->id, '|';
    unset($last);
    echo 'done';
}
retainedCellAliases();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9|d9|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "9|d9|done");
}

/// Rebinding retires exactly the previous owner without making inline element aliases own their parent.
#[test]
fn test_core_reference_cell_rebinding_and_borrowed_element_owners() {
    let source = r#"<?php
function reboundCellAliases(): void {
    $a = ['old'];
    $b = ['new'];
    $alias = &$a;
    $alias = &$a;
    $alias = &$b;
    unset($a, $b);
    echo $alias[0], '|';
    unset($alias);
    $numbers = [3, 4];
    $first = &$numbers[0];
    $second = &$first;
    unset($first);
    $second = 8;
    echo $numbers[0], ':', $numbers[1];
    unset($second, $numbers);
}
reboundCellAliases();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "new|8:4", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "new|8:4");
}
