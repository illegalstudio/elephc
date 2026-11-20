//! Purpose:
//! Checks that a packed list bound to a declared hash parameter (`array<K, V>`) reaches the
//! callee as the hash it is compiled for, by value and by reference, and that assigning a list
//! to typed hash storage leaves the list alone.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - By value the argument is rebuilt as an independent hash, so the caller's list is unchanged.
//! - By reference an ordinary local is promoted to the hash before its address is taken and
//!   stays one afterwards, like a local promoted by `unset($a[$k])`. Inside a loop the
//!   promotion happens once, at the loop entry.
//! - A written `array<mixed> &` parameter is a contract too: the caller's typed list is boxed
//!   in place before the call, instead of the callee being compiled for the caller's elements.
//! - Descriptor calls carry boxed arguments; the invoker rebuilds a boxed list as the hash.
//! - Expected output is PHP 8.5's (with the generic annotations read as plain `array`).
//! - Before this, the callee wrote hash entries into the packed vector and crashed.
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

/// A list passed by value to functions, methods, constructors, closures, defaults and a
/// variadic collector is read as a hash, and the caller keeps its list.
#[test]
fn test_list_argument_binds_to_declared_hash_parameter_by_value() {
    let source = r#"<?php
final class Item { public function __construct(public int $v) {} }
final class Sink
{
    public function __construct(public array<int, int> $kept = [1, 2]) {}
    public function sum(array<int, int> $m): int { $m[9] = 7; return count($m) + $m[0]; }
    public static function second(array<int, int> $m): int { return count($m) + $m[1]; }
}
function ints(array<int, int> $m): int { $m[9] = 7; return count($m) + $m[0]; }
function size(array<int, int> $m): int { $m[9] = 7; return count($m); }
function boxed(array<int, mixed> $m): int { $m[9] = 'x'; return count($m); }
function objects(array<int, Item> $m): int { $m[9] = new Item(3); return count($m) + $m[0]->v; }
function strings(array<int, string> $m): string { $m[9] = 'z'; return count($m) . $m[0]; }
function defaults(int $x, array<int, int> $m = [1, 2]): int { $m[9] = 7; return count($m) + $m[0] + $x; }
function collect(array<int, int> ...$maps): int { $n = 0; foreach ($maps as $m) { $n += count($m) + $m[0]; } return $n; }
function run(): void
{
    $ids = [4, 5];
    $items = [new Item(1), new Item(2)];
    $names = ['a', 'b'];
    $empty = [];
    echo ints($ids), ints([4, 5]), size($empty), size([]), "|";
    echo boxed($ids), objects($items), strings($names), "|";
    echo (new Sink())->sum($ids), Sink::second($ids), "|";
    $sink = new Sink($ids);
    $sink->kept[9] = 1;
    echo count($sink->kept), $sink->kept[0], count((new Sink())->kept), "|";
    $closure = function (array<int, int> $m): int { $m[9] = 7; return count($m) + $m[0]; };
    $arrow = fn(array<int, int> $m): int => count($m) + $m[1];
    echo $closure($ids), $arrow($ids), "|";
    echo defaults(m: $ids, x: 1), defaults(1), collect($ids, [7]), "|";
    echo count($ids), $ids[1], count($items), $items[1]->v, count($names), $names[1], count($empty);
}
run();
"#;
    assert_eq!(run_clean(source), "7711|343a|77|342|77|8514|25222b0");
}

