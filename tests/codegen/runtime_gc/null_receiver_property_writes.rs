//! Purpose:
//! Checks that a property write whose object receiver is null at run time throws PHP's
//! catchable `Error` instead of dereferencing the null sentinel.
//!
//! Called from:
//! - The `codegen_tests` runtime GC suite.
//!
//! Key details:
//! - A class-typed receiver can be null: an element read that misses its container answers the
//!   null sentinel with the element's declared type (`$nodes[5]->count` on a one-element list).
//! - Plain assignments report `Attempt to assign property`, element writes and appends report
//!   `Attempt to modify property`, as PHP 8.5 does. Expected output is PHP's.
//! - The missed element read is silenced with `@`: PHP fetches it for write without a warning.
//! - `isset`, `empty` and `??` on a property of that receiver stay silent and answer as for null,
//!   and a readonly store reports the null receiver, not an initialized overwrite.
//! - An `ArrayAccess` object property is read before its `offsetSet` call; the null receiver is
//!   rejected ahead of that read.
//! - Compound updates and `++`/`--` of a property element, as statements or expressions, throw
//!   before their element read: no warning, no `offsetGet` call, no readonly diagnostic.
//! - Every fixture runs in both EIR modes under heap debugging and must leave a clean heap.

use crate::support::*;

/// Runs one fixture in both EIR modes, requiring clean heaps and one shared stdout.
fn run_clean(source: &str) -> String {
    let optimized = compile_and_run_with_heap_debug(source);
    let plain = without_ir_opt(|| compile_and_run_with_heap_debug(source));
    for (mode, output) in [("optimized", &optimized), ("--no-ir-opt", &plain)] {
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", output.stderr);
    }
    assert_eq!(optimized.stdout, plain.stdout, "both EIR modes print the same output");
    optimized.stdout
}

/// Scalar stores, element writes and appends through a missing element all throw `Error`,
/// and the same writes through a present object still work.
#[test]
fn test_property_write_on_null_object_receiver_throws_error() {
    let source = r#"<?php
final class Inner { public int $v = 0; }
final class Node
{
    public int $count = 0;
    public string $name = '';
    public array<int, int> $map = [];
    public array<int> $list = [];
    public array $plain = [];
    public Inner $held;
    public function __construct() { $this->held = new Inner(); }
}
function attempt(callable $write): void
{
    try { $write(); echo "no error|"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "|"; }
}
function run(array<Node> $nodes, array<string, Node> $named): void
{
    attempt(function () use ($nodes): void { @$nodes[5]->count = 2; });
    attempt(function () use ($nodes): void { @$nodes[5]->name = 'x'; });
    attempt(function () use ($nodes): void { @$nodes[5]->map[1] = 2; });
    attempt(function () use ($nodes): void { @$nodes[5]->list[] = 2; });
    attempt(function () use ($nodes): void { @$nodes[5]->list[0] = 2; });
    attempt(function () use ($nodes): void { @$nodes[5]->plain['k'] = 2; });
    attempt(function () use ($named): void { @$named['x']->map[1] = 2; });
    attempt(function () use ($nodes): void { $missing = @$nodes[5]; $missing->count = 3; });
    attempt(function () use ($nodes): void { $missing = @$nodes[5]; $missing->list[] = 3; });
    attempt(function () use ($nodes): void { $nodes[0]->count = 4; $nodes[0]->map[1] = 5; $nodes[0]->list[] = 6; $nodes[0]->held->v = 7; });
    echo $nodes[0]->count, $nodes[0]->map[1], $nodes[0]->list[0], $nodes[0]->held->v;
}
run([new Node()], ['a' => new Node()]);
"#;
    let assign = |name: &str| format!("Error: Attempt to assign property \"{name}\" on null|");
    let modify = |name: &str| format!("Error: Attempt to modify property \"{name}\" on null|");
    let expected = [
        assign("count"), assign("name"), modify("map"), modify("list"), modify("list"),
        modify("plain"), modify("map"), assign("count"), modify("list"),
        "no error|4567".to_string(),
    ].concat();
    assert_eq!(run_clean(source), expected);
}

/// Probing a property of a null receiver is silent, and readonly stores made from inside the
/// class report the null receiver instead of a readonly violation.
#[test]
fn test_property_probes_and_readonly_stores_on_null_object_receiver_match_php() {
    let source = r#"<?php
final class Node
{
    public int $count = 0;
    public array<int> $list = [];
    public readonly int $ro;
    public readonly array<int> $roList;
    public function __construct() { $this->ro = 1; $this->roList = [1]; }
    public function reset(array<Node> $nodes): void
    {
        $other = @$nodes[5];
        try { $other->ro = 3; echo "no error|"; } catch (Error $e) { echo $e->getMessage(), "|"; }
        try { $other->roList[] = 3; echo "no error|"; } catch (Error $e) { echo $e->getMessage(), "|"; }
    }
}
function run(array<Node> $nodes): void
{
    $missing = @$nodes[5];
    var_export(isset($missing->count)); echo ",";
    var_export(empty($missing->count)); echo ",";
    var_export($missing->count ?? 1); echo ",";
    var_export($missing->list[0] ?? 7); echo "|";
    $nodes[0]->reset($nodes);
    $present = $nodes[0];
    var_export(isset($present->count)); echo ",";
    var_export(empty($present->count)); echo ",";
    var_export($present->count ?? 1); echo ",";
    var_export($present->list[0] ?? 7);
}
run([new Node()]);
"#;
    let expected = "false,true,1,7|Attempt to assign property \"ro\" on null|\
                    Attempt to modify property \"roList\" on null|true,true,0,7";
    let optimized = compile_and_run_with_heap_debug(source);
    let plain = without_ir_opt(|| compile_and_run_with_heap_debug(source));
    for (mode, output) in [("optimized", &optimized), ("--no-ir-opt", &plain)] {
        assert_eq!(output.stdout, expected, "{mode}");
        assert!(!output.stderr.contains("Warning"), "{mode}: probes stay silent: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", output.stderr);
    }
}

