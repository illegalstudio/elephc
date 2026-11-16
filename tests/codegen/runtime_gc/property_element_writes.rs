//! Purpose:
//! Checks that an element write into a typed array or hash property mutates the property's own
//! container in place, without copying it, and that every PHP-visible effect of the write is
//! unchanged.
//!
//! Called from:
//! - The `codegen_tests` runtime GC suite.
//!
//! Key details:
//! - Allocation counts are taken only around the writes, with keys and slots prepared before
//!   the window so loop counters and `foreach` keys cannot box inside it.
//! - A copy taken before a write still separates, because the write itself keeps copy-on-write.
//! - An overwrite stores the new element before releasing the old one, so a destructor that
//!   reads, grows or replaces the container sees PHP's state. Expected output is PHP 8.5's.
//! - A boxed `mixed` value stored into concrete element storage is converted inside the write,
//!   and that conversion can run `__toString` or a warning handler which replaces the property.
//!   Those writes keep a private copy of the container, so they stay memory safe.
//! - Every fixture runs in both EIR modes under heap debugging, except where noted.

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

/// Asserts that both EIR modes print `expected`.
fn assert_both_modes(source: &str, expected: &str) {
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, expected, "optimized");
    assert_eq!(plain, expected, "--no-ir-opt");
}

/// Filling typed and inferred array properties allocates for growth only, not once per write.
///
/// Each write used to copy the whole container and free the previous one, so 2000 writes into
/// five properties cost well over 10000 allocations. In place, only table and array growth
/// allocates.
#[test]
fn test_property_element_writes_fill_without_copying_the_container() {
    let source = r#"<?php
final class Item { public int $v = 0; }
final class Store
{
    public array<int, Item> $objects = [];
    public array<int, int> $numbers = [];
    public array<int> $list = [];
    public $inferredList = [];
    public $inferredMap = [];
    public function fill(array<int> $keys, array<int> $slots, Item $item): void
    {
        foreach ($keys as $key) {
            $this->objects[$key] = $item;
            $this->numbers[$key] = $key;
            $this->list[] = $key;
            $this->inferredList[] = $key;
            $this->inferredMap[$key] = $key;
        }
        foreach ($keys as $key) { $this->objects[$key] = $item; }
        foreach ($slots as $slot) { $this->list[$slot] = $slot; }
    }
}
$keys = [];
$slots = [];
for ($i = 0; $i < $argc + 1999; $i++) { $keys[] = (int) ($i * 3); $slots[] = (int) $i; }
$store = new Store();
$item = new Item();
$before = memory_get_allocations();
$store->fill($keys, $slots, $item);
$after = memory_get_allocations();
echo $after - $before, "|", count($store->objects), ",", count($store->numbers), ",", count($store->list), ",",
    count($store->inferredList), ",", count($store->inferredMap), "|", $store->list[1999], ",", $store->numbers[5997];
"#;
    let (optimized, plain) = both_modes(source);
    for (mode, stdout) in [("optimized", optimized), ("--no-ir-opt", plain)] {
        let (allocations, rest) = stdout.split_once('|').expect("allocation count prefix");
        let allocations: u64 = allocations.parse().expect("allocation count");
        assert!(allocations < 200, "{mode}: {allocations} allocations for 14000 element writes");
        assert_eq!(rest, "2000,2000,2000,2000,2000|1999,5997", "{mode}");
    }
}

/// A copy, a clone and a default shared between instances all keep their own elements.
#[test]
fn test_property_element_writes_keep_copy_on_write() {
    let source = r#"<?php
final class Bag
{
    public array<int, int> $map = [];
    public array<string, string> $names = [];
    public array<int> $list = [];
    public $inferred = [1, 2];
    public $lookup = ['a' => 1];
}
$bag = new Bag();
$other = new Bag();
$snapshots = [];
for ($i = 0; $i < $argc + 199; $i++) {
    $bag->map[$i * 7] = $i;
    $bag->names["k$i"] = "v$i";
    $bag->list[] = $i;
    if ($i % 50 == 0) { $snapshots[] = $bag->map; $snapshots[] = $bag->list; }
}
$map = $bag->map;
$list = $bag->list;
$bag->map[7] = 100;
$bag->list[1] = 101;
$bag->list[] = 102;
$bag->inferred[] = 3;
$bag->inferred[0] = 9;
$bag->lookup['a'] = 8;
$bag->lookup['z'] = 7;
$clone = clone $bag;
$clone->map[7] = 200;
$clone->list[1] = 201;
$clone->names['k1'] = 'changed';
echo count($bag->map), ",", $bag->map[7], ",", $map[7], ",", $clone->map[7], "|";
echo count($bag->list), ",", $bag->list[1], ",", $list[1], ",", count($list), ",", $clone->list[1], "|";
echo $bag->names['k1'], ",", $clone->names['k1'], ",", count($bag->names), "|";
echo count($snapshots[0]), ",", count($snapshots[1]), ",", count($snapshots[6]), ",", count($snapshots[7]), "|";
echo count($bag->inferred), $bag->inferred[0], count($other->inferred), $other->inferred[0], ",";
echo count($bag->lookup), $bag->lookup['a'], count($other->lookup), $other->lookup['a'];
"#;
    assert_both_modes(source, "200,100,1,200|201,101,1,200,201|v1,changed,200|1,1,151,151|3921,2811");
}

