//! Purpose:
//! Checks that a by-value `foreach` over a hash of objects binds each object without a
//! temporary `Mixed` box, and that every ownership edge of the binding is unchanged.
//!
//! Called from:
//! - The `codegen_tests` runtime GC suite.
//!
//! Key details:
//! - Allocation counts are taken only around the walk, so setup and output are excluded.
//! - Reference entries, early exits, source replacement and destructor order keep PHP behavior.
//! - Both EIR modes run under heap debugging.

use crate::support::*;

/// Runs one fixture in both EIR modes, requiring clean heaps and returning both stdouts.
fn both_modes(source: &str) -> (String, String) {
    let optimized = compile_and_run_with_heap_debug(source);
    let plain = without_ir_opt(|| compile_and_run_with_heap_debug(source));
    for (mode, output) in [("optimized", &optimized), ("--no-ir-opt", &plain)] {
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", output.stderr);
    }
    (optimized.stdout, plain.stdout)
}

/// Walking a typed object hash allocates nothing per entry (Ivory object-hash-iteration probe).
#[test]
fn test_foreach_hash_objects_walk_does_not_allocate() {
    let source = r#"<?php
abstract class Item { abstract public function tick(): void; }
final class Idle extends Item { public function tick(): void {} }
final class Groups
{
    private array<int, Item> $map = [];
    public function add(int $key, Item $item): void { $this->map[$key] = $item; }
    public function walk(): void
    {
        $items = $this->map;
        foreach ($items as $item) { $item->tick(); }
    }
    public function size(): int { return count($this->map); }
}
$groups = new Groups();
for ($i = 1; $i <= $argc + 999; $i++) { $groups->add($i, new Idle()); }
$before = memory_get_allocations();
$groups->walk();
$after = memory_get_allocations();
echo "allocations=", $after - $before, " size=", $groups->size();
"#;
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, "allocations=0 size=1000");
    assert_eq!(plain, "allocations=0 size=1000");
}

/// Reference entries, early exits, replaced sources and released bindings keep their semantics.
#[test]
fn test_foreach_hash_objects_preserve_binding_ownership() {
    let source = r#"<?php
final class Tracked
{
    public function __construct(public string $name) {}
    public function __destruct() { echo "drop:", $this->name, " "; }
}
function walk(): void
{
    array<string, Tracked> $map = [];
    $map['a'] = new Tracked('a');
    $map['b'] = new Tracked('b');
    $map['c'] = new Tracked('c');
    $alias = &$map['b'];
    foreach ($map as $key => $value) {
        echo $key, "=", $value->name, " ";
        if ($key === 'a') { $map = []; echo "cleared "; }
    }
    echo "after-loop ";
    unset($alias);
    echo "kept:", $value->name, " ";
    unset($value);
    echo "end ";
}
walk();
echo "\n";
function firstOf(array<int, Tracked> $map): Tracked
{
    foreach ($map as $value) { return $value; }
    throw new RuntimeException('empty');
}
$first = firstOf([7 => new Tracked('x'), 8 => new Tracked('y')]);
echo "first:", $first->name, "\n";
"#;
    let (optimized, plain) = both_modes(source);
    let expected = "a=a cleared b=b c=c drop:a after-loop drop:b kept:c drop:c end \ndrop:y first:x\ndrop:x ";
    assert_eq!(optimized, expected);
    assert_eq!(plain, expected);
}