/// A list local bound by reference is promoted in place: the callee's writes are visible, and
/// the local keeps working as a keyed array afterwards.
#[test]
fn test_list_local_is_promoted_for_declared_hash_reference_parameter() {
    let source = r#"<?php
final class Item { public function __construct(public int $v) {} }
final class Writer
{
    public function put(array<int, int> &$m): void { $m[9] = 7; }
    public static function mark(array<int, int> &$m): void { $m[8] = 1; }
}
function put(array<int, int> &$m, int $k): void { $m[$k] = 7; }
function boxed(array<int, mixed> &$m): void { $m[0] = 'x'; }
function objects(array<int, Item> &$m): void { $m[9] = new Item(3); }
function strings(array<int, string> &$m): void { $m[9] = 'z'; }
function straight(): void
{
    $ids = [4, 5];
    put($ids, 0);
    echo $ids[0], $ids[1], count($ids), " ";
    $empty = [];
    put($empty, 3);
    echo $empty[3], count($empty), " ";
    $mixed = [4, 5];
    boxed($mixed);
    echo $mixed[0], $mixed[1], count($mixed), " ";
    $items = [new Item(1), new Item(2)];
    objects($items);
    echo count($items), $items[0]->v, $items[9]->v, " ";
    $names = ['a', 'b'];
    strings($names);
    echo count($names), $names[0], $names[9], "|";
}
function callables(): void
{
    $ids = [4, 5];
    (new Writer())->put($ids);
    Writer::mark($ids);
    $closure = function (array<int, int> &$m): void { $m[7] = 1; };
    $closure($ids);
    echo count($ids), $ids[7], $ids[8], $ids[9], "|";
}
function branches(int $n): void
{
    $ids = [4, 5];
    if ($n > 1) { put($ids, 9); } else { $ids[] = 6; }
    echo count($ids);
    foreach ($ids as $k => $v) { echo " $k=$v"; }
    echo "|";
}
function afterwards(array<int> $given): void
{
    $ids = [4, 5];
    $copy = $ids;
    put($ids, 9);
    $ids[] = 1;
    $ids[0] = 2;
    echo count($ids), " ", implode(",", $ids), " ", array_sum($ids), " ", count($copy), $copy[1], " ";
    put($ids, 20);
    for ($i = 0; $i < 3; $i++) { put($ids, 30 + (int) $i); }
    echo count($ids), " ";
    put($given, 5);
    echo count($given), $given[5];
}
straight();
callables();
branches(1);
branches(2);
afterwards([1, 2]);
"#;
    assert_eq!(
        run_clean(source),
        "752 71 x52 313 3az|5117|3 0=4 1=5 2=6|3 0=4 1=5 9=7|4 2,5,7,1 15 25 8 37"
    );
}

/// Assigning a list to a typed hash property, static property or local converts a copy and
/// leaves the source list intact.
#[test]
fn test_list_assigned_to_typed_hash_storage_keeps_the_source() {
    let source = r#"<?php
final class Item { public function __construct(public int $v) {} }
final class Holder
{
    public array<int, int> $map = [];
    public array<int, string> $names = [];
    public array<int, Item> $items = [];
    public static array<int, int> $shared = [];
}
function make(): array<int> { return [4, 5, 6]; }
function run(): void
{
    $ids = [4, 5];
    $holder = new Holder();
    $holder->map = $ids;
    $holder->map[9] = 1;
    echo count($holder->map), $holder->map[0], "|", count($ids), $ids[1], "|";
    array<int, int> $local = $ids;
    $local[9] = 1;
    echo count($local), $local[0], "|", count($ids), $ids[1], "|";
    Holder::$shared = $ids;
    Holder::$shared[9] = 1;
    echo count(Holder::$shared), "|", count($ids), $ids[0], "|";
    $items = [new Item(1), new Item(2)];
    $holder->items = $items;
    $holder->items[9] = new Item(3);
    echo count($holder->items), $holder->items[9]->v, "|", count($items), $items[1]->v, "|";
    $holder->map = [7, 8];
    $holder->names = ['x', 'y'];
    $holder->map = make();
    echo count($holder->map), $holder->map[2], count($holder->names), $holder->names[1];
}
run();
"#;
    assert_eq!(run_clean(source), "34|25|34|25|3|24|33|22|362y");
}

