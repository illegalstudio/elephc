use crate::support::*;

// ----- SplStack -----------------------------------------------------------

#[test]
fn test_spl_stack_push_pop_lifo_order() {
    let out = compile_and_run(
        r#"<?php
$s = new SplStack();
$s->push("a");
$s->push("b");
$s->push("c");
echo $s->pop();
echo $s->pop();
echo $s->pop();
"#,
    );
    assert_eq!(out, "cba");
}

#[test]
fn test_spl_stack_top_does_not_remove() {
    let out = compile_and_run(
        r#"<?php
$s = new SplStack();
$s->push(1);
$s->push(2);
echo $s->top();
echo $s->count();
"#,
    );
    assert_eq!(out, "22");
}

#[test]
fn test_spl_stack_count_and_is_empty() {
    let out = compile_and_run(
        r#"<?php
$s = new SplStack();
echo $s->isEmpty() ? "y" : "n";
echo $s->count();
$s->push("x");
echo $s->isEmpty() ? "y" : "n";
echo $s->count();
"#,
    );
    assert_eq!(out, "y0n1");
}

// ----- SplQueue -----------------------------------------------------------

#[test]
fn test_spl_queue_enqueue_dequeue_fifo_order() {
    let out = compile_and_run(
        r#"<?php
$q = new SplQueue();
$q->enqueue("a");
$q->enqueue("b");
$q->enqueue("c");
echo $q->dequeue();
echo $q->dequeue();
echo $q->dequeue();
"#,
    );
    assert_eq!(out, "abc");
}

#[test]
fn test_spl_queue_count_after_enqueue() {
    let out = compile_and_run(
        r#"<?php
$q = new SplQueue();
$q->enqueue(1);
$q->enqueue(2);
$q->enqueue(3);
echo $q->count();
echo $q->dequeue();
echo $q->count();
"#,
    );
    assert_eq!(out, "312");
}

// ----- SplDoublyLinkedList -----------------------------------------------

#[test]
fn test_spl_dll_push_pop_and_top_bottom() {
    let out = compile_and_run(
        r#"<?php
$d = new SplDoublyLinkedList();
$d->push("x");
$d->push("y");
$d->push("z");
echo $d->bottom();
echo $d->top();
echo $d->pop();
echo $d->top();
"#,
    );
    assert_eq!(out, "xzzy");
}

#[test]
fn test_spl_dll_unshift_and_shift() {
    let out = compile_and_run(
        r#"<?php
$d = new SplDoublyLinkedList();
$d->push("b");
$d->unshift("a");
$d->push("c");
echo $d->shift();
echo $d->shift();
echo $d->shift();
"#,
    );
    assert_eq!(out, "abc");
}

#[test]
fn test_spl_dll_count_isempty_after_drain() {
    let out = compile_and_run(
        r#"<?php
$d = new SplDoublyLinkedList();
$d->push(1);
$d->push(2);
echo $d->count();
$d->pop();
$d->pop();
echo $d->count();
echo $d->isEmpty() ? "y" : "n";
"#,
    );
    assert_eq!(out, "20y");
}

// ----- SplFixedArray -----------------------------------------------------

#[test]
fn test_spl_fixed_array_size_and_offset_set_get() {
    let out = compile_and_run(
        r#"<?php
$fa = new SplFixedArray(3);
$fa->offsetSet(0, "a");
$fa->offsetSet(1, "b");
$fa->offsetSet(2, "c");
echo $fa->getSize();
echo $fa->offsetGet(0);
echo $fa->offsetGet(1);
echo $fa->offsetGet(2);
"#,
    );
    assert_eq!(out, "3abc");
}

#[test]
fn test_spl_fixed_array_count_matches_size() {
    let out = compile_and_run(
        r#"<?php
$fa = new SplFixedArray(5);
echo $fa->count();
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_spl_fixed_array_offset_exists_bounds() {
    let out = compile_and_run(
        r#"<?php
$fa = new SplFixedArray(3);
echo $fa->offsetExists(0) ? "y" : "n";
echo $fa->offsetExists(2) ? "y" : "n";
echo $fa->offsetExists(3) ? "y" : "n";
echo $fa->offsetExists(-1) ? "y" : "n";
"#,
    );
    assert_eq!(out, "yynn");
}

#[test]
fn test_spl_fixed_array_out_of_range_throws_runtime_exception() {
    let out = compile_and_run(
        r#"<?php
$fa = new SplFixedArray(2);
try {
    $fa->offsetGet(5);
} catch (RuntimeException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "SplFixedArray index out of range");
}

#[test]
fn test_spl_fixed_array_offset_unset_resets_to_null() {
    let out = compile_and_run(
        r#"<?php
$fa = new SplFixedArray(2);
$fa->offsetSet(0, "kept");
$fa->offsetSet(1, "dropped");
$fa->offsetUnset(1);
echo $fa->offsetGet(0);
echo "|";
echo $fa->offsetGet(1) === null ? "null" : "not-null";
"#,
    );
    assert_eq!(out, "kept|null");
}

// ----- Cross-class regression --------------------------------------------

#[test]
fn test_data_structures_coexist_in_same_program() {
    let out = compile_and_run(
        r#"<?php
$s = new SplStack();
$q = new SplQueue();
$d = new SplDoublyLinkedList();
$f = new SplFixedArray(2);
$s->push("S");
$q->enqueue("Q");
$d->push("D");
$f->offsetSet(0, "F");
echo $s->pop();
echo $q->dequeue();
echo $d->pop();
echo $f->offsetGet(0);
"#,
    );
    assert_eq!(out, "SQDF");
}

#[test]
fn test_data_structures_namespaced_user_code() {
    // The synthesised classes must resolve to the global namespace even
    // when the user has their own `namespace App;` declaration.
    let out = compile_and_run(
        r#"<?php
namespace App;
$s = new \SplStack();
$s->push("ok");
echo $s->pop();
"#,
    );
    assert_eq!(out, "ok");
}
