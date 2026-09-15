//! Purpose:
//! Verifies storage-neutral array reduction and arbitrary carry ownership.
//!
//! Called from:
//! - The runtime GC codegen suite on every executable target.
//!
//! Key details:
//! - Declared PHP array parameters and returns force runtime layout dispatch.
//! - Normal, empty, reentrant and exceptional reductions must retire all intermediate owners.

use crate::support::*;

/// Empty returned string arrays preserve an integer, string, object, array or omitted null initial.
#[test]
fn test_core_boxed_array_reduce_empty_preserves_initial_types() {
    assert_clean_reduce(r#"<?php
class EmptyReduceOwner { public int $value = 9; }
function emptyReduceStrings(): array { $items = ["a"]; array_pop($items); return $items; }
function neverReduce(mixed $carry, mixed $value): mixed { echo "unexpected|"; return $value; }
echo array_reduce(emptyReduceStrings(), fn($c, $v) => $c + strlen($v), 7), "|";
echo array_reduce(emptyReduceStrings(), "neverReduce", "initial"), "|";
echo array_reduce(emptyReduceStrings(), "neverReduce") === null ? "null|" : "bad|";
$owner = new EmptyReduceOwner();
$result = array_reduce(emptyReduceStrings(), "neverReduce", $owner);
echo $result === $owner ? "same|" : "bad|";
$initial = ["key" => "old"];
$copy = array_reduce(emptyReduceStrings(), "neverReduce", $initial);
$copy["key"] = "new";
echo $initial["key"], ":", $copy["key"];
unset($copy, $initial, $result, $owner);
"#, "7|initial|null|same|old:new");
}

/// Mixed carries change type, preserve float bits, and follow associative insertion order.
#[test]
fn test_core_boxed_array_reduce_dynamic_carries_and_layouts() {
    assert_clean_reduce(r#"<?php
function reduceItems(): array { return ["first" => "a", "removed" => "x", "last" => "b"]; }
function accumulateReduce(mixed $carry, mixed $item): mixed { return $carry . $item; }
$items = reduceItems();
unset($items["removed"]);
echo array_reduce($items, "accumulateReduce", "prefix:"), "|";
echo array_reduce([1.5, 2.25], fn($carry, $item) => $carry + $item, 0.5), "|";
echo array_reduce([1, 2, 3], fn($carry, $item) => $carry + $item), "|";
$result = array_reduce([1, 2], function(mixed $carry, int $item): mixed {
    if ($item === 1) { return "first"; }
    return ["value" => $carry . ":second"];
}, null);
echo $result["value"], "|";
echo array_reduce([true, false], fn($carry, $item) => $carry && $item, true) ? "bad" : "false";
unset($result, $items);
"#, "prefix:ab|4.25|6|first:second|false");
}

/// String, method, invokable and boxed callbacks use the same carry and result contract.
#[test]
fn test_core_boxed_array_reduce_callback_and_callable_matrix() {
    assert_clean_reduce(r#"<?php
function reduceNamed(string $carry, string $item): string { return $carry . $item; }
class ReduceReceiver {
    public function append(string $carry, string $item): string { return $carry . $item; }
    public function __invoke(string $carry, string $item): string { return $carry . $item; }
    public static function appendStatic(string $carry, string $item): string { return $carry . $item; }
}
function boxedReduce(mixed $callback, array $items): mixed { return array_reduce($items, $callback, ""); }
$receiver = new ReduceReceiver();
echo boxedReduce("reduceNamed", ["a", "b"]), "|";
echo boxedReduce([$receiver, "append"], ["a", "b"]), "|";
echo boxedReduce($receiver, ["a", "b"]), "|";
echo array_reduce(["a", "b"], [$receiver, "append"], ""), "|";
echo array_reduce(["a", "b"], ReduceReceiver::appendStatic(...), ""), "|";
$reduce = array_reduce(...);
echo $reduce(["a", "b"], reduceNamed(...), ""), "|";
echo call_user_func("array_reduce", ["a", "b"], "reduceNamed", ""), "|";
echo \ARRAY_REDUCE(...["initial" => "", "callback" => "reduceNamed", "array" => ["a", "b"]]);
unset($reduce, $receiver);
"#, "ab|ab|ab|ab|ab|ab|ab|ab");
}

/// An independently owned payload snapshot survives callback replacement of its caller's array.
#[test]
fn test_core_boxed_array_reduce_callback_mutation_keeps_snapshot() {
    assert_clean_reduce(r#"<?php
function reduceSnapshotItems(): array { return ["first" => "a", "last" => "b"]; }
$items = reduceSnapshotItems();
$callback = function(string $carry, string $item) use (&$items): string {
    $items = ["replacement" => "new"];
    return $carry . $item;
};
$result = array_reduce($items, $callback, "");
echo $result, "|", $items["replacement"];
unset($result, $items, $callback);
"#, "ab|new");
}

/// Empty and nonempty reductions preserve resource identity without retaining borrowed stack cells.
#[test]
fn test_core_boxed_array_reduce_resource_carry_preserves_identity() {
    assert_clean_reduce(r#"<?php
function keepReduceResource(mixed $carry, mixed $item): mixed { return $carry; }
$stream = fopen("php://memory", "w+");
$empty = array_reduce([], "keepReduceResource", $stream);
$result = array_reduce([1, 2], "keepReduceResource", $stream);
echo $empty === $stream ? "empty|" : "bad|";
echo $result === $stream ? "same|" : "bad|";
fwrite($stream, "live");
rewind($stream);
echo fread($stream, 4);
fclose($stream);
unset($empty, $result, $stream);
"#, "empty|same|live");
}

/// Validation rejects invalid sources and callbacks even when an empty array would skip iteration.
#[test]
fn test_core_boxed_array_reduce_invalid_arguments_are_catchable() {
    let out = compile_and_run(r#"<?php
function invalidReduceSource(mixed $source): void {
    try { array_reduce($source, fn($carry, $item) => $item); echo "missed|"; }
    catch (TypeError $error) { echo "source|"; unset($error); }
}
function invalidReduceCallback(mixed $callback): void {
    try { array_reduce([], $callback); echo "missed|"; }
    catch (TypeError $error) { echo "callback|"; unset($error); }
}
invalidReduceSource(7);
invalidReduceSource(null);
invalidReduceCallback(null);
invalidReduceCallback(42);
invalidReduceCallback("missingReduceCallback");
invalidReduceCallback([]);
echo "done";
"#);
    assert_eq!(out, "source|source|callback|callback|callback|callback|done");
}

/// A throwing callback releases its arguments, source snapshot, descriptor lease and previous carry.
#[test]
fn test_core_boxed_array_reduce_callback_throw_retires_owners() {
    assert_clean_reduce(r#"<?php
class ReduceCarryOwner {
    public function __destruct() { echo "carry|"; }
}
function throwingReduceItems(): array { return ["one", "two"]; }
function throwDuringReduce(): void {
    $prefix = "message:";
    try {
        array_reduce(throwingReduceItems(), function(mixed $carry, string $item) use ($prefix): mixed {
            if ($item === "two") { throw new RuntimeException($prefix . $item); }
            return $carry;
        }, new ReduceCarryOwner());
    } catch (RuntimeException $error) {
        echo $error->getMessage(), "|";
        unset($error);
    }
}
throwDuringReduce();
echo "done";
"#, "carry|message:two|done");
}

/// A destructor throw while replacing the carry also retires the already returned next carry.
#[test]
fn test_core_boxed_array_reduce_carry_destructor_throw_retires_next() {
    assert_clean_reduce(r#"<?php
class ReduceThrowingCarry {
    public function __destruct() { throw new RuntimeException("carry"); }
}
class ReduceNextCarry {
    public function __destruct() { echo "next|"; }
}
function replaceReduceCarry(mixed $carry, int $item): mixed {
    if ($item === 1) { return new ReduceThrowingCarry(); }
    return new ReduceNextCarry();
}
try { array_reduce([1, 2], "replaceReduceCarry"); echo "missed|"; }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
echo "done";
"#, "next|carry|done");
}

/// Checks observable output and clean heap ownership without suppressing runtime diagnostics.
fn assert_clean_reduce(source: &str, expected: &str) {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
