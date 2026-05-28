//! Purpose:
//! End-to-end tests for SPL Phase 6 heap, priority queue, and object storage classes.
//! Covers declarations, ordering behavior, ArrayAccess, iteration, and per-instance cleanup.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - Heap and priority-queue iterators are destructive, matching PHP SPL behavior.
//! - Heap-debug coverage verifies property-backed handles are finalized through object cleanup.

use crate::support::*;

/// Verifies that Phase 6 SPL classes are declared and implement their core interfaces.
#[test]
fn test_phase6_spl_classes_are_declared_and_typed() {
    let out = compile_and_run(
        r#"<?php
var_dump(class_exists("SplHeap"));
var_dump(class_exists("SplMaxHeap"));
var_dump(class_exists("SplMinHeap"));
var_dump(class_exists("SplPriorityQueue"));
var_dump(class_exists("SplObjectStorage"));
var_dump(new SplMaxHeap() instanceof Iterator);
var_dump(new SplPriorityQueue() instanceof Countable);
var_dump(new SplObjectStorage() instanceof ArrayAccess);
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
        )
    );
}

/// Verifies max/min heap extraction and destructive foreach ordering.
#[test]
fn test_spl_max_and_min_heap_ordering() {
    let out = compile_and_run(
        r#"<?php
$max = new SplMaxHeap();
$min = new SplMinHeap();
foreach ([3, 1, 5, 2] as $value) {
    $max->insert($value);
    $min->insert($value);
}

echo $max->top();
echo ":";
while (!$max->isEmpty()) {
    echo $max->extract();
}
echo "|";
foreach ($min as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "5:5321|3=1;2=2;1=3;0=5;");
}

/// Verifies that user subclasses of `SplHeap` dispatch through their protected `compare()` override.
#[test]
fn test_spl_heap_subclass_compare_override() {
    let out = compile_and_run(
        r#"<?php
class ReverseHeap extends SplHeap {
    protected function compare(mixed $left, mixed $right): int {
        return $right <=> $left;
    }
}

$heap = new ReverseHeap();
foreach ([4, 1, 3] as $value) {
    $heap->insert($value);
}
while ($heap->valid()) {
    echo $heap->current();
    $heap->next();
}
"#,
    );
    assert_eq!(out, "134");
}

/// Verifies priority queue top/extract behavior and extraction flags.
#[test]
fn test_spl_priority_queue_extract_flags() {
    let out = compile_and_run(
        r#"<?php
$queue = new SplPriorityQueue();
$queue->insert("low", 1);
$queue->insert("high", 5);
$queue->insert("mid", 3);

echo $queue->top();
echo "|";
$queue->setExtractFlags(SplPriorityQueue::EXTR_BOTH);
$both = $queue->extract();
echo $both["data"];
echo ":";
echo $both["priority"];
echo "|";
$queue->setExtractFlags(SplPriorityQueue::EXTR_PRIORITY);
echo $queue->extract();
echo "|";
$queue->setExtractFlags(SplPriorityQueue::EXTR_DATA);
foreach ($queue as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
}
"#,
    );
    assert_eq!(out, "high|high:5|3|0=low");
}

/// Verifies object storage attach, ArrayAccess, info updates, iteration, hashes, and detach.
#[test]
fn test_spl_object_storage_attach_arrayaccess_and_iteration() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $id;
    public function __construct(int $id) {
        $this->id = $id;
    }
}

$left = new Box(1);
$right = new Box(2);
$storage = new SplObjectStorage();
$storage->attach($left, "left");
$storage[$right] = "right";

echo count($storage);
echo ":";
echo $storage->contains($left) ? "yes" : "no";
echo ":";
echo $storage[$right];
echo ":";

$storage->rewind();
echo $storage->key();
echo "=";
echo $storage->getInfo();
$storage->next();
$storage->setInfo("RIGHT");
echo ";";
echo $storage->key();
echo "=";
echo $storage[$right];
echo ":";
echo $storage->getHash($left) === $storage->getHash($left) ? "stable" : "drift";

$storage->detach($left);
echo ":";
echo count($storage);
"#,
    );
    assert_eq!(out, "2:yes:right:0=left;1=RIGHT:stable:1");
}

/// Verifies `SplObjectStorage::addAll`, `removeAll`, and `removeAllExcept`.
#[test]
fn test_spl_object_storage_bulk_operations() {
    let out = compile_and_run(
        r#"<?php
class Item {}

$a = new Item();
$b = new Item();
$c = new Item();

$left = new SplObjectStorage();
$left->attach($a, "a");
$left->attach($b, "b");

$right = new SplObjectStorage();
$right->attach($b, "B");
$right->attach($c, "C");

$left->addAll($right);
echo count($left);
echo ":";
echo $left[$b];
echo ":";
$left->removeAllExcept($right);
echo count($left);
echo ":";
$left->removeAll($right);
echo count($left);
"#,
    );
    assert_eq!(out, "3:B:2:0");
}

/// Verifies Phase 6 containers clean their per-instance storage under heap-debug.
#[test]
fn test_phase6_spl_storage_finalizes_cleanly() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box {}

$heap = new SplMaxHeap();
$heap->insert("alpha");
$heap->insert([1, 2, 3]);

$queue = new SplPriorityQueue();
$queue->insert("low", 1);
$queue->insert("high", 2);

$storage = new SplObjectStorage();
$storage->attach(new Box(), ["payload" => "value"]);

unset($heap);
unset($queue);
unset($storage);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