/// A list first bound to a hash reference inside a loop is promoted once at the loop entry,
/// and one variable can fill two reference slots of the same call.
#[test]
fn test_list_local_bound_to_hash_reference_inside_loops() {
    let source = r#"<?php
function put(array<int, int> &$ids, int $k): void { $ids[$k] = 7; }
function grow(array<int, int> &$ids): void { $ids[count($ids) + 50] = 7; }
function drop(array<int, int> &$ids, int $k): void { unset($ids[$k]); }
function two(array<int, int> &$a, array<int, int> &$b): void { $a[5] = 1; $b[6] = 2; }
function dump(array<int, int> $ids): void { echo count($ids); foreach ($ids as $k => $v) { echo " $k=$v"; } }
function forLoop(): void
{
    $ids = [4, 5];
    for ($i = 0; $i < 3; $i++) { echo $ids[0], count($ids), " "; put($ids, 10 + (int) $i); }
    dump($ids);
}
function whileLoop(int $n): void
{
    $ids = [4, 5];
    while ($n > 0) { grow($ids); $n = $n - 1; }
    echo count($ids), $ids[0], $ids[52];
}
function readBefore(): void
{
    $ids = [4, 5];
    $sum = 0;
    for ($i = 0; $i < 3; $i++) { $sum += $ids[0] + count($ids); $ids[] = 1; put($ids, 100 + (int) $i); }
    echo $sum, " ", count($ids);
}
function nested(): void
{
    $ids = [4, 5];
    foreach ([1, 2] as $a) {
        foreach ([10, 20] as $b) {
            if ($b > 10) { put($ids, $a * $b); } else { $ids[] = $a; }
        }
    }
    dump($ids);
}
function doWhile(int $n): void
{
    $ids = [4, 5];
    do { if ($n % 2 == 0) { put($ids, 30 + $n); } echo count($ids); $n = $n - 1; } while ($n > 0);
    echo " ", count($ids);
}
function twoCallees(): void
{
    $ids = [4, 5, 6];
    foreach ([0, 1] as $k) { drop($ids, $k); put($ids, $k + 40); }
    dump($ids);
}
function sameVariable(): void
{
    $ids = [4, 5];
    two($ids, $ids);
    echo count($ids), $ids[5], $ids[6];
}
forLoop(); echo "|";
whileLoop(3); echo "|";
readBefore(); echo "|";
nested(); echo "|";
doWhile(4); echo "|";
twoCallees(); echo "|";
sameVariable();
"#;
    assert_eq!(
        run_clean(source),
        "42 43 44 5 0=4 1=5 10=7 11=7 12=7|547|23 6|6 0=4 1=5 2=1 20=7 21=2 40=7|3344 4|\
         3 2=6 40=7 41=7|412"
    );
}

/// A typed list bound to a written `array<mixed> &` parameter is boxed in place, so what the
/// callee stores reads back as itself from every caller, in loops too.
#[test]
fn test_typed_list_local_is_boxed_for_array_mixed_reference_parameter() {
    let source = r#"<?php
final class Item { public function __construct(public int $v) {} }
final class Appender { public function add(array<mixed> &$a): void { $a[] = 'x'; } }
function add(array<mixed> &$a): void { $a[] = 'x'; }
function swap(array<mixed> &$a, int $i): void { $a[$i] = 'gone'; }
function straight(): void { $ids = [4, 5]; add($ids); echo $ids[2], count($ids), $ids[0] + 1; }
function callers(): void
{
    $ids = [4, 5];
    add($ids);
    $names = ['a', 'b'];
    add($names);
    $mixed = [1, 'b'];
    add($mixed);
    echo $ids[2], count($ids), $names[2], $names[0], count($mixed), $mixed[2];
}
function method(): void { $ids = [4, 5]; (new Appender())->add($ids); echo $ids[2], count($ids), $ids[0] + 1; }
function loop(): void
{
    $ids = [4, 5];
    for ($i = 0; $i < 3; $i++) { echo $ids[0]; add($ids); }
    echo $ids[2], count($ids), $ids[0] + 1;
}
function objects(): void
{
    $items = [new Item(1), new Item(2), new Item(3)];
    foreach ([0, 2] as $i) { swap($items, $i); }
    echo $items[0], $items[1]->v, $items[2], count($items);
}
function emptyList(): void { $empty = []; add($empty); echo count($empty), $empty[0]; }
straight(); echo "|";
callers(); echo "|";
method(); echo "|";
loop(); echo "|";
objects(); echo "|";
emptyList();
"#;
    assert_eq!(run_clean(source), "x35|x3xa3x|x35|444x55|gone2gone3|1x");
}

