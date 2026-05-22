//! Purpose:
//! End-to-end tests for built-in SPL class metadata shells.
//! Verifies Phase 4 container names are visible to introspection and interface checks.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - These tests intentionally avoid container mutation semantics; runtime storage lands in later SPL work.

use crate::support::*;

#[test]
fn test_phase4_spl_classes_are_declared_for_introspection() {
    let out = compile_and_run(
        r#"<?php
function has_name(array $names, string $target): bool {
    foreach ($names as $name) {
        if ($name === $target) {
            return true;
        }
    }
    return false;
}

$spl = spl_classes();
echo has_name($spl, "SplDoublyLinkedList");
echo has_name($spl, "SplStack");
echo has_name($spl, "SplQueue");
echo has_name($spl, "SplFixedArray");

$declared = get_declared_classes();
echo has_name($declared, "SplDoublyLinkedList");
echo has_name($declared, "SplStack");
echo has_name($declared, "SplQueue");
echo has_name($declared, "SplFixedArray");

var_dump(class_exists("SplDoublyLinkedList"));
var_dump(class_exists("splstack"));
"#,
    );
    assert_eq!(out, "11111111bool(true)\nbool(true)\n");
}

#[test]
fn test_phase4_spl_class_interface_and_parent_metadata() {
    let out = compile_and_run(
        r#"<?php
$list = new SplDoublyLinkedList();
var_dump($list instanceof Iterator);
var_dump($list instanceof Countable);
var_dump($list instanceof ArrayAccess);

$stack = new SplStack();
var_dump($stack instanceof SplDoublyLinkedList);
var_dump($stack instanceof Iterator);

$queue = new SplQueue();
var_dump($queue instanceof SplDoublyLinkedList);
var_dump($queue instanceof Countable);

$fixed = new SplFixedArray();
var_dump($fixed instanceof IteratorAggregate);
var_dump($fixed instanceof ArrayAccess);
var_dump($fixed instanceof Countable);
var_dump($fixed instanceof JsonSerializable);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

#[test]
fn test_phase4_spl_doubly_linked_list_constants_are_inherited() {
    let out = compile_and_run(
        r#"<?php
echo SplDoublyLinkedList::IT_MODE_LIFO;
echo ",";
echo SplStack::IT_MODE_DELETE;
echo ",";
echo SplQueue::IT_MODE_FIFO;
"#,
    );
    assert_eq!(out, "2,1,0");
}