/// The destructor of an overwritten element already sees the new element, and what it writes
/// to the same property survives the outer write.
#[test]
fn test_property_element_overwrite_publishes_before_the_destructor_runs() {
    let source = r#"<?php
final class Holder
{
    public array<int, Node> $map = [];
    public array<Node> $list = [];
}
final class Node
{
    public function __construct(public Holder $holder, public int $id, public string $mode) {}
    public function __destruct()
    {
        if ($this->mode === 'grow-map') {
            echo "map:", $this->holder->map[1]->id, " ";
            for ($i = 10; $i < 60; $i++) { $this->holder->map[$i] = new Node($this->holder, 100 + $i, ''); }
        } elseif ($this->mode === 'grow-list') {
            echo "list:", $this->holder->list[0]->id, " ";
            for ($i = 0; $i < 100; $i++) { $this->holder->list[] = new Node($this->holder, 200 + $i, ''); }
        } elseif ($this->mode === 'replace') {
            echo "replace ";
            $this->holder->map = [];
            $this->holder->list = [];
        }
    }
}
$holder = new Holder();
$holder->map[1] = new Node($holder, 1, 'grow-map');
$holder->map[1] = new Node($holder, 2, '');
echo count($holder->map), ",", $holder->map[1]->id, ",", $holder->map[59]->id, " ";
$holder->list[] = new Node($holder, 3, 'grow-list');
$holder->list[0] = new Node($holder, 4, '');
echo count($holder->list), ",", $holder->list[0]->id, ",", $holder->list[100]->id, " ";
$holder->map[1] = new Node($holder, 5, 'replace');
$holder->map[1] = new Node($holder, 6, '');
echo count($holder->map), " ";
$holder->list[0] = new Node($holder, 7, 'replace');
$holder->list[0] = new Node($holder, 8, '');
echo count($holder->list), " ";
$holder->map[5] = new Node($holder, 9, '');
$holder->list[] = new Node($holder, 10, '');
echo count($holder->map), count($holder->list);
"#;
    assert_both_modes(source, "map:2 51,2,159 list:4 101,4,299 replace 0 replace 0 11");
}

/// The same order holds for containers reached through `global`, including boxed elements.
#[test]
fn test_element_overwrite_publishes_before_the_destructor_for_global_containers() {
    let source = r#"<?php
final class Probe
{
    public function __construct(public int $id) {}
    public function __destruct()
    {
        global $hash, $list, $mixed;
        if ($this->id === 1) { echo "hash:", $hash['k']->id, " "; }
        if ($this->id === 2) { echo "list:", $list[0]->id, " "; }
        if ($this->id === 3) { echo "mixed:", is_string($mixed[0]) ? $mixed[0] : "object", " "; }
    }
}
$hash = ['k' => new Probe(1)];
$list = [new Probe(2)];
$mixed = [new Probe(3), "s", 3];
$hash['k'] = new Probe(10);
$list[0] = new Probe(20);
$mixed[0] = "replaced";
echo "end";
"#;
    assert_both_modes(source, "hash:10 list:20 mixed:replaced end");
}

