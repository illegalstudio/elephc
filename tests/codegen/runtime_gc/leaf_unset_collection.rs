//! Purpose:
//! Checks that removing a leaf value (int, float, bool, string, null) skips the automatic cycle
//! collector, while removing any value that may own a graph keeps its safe point.
//!
//! Called from:
//! - The `codegen_tests` runtime GC suite.
//!
//! Key details:
//! - `gc_status()['runs']` counts only productive passes, so each probe first strands one
//!   unreachable cycle; a safe point reclaims it (+1) and a skipped one leaves it pending (0).
//! - The decision reads the removed entry's runtime tag, never the declared element type.
//! - Leaf removals skip it in typed and untyped arrays, locals, properties and static properties.

use crate::support::*;

/// Leaf removals leave a stranded cycle pending; graph-owning removals reclaim it.
#[test]
fn test_leaf_unset_collection_skips_only_leaf_values() {
    let source = r#"<?php
final class Leaf {}
class Node { public $next; }
final class Noisy { public function __destruct() { echo "noisy-gone "; } }
final class Holder
{
    private array<int, int> $ints = [];
    private array $plain = [];
    private array<int, Leaf> $leaves = [];
    private array<int, Node> $nodes = [];
    public static array $names = [];
    public function fill(int $count): void
    {
        for ($i = 0; $i < $count; $i++) {
            $this->ints[$i] = $i;
            $this->plain[$i] = 'v' . $i;
            self::$names[$i] = $i * 1.5;
            $this->leaves[$i] = new Leaf();
            $this->nodes[$i] = new Node();
        }
    }
    public function dropLeafValues(int $count): void
    {
        for ($i = 0; $i < $count; $i++) { unset($this->ints[$i], $this->plain[$i], self::$names[$i]); }
    }
    public function dropLeaves(int $count): void { for ($i = 0; $i < $count; $i++) { unset($this->leaves[$i]); } }
    public function dropNodes(int $count): void { for ($i = 0; $i < $count; $i++) { unset($this->nodes[$i]); } }
    public function sizes(): string
    {
        return count($this->ints) . '/' . count($this->plain) . '/' . count(self::$names) . '/'
            . count($this->leaves) . '/' . count($this->nodes);
    }
}
function runs(): int { return gc_status()['runs']; }
function strandCycle(): void { $a = new Node(); $b = new Node(); $a->next = $b; $b->next = $a; }
$holder = new Holder();
$holder->fill($argc + 3);
strandCycle();
$before = runs();
$holder->dropLeafValues($argc + 3);
echo "leaf-runs=", runs() - $before, " ";
$before = runs();
$holder->dropLeaves($argc + 3);
echo "object-runs=", runs() - $before, " ";
strandCycle();
$before = runs();
$holder->dropNodes($argc + 3);
echo "node-runs=", runs() - $before, " sizes=", $holder->sizes(), "\n";

$names = ['x' => 'one', 'y' => 'two', 'z' => 'three'];
$tagged = ['a' => new Leaf()];
strandCycle();
$before = runs();
unset($names['x'], $names['y'], $names['z']);
echo "local-leaf-runs=", runs() - $before, " ";
unset($tagged['a']);
echo "local-object-runs=", runs() - $before, " ", count($names), count($tagged), "\n";

$noisy = [1 => new Noisy()];
strandCycle();
$before = runs();
unset($noisy[1]);
echo "destructor-runs=", runs() - $before, "\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(
        output.stdout,
        "leaf-runs=0 object-runs=1 node-runs=1 sizes=0/0/0/0/0\n\
local-leaf-runs=0 local-object-runs=1 00\nnoisy-gone destructor-runs=1\n"
    );
}