/// Calls that carry their arguments boxed (string and array callables, callables chosen at
/// run time, `call_user_func*`, a declared PHP `array` value) rebuild a list as the hash too,
/// including defaults, variadic collectors, by-reference writers and a throwing callee.
#[test]
fn test_list_argument_reaches_hash_parameter_through_dynamic_calls() {
    let source = r#"<?php
final class Item { public function __construct(public int $v) {} }
final class Keeper
{
    public function __construct(public array<int, int> $kept) {}
    public function sum(array<int, int> $m): int { $m[9] = 7; return count($m) + $m[0]; }
    public static function second(array<int, int> $m): int { $m[9] = 7; return count($m) + $m[1]; }
    public function fail(array<int, int> $m): int { throw new Exception("thrown" . count($m)); }
}
function first(array<int, int> $m): int { $m[9] = 7; return count($m) + $m[0]; }
function last(array<int, int> $m): int { $m[8] = 7; return count($m) + $m[1]; }
function withDefault(int $x, array<int, int> $m = [4, 5]): int { $m[9] = 7; return count($m) + $m[0] + $x; }
function otherDefault(int $x, array<int, int> $m = [6]): int { $m[8] = 7; return count($m) + $m[0] + $x; }
function items(array<int, Item> $m): int { $m[9] = new Item(5); return count($m) + $m[0]->v; }
function noItems(array<int, Item> $m): int { return count($m); }
function collect(array<int, int> ...$maps): int { $n = 0; foreach ($maps as $m) { $m[9] = 1; $n += count($m) + $m[0]; } return $n; }
function put(array<int, int> &$m): void { $m[9] = 7; }
function boxed(array $any): int { return first($any) + Keeper::second($any); }
function run(int $n): void
{
    $ids = [4, 5];
    $hash = [3 => 1, 0 => 2];
    $name = 'first';
    echo call_user_func('first', $ids), $name($ids), call_user_func_array('last', [$ids]), "|";
    $chosen = $n > 0 ? first(...) : last(...);
    echo $chosen($ids), $chosen($hash), call_user_func_array($chosen, [$ids]), "|";
    $keeper = new Keeper($ids);
    $method = [$keeper, 'sum'];
    $static = ['Keeper', 'second'];
    echo $method($ids), call_user_func($method, $ids), call_user_func_array($method, [$ids]), "|";
    echo $static($ids), call_user_func($static, $ids), call_user_func_array($static, [$ids]), "|";
    $defaulted = $n > 0 ? withDefault(...) : otherDefault(...);
    echo $defaulted(1), $defaulted(1, [2, 3]), "|";
    $objects = [new Item(1), new Item(2)];
    $itemFn = $n > 0 ? items(...) : noItems(...);
    echo $itemFn($objects), count($objects), $objects[1]->v, "|";
    $variadic = collect(...);
    echo $variadic($ids, [7]), call_user_func('collect', $ids), "|";
    $writer = put(...);
    $target = [4, 5];
    $writer($target);
    echo count($target), $target[9], "|";
    try { echo call_user_func_array([$keeper, 'fail'], [$ids]); } catch (Exception $e) { echo $e->getMessage(), "|"; }
    echo boxed($ids), boxed([1 => 6, 0 => 3]), "|";
    echo count($ids), $ids[1], count($hash), count($keeper->kept);
}
run(1);
"#;
    assert_eq!(
        run_clean(source),
        "778|757|777|888|86|422|167|37|thrown2|1515|2522"
    );
}