/// Reference-bound, uninitialized, unset and nested properties take the write the same way,
/// and a throwing right-hand side leaves the property untouched.
#[test]
fn test_property_element_writes_follow_references_and_lazy_initialization() {
    let source = r#"<?php
final class Inner
{
    public array<string, int> $map = [];
    public array<int> $list = [];
}
final class Outer
{
    public Inner $inner;
    public array<int, Inner> $inners = [];
    public array<int, int> $late;
    public array<int> $lateList;
    public array<int, int> $dropped = [1 => 1];
    public $droppedList = [1, 2];
    public function __construct() { $this->inner = new Inner(); }
    public function fillLate(): void { $this->late[5] = 1; $this->lateList[] = 1; $this->lateList[0] = 2; }
}
function fail(): int { throw new Exception("fail"); }
$outer = new Outer();
$alias = &$outer->inner->map;
$outer->inner->map['a'] = 1;
$alias['b'] = 2;
$outer->inner->map['c'] = 3;
echo count($outer->inner->map), count($alias), $alias['c'], "|";
$outer->inner->list[] = 4;
$outer->inners[4] = new Inner();
$outer->inners[4]->map['x'] = 5;
$outer->inners[4]->list[] = 6;
echo $outer->inner->list[0], $outer->inners[4]->map['x'], $outer->inners[4]->list[0], "|";
$outer->fillLate();
echo count($outer->late), count($outer->lateList), $outer->lateList[0], "|";
unset($outer->dropped, $outer->droppedList);
$outer->dropped[4] = 1;
$outer->droppedList[] = 7;
echo count($outer->dropped), count($outer->droppedList), "|";
$outer->inner->map['a'] += 5;
$outer->inner->map['a']++;
$outer->inner->list[0] += 10;
$outer->inner->map['n'] ??= 7;
$outer->inner->map['n'] ??= 9;
echo $outer->inner->map['a'], $outer->inner->list[0], $outer->inner->map['n'], "|";
try { $outer->inner->list[] = fail(); } catch (Exception $e) { echo "caught "; }
try { $outer->inner->map['q'] = fail(); } catch (Exception $e) { echo "caught "; }
echo count($outer->inner->list), count($outer->inner->map);
"#;
    assert_both_modes(source, "333|456|112|11|7147|caught caught 14");
}

/// A warning handler that copies the property during the key conversion keeps its own copy.
#[test]
fn test_property_element_write_keeps_a_copy_taken_by_a_key_warning_handler() {
    let source = r#"<?php
final class Box
{
    public array<int, int> $map = [2 => 2];
    public array $kept = [];
}
function run(): void
{
    $box = new Box();
    set_error_handler(function (int $no, string $message) use ($box): bool {
        echo "[warned] ";
        $box->kept[] = $box->map;
        return true;
    });
    $box->map[1.5] = 3;
    restore_error_handler();
    echo count($box->kept[0]), isset($box->kept[0][1]) ? "y" : "n", $box->map[2];
}
run();
"#;
    assert_both_modes(source, "[warned] 1n2");
}

/// A warning handler or a `__toString` that replaces the property while a boxed value is being
/// converted for the element cannot free the container the write is using.
///
/// The write is made on a private copy that is then stored back, so the element lands and the
/// replacement made during the conversion is overwritten. PHP has no typed element storage,
/// so there is no PHP output to match here: the point is that the program neither crashes nor
/// corrupts the heap.
#[test]
fn test_property_element_write_survives_a_value_conversion_that_replaces_the_property() {
    let source = r#"<?php
final class Box
{
    public array<string, string> $names = ["a" => "x"];
    public array<string> $list = ["x"];
}
final class Replacer
{
    public function __construct(public Box $box) {}
    public function __toString(): string
    {
        $this->box->list = ["replaced", "kept", "more"];
        return "cast";
    }
}
function pushList(Box $box, mixed $value): void { $box->list[] = $value; }
function warned(Box $box, mixed $value): void
{
    set_error_handler(function (int $no, string $message) use ($box): bool {
        echo "[warned] ";
        $box->names = ["z" => "handler"];
        return true;
    });
    $box->names["w"] = $value;
    restore_error_handler();
}
function run(): void
{
    $box = new Box();
    pushList($box, new Replacer($box));
    echo count($box->list), $box->list[0], "|";
    warned($box, [1, 2]);
    echo count($box->names), $box->names["w"], "|";
    pushList($box, "tail");
    $box->names["a"] = "plain";
    echo count($box->list), $box->list[2], $box->names["a"];
}
run();
"#;
    assert_both_modes(source, "2x|[warned] 2Array|3tailplain");
}

/// The keyed form of the same hazard: `__toString` replaces the hash while its value is cast.
///
/// Run without the clean-heap assertion: storing a boxed object into a string element leaks
/// the cast string on the released compiler too, which is tracked separately.
#[test]
fn test_property_hash_write_survives_a_string_cast_that_replaces_the_property() {
    let source = r#"<?php
final class Box { public array<string, string> $names = ["a" => "x"]; }
final class Replacer
{
    public function __construct(public Box $box) {}
    public function __toString(): string
    {
        $this->box->names = ["a" => "replaced", "b" => "kept"];
        return "cast";
    }
}
function setName(Box $box, mixed $value): void { $box->names["a"] = $value; }
function run(): void
{
    $box = new Box();
    for ($i = 0; $i < 20; $i++) { setName($box, new Replacer($box)); }
    echo count($box->names), "[", $box->names["a"], "]";
}
run();
"#;
    assert_eq!(compile_and_run(source), "1[cast]");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "1[cast]");
}
