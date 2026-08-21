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

/// Verifies heap debug projections use php-src's private keys and physical heap order.
#[test]
fn test_spl_heap_debug_info_matches_php_private_snapshot() {
    let out = compile_and_run(
        r#"<?php
$max = new SplMaxHeap();
foreach ([1, 5, 2, 4, 3, 6] as $value) {
    $max->insert($value);
}
$debug = $max->__debugInfo();
echo $debug["\0SplHeap\0flags"];
echo ":";
echo $debug["\0SplHeap\0isCorrupted"] ? "1" : "0";
echo ":";
foreach ($debug["\0SplHeap\0heap"] as $value) {
    echo $value;
    echo ",";
}
echo ":";
echo count($max);
echo ":";
echo $max->top();
echo "\n";

$min = new SplMinHeap();
foreach ([5, 4, 3, 2, 1] as $value) {
    $min->insert($value);
}
$debug = $min->__debugInfo();
foreach ($debug["\0SplHeap\0heap"] as $value) {
    echo $value;
    echo ",";
}
echo "\n";
"#,
    );
    assert_eq!(out, "0:0:6,4,5,1,3,2,:6:6\n1,2,4,5,3,\n");
}

/// Verifies priority debug heaps retain data/priority pairs and current flags.
#[test]
fn test_spl_priority_queue_debug_info_matches_php_pair_snapshot() {
    let out = compile_and_run(
        r#"<?php
$queue = new SplPriorityQueue();
foreach ([['a', 1], ['b', 5], ['c', 2], ['d', 4], ['e', 3], ['f', 6]] as $pair) {
    $queue->insert($pair[0], $pair[1]);
}
$queue->setExtractFlags(SplPriorityQueue::EXTR_PRIORITY);
$debug = $queue->__debugInfo();
echo $debug["\0SplPriorityQueue\0flags"];
echo ":";
echo $debug["\0SplPriorityQueue\0isCorrupted"] ? "1" : "0";
echo ":";
foreach ($debug["\0SplPriorityQueue\0heap"] as $pair) {
    echo $pair['data'];
    echo "=";
    echo $pair['priority'];
    echo ",";
}
echo ":";
echo count($queue);
echo "\n";
"#,
    );
    assert_eq!(out, "2:0:f=6,d=4,b=5,a=1,e=3,c=2,:6\n");
}

/// Verifies post-extraction physical layouts distinguish histories exactly like php-src.
#[test]
fn test_spl_heap_debug_info_preserves_php_physical_history_after_extract() {
    let out = compile_and_run(
        r#"<?php
$direct = new SplMaxHeap();
foreach ([4, 3, 2, 1] as $value) {
    $direct->insert($value);
}
foreach ($direct->__debugInfo()["\0SplHeap\0heap"] as $value) {
    echo $value;
    echo ",";
}
echo "\n";

$max = new SplMaxHeap();
foreach ([5, 4, 3, 2, 1] as $value) {
    $max->insert($value);
}
echo $max->extract();
echo ":";
foreach ($max->__debugInfo()["\0SplHeap\0heap"] as $value) {
    echo $value;
    echo ",";
}
echo "\n";

$min = new SplMinHeap();
foreach ([5, 4, 3, 2, 1] as $value) {
    $min->insert($value);
}
echo $min->extract();
echo ":";
foreach ($min->__debugInfo()["\0SplHeap\0heap"] as $value) {
    echo $value;
    echo ",";
}
echo "\n";

$queue = new SplPriorityQueue();
foreach ([["a", 5], ["b", 4], ["c", 3], ["d", 2], ["e", 1]] as $pair) {
    $queue->insert($pair[0], $pair[1]);
}
$queue->setExtractFlags(SplPriorityQueue::EXTR_BOTH);
$extracted = $queue->extract();
echo $extracted["data"];
echo "=";
echo $extracted["priority"];
echo ":";
foreach ($queue->__debugInfo()["\0SplPriorityQueue\0heap"] as $pair) {
    echo $pair["data"];
    echo "=";
    echo $pair["priority"];
    echo ",";
}
echo "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "4,3,2,1,\n",
            "5:4,2,3,1,\n",
            "1:2,3,4,5,\n",
            "a=5:b=4,d=2,c=3,e=1,\n",
        )
    );
}

/// Verifies debug projection neither invokes `compare()` nor mutates the receiver.
#[test]
fn test_spl_heap_debug_info_does_not_compare_or_mutate() {
    let out = compile_and_run(
        r#"<?php
class CountingHeap extends SplHeap {
    public int $calls = 0;

    protected function compare(mixed $left, mixed $right): int {
        $this->calls = $this->calls + 1;
        return $left <=> $right;
    }
}

$heap = new CountingHeap();
foreach ([1, 5, 2, 4, 3, 6] as $value) {
    $heap->insert($value);
}
echo $heap->calls;
echo ":";
$debug = $heap->__debugInfo();
echo $heap->calls;
echo ":";
foreach ($debug["\0SplHeap\0heap"] as $value) {
    echo $value;
    echo ",";
}
echo ":";
echo count($heap);
echo ":";
echo $heap->top();
"#,
    );
    assert_eq!(out, "7:7:6,4,5,1,3,2,:6:6");
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
