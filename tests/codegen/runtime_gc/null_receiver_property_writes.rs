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