/// An element write into an `ArrayAccess` object property reads the slot before calling it;
/// through a null receiver it throws PHP's `Error` instead of reading the property of null.
#[test]
fn test_array_access_property_element_write_on_null_object_receiver_throws_error() {
    let source = r#"<?php
class Slot
{
    public ArrayObject $items;
    public ?ArrayObject $maybe = null;
    public function __construct() { $this->items = new ArrayObject(); }
}
function write(array<Slot> $slots): void
{
    try { $missing = @$slots[5]; $missing->items[0] = 1; echo "no error|"; }
    catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "|"; }
    try { $missing = @$slots[5]; $missing->maybe[0] = 1; echo "no error|"; }
    catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "|"; }
    $slots[0]->items[0] = 1;
    echo count($slots[0]->items);
}
write([new Slot()]);
"#;
    let expected = "Error:Attempt to modify property \"items\" on null|\
                    Error:Attempt to modify property \"maybe\" on null|1";
    let optimized = compile_and_run_with_heap_debug(source);
    let plain = without_ir_opt(|| compile_and_run_with_heap_debug(source));
    for (mode, output) in [("optimized", &optimized), ("--no-ir-opt", &plain)] {
        assert_eq!(output.stdout, expected, "{mode}");
        assert!(!output.stderr.contains("Warning"), "{mode}: no property is read: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", output.stderr);
    }
}

/// A read-modify-write of a property element through a null receiver throws before the element
/// is read, for list, readonly and `ArrayAccess` properties, and the same updates through a
/// present object still work.
#[test]
fn test_property_element_updates_on_null_object_receiver_throw_before_reading() {
    let source = r#"<?php
final class Node
{
    public array<int> $list = [5];
    public readonly array<int> $roList;
    public ArrayObject $items;
    public function __construct() { $this->roList = [1]; $this->items = new ArrayObject([3]); }
    public function go(array<Node> $nodes): void
    {
        $missing = @$nodes[5];
        try { $missing->items[0] += 1; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
        try { $missing->items[0]++; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
        try { $missing->roList[0] += 1; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
        try { $missing->roList[0]++; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
        try { $missing->list[0] += 1; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
        try { $missing->list[0]++; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
        try { --$missing->list[0]; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
        $present = $nodes[0];
        $present->list[0] += 1; $present->list[0]++; --$present->list[0];
        $present->items[0] += 1; $present->items[0]++;
        $this->list[0] += 10; $this->list[0]++;
        echo $present->list[0], ",", $present->items[0], ",", $this->list[0], "\n";
    }
}
function expressions(array<Node> $nodes): void
{
    $missing = @$nodes[5];
    try { $x = $missing->list[0]++; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
    try { $x = ++$missing->list[0]; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
    try { $x = ($missing->list[0] += 4); echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
    try { $x = $missing->items[0]--; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
    try { $missing->items[0] .= "x"; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
    try { $missing->list[0] ??= 1; echo "no error|"; } catch (Throwable $e) { echo $e->getMessage(), "|"; }
    $x = $nodes[0]->list[0]++; $y = ++$nodes[0]->list[0]; $nodes[0]->list[1] ??= 9;
    $z = ($nodes[0]->list[0] += 4); $w = $nodes[0]->items[0]--;
    echo $x, ",", $y, ",", $z, ",", $w, ",", $nodes[0]->list[0], ",", $nodes[0]->list[1], ",", $nodes[0]->items[0];
}
$node = new Node();
$node->go([$node]);
expressions([$node]);
"#;
    let items = "Attempt to modify property \"items\" on null|";
    let ro_list = "Attempt to modify property \"roList\" on null|";
    let list = "Attempt to modify property \"list\" on null|";
    let expected = format!(
        "{items}{items}{ro_list}{ro_list}{list}{list}{list}17,5,17\n\
         {list}{list}{list}{items}{items}{list}17,19,23,5,23,9,4"
    );
    let optimized = compile_and_run_with_heap_debug(source);
    let plain = without_ir_opt(|| compile_and_run_with_heap_debug(source));
    for (mode, output) in [("optimized", &optimized), ("--no-ir-opt", &plain)] {
        assert_eq!(output.stdout, expected, "{mode}");
        assert!(!output.stderr.contains("Warning"), "{mode}: nothing is read: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", output.stderr);
    }
}
