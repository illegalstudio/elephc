//! Purpose:
//! Covers copy-on-write and removed-slot ownership during boxed and concrete array splices.
//!
//! Called from:
//! - The runtime GC codegen integration module.
//!
//! Key details:
//! - Removed pointers transfer ownership instead of being retained after leaving the source.

use crate::support::*;

/// Separating a reference receiver protects its aliases and leaves removed heap values owned once.
#[test]
fn test_boxed_array_splice_removals_and_growth_preserve_aliases() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function spliceValues(array &$values): array { return array_splice($values, 1, 2, ["p", "q", "r", "s"]); }
$values = [str_repeat("a", 3), str_repeat("b", 3), str_repeat("c", 3), str_repeat("d", 3)];
$alias = $values;
$removed = spliceValues($values);
echo implode(",", $values), "|", implode(",", $alias), "|", implode(",", $removed), "|";
unset($values, $alias);
echo $removed[0], ":", $removed[1];
unset($removed);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "aaa,p,q,r,s,ddd|aaa,bbb,ccc,ddd|bbb,ccc|bbb:ccc", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Concrete object slots leave the source without an extra retained owner or a premature destructor.
#[test]
fn test_array_splice_refcounted_transfers_multiple_object_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SpliceOwner {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function __destruct() { echo "drop:", $this->id, "|"; }
}
$values = [new SpliceOwner(1), new SpliceOwner(2), new SpliceOwner(3), new SpliceOwner(4)];
$removed = array_splice($values, 1, 2);
echo count($values), ":", count($removed), ":", $removed[0]->id, ":", $removed[1]->id, "|";
unset($removed);
echo "rest|";
unset($values);
echo "done";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:2:2:3|drop:2|drop:3|rest|drop:1|drop:4|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Empty and full removals preserve payload tags without invoking any per-element retain loop.
#[test]
fn test_boxed_array_splice_empty_and_full_removal_ownership() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function removeRange(array &$values, int $offset, int $length): array { return array_splice($values, $offset, $length); }
$values = [[str_repeat("x", 2)], [str_repeat("y", 2)]];
echo count(removeRange($values, 1, 0)), ":", count($values), "|";
$removed = removeRange($values, 0, 99);
echo count($values), ":", $removed[0][0], ":", $removed[1][0];
unset($values, $removed);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0:2|0:xx:yy", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
