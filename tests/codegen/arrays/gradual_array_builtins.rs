//! Purpose:
//! Regression tests for the array builtins whose operands or flags php settles when the call runs.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is verbatim `php -n` (PHP 8.5) output for the same fixture.
//! - Before this suite `sort($a, $flags)`, `array_unique($a, $flags)`, `array_pad()` over a
//!   `mixed` array, `array_reverse($mixed, true)`, `array_diff_assoc()` over a `mixed` operand,
//!   `array_udiff()` with a `callable` comparator, a spread into `array_map()` / `range()` /
//!   `array_intersect()`, `array_merge_recursive()` over three arrays, and an `array_merge()` fed
//!   the result of the method it sits in were refused at compile time.
//! - Each fixture also asserts the managed heap ends clean.

use crate::support::compile_and_run_with_heap_debug;

/// Compiles `source`, runs it and asserts php's output and a clean heap.
fn assert_matches_php(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Compiles `source`, runs it and asserts php's output only.
///
/// For fixtures whose shape (`$r = []; $r = array_merge($r, call()); $r[] = $x;`) leaks the
/// widened empty accumulator independently of this suite's builtins.
fn assert_matches_php_unchecked_heap(source: &str, expected: &str) {
    let output = crate::support::compile_and_run(source);
    assert_eq!(output, expected);
}

/// sort(), rsort(), asort() and arsort() order under every `$flags` comparison, literal or computed.
#[test]
fn test_sort_family_applies_flags_like_php() {
    assert_matches_php(
        r##"<?php
function show(array $a): void { echo json_encode($a), "\n"; }
$strings = ["10", "9", "2", "1", "b", "B", "a"];
$modes = [SORT_REGULAR, SORT_NUMERIC, SORT_STRING, SORT_LOCALE_STRING, SORT_NATURAL, SORT_STRING | SORT_FLAG_CASE, SORT_NATURAL | SORT_FLAG_CASE];
foreach ($modes as $mode) {
    $a = $strings; sort($a, $mode); echo $mode, ": "; show($a);
    $a = $strings; rsort($a, $mode); echo $mode, ": "; show($a);
}
$images = ["img12.png", "img10.png", "IMG2.png", "img1.png"];
sort($images, SORT_NATURAL | SORT_FLAG_CASE); show($images);
sort($images, SORT_NATURAL); show($images);
$keyed = ["x" => "10", "y" => "9", "z" => "1", "w" => "b"];
foreach ($modes as $mode) {
    $a = $keyed; asort($a, $mode); echo $mode, ": "; show($a);
    $a = $keyed; arsort($a, $mode); echo $mode, ": "; show($a);
}
$ints = [3, 10, 2, 33, 1];
sort($ints, SORT_STRING); show($ints);
rsort($ints, SORT_STRING); show($ints);
sort($ints, SORT_NUMERIC); show($ints);
// a flag only known at run time, and one with a side effect
$chosen = $argc > 5 ? SORT_NUMERIC : SORT_STRING;
$a = $strings; sort($a, $chosen); show($a);
$calls = 0;
function flag(int &$calls): int { $calls++; return SORT_STRING; }
$a = $strings; sort($a, flag($calls)); echo $calls, "\n";
$a = $strings; sort($a, flags: SORT_NUMERIC); show($a);
$a = $strings; sort($a, SORT_REGULAR); show($a);
var_dump(sort($a, SORT_STRING));
class Box { public array $items = ["b10", "b9", "b1"]; }
$box = new Box; sort($box->items, SORT_NATURAL); show($box->items);
"##,
        r##"0: ["1","2","9","10","B","a","b"]
0: ["b","a","B","10","9","2","1"]
1: ["b","B","a","1","2","9","10"]
1: ["10","9","2","1","b","B","a"]
2: ["1","10","2","9","B","a","b"]
2: ["b","a","B","9","2","10","1"]
5: ["1","10","2","9","B","a","b"]
5: ["b","a","B","9","2","10","1"]
6: ["1","2","9","10","B","a","b"]
6: ["b","a","B","10","9","2","1"]
10: ["1","10","2","9","a","b","B"]
10: ["b","B","a","9","2","10","1"]
14: ["1","2","9","10","a","b","B"]
14: ["b","B","a","10","9","2","1"]
["img1.png","IMG2.png","img10.png","img12.png"]
["IMG2.png","img1.png","img10.png","img12.png"]
0: {"z":"1","y":"9","x":"10","w":"b"}
0: {"w":"b","x":"10","y":"9","z":"1"}
1: {"w":"b","z":"1","y":"9","x":"10"}
1: {"x":"10","y":"9","z":"1","w":"b"}
2: {"z":"1","x":"10","y":"9","w":"b"}
2: {"w":"b","y":"9","x":"10","z":"1"}
5: {"z":"1","x":"10","y":"9","w":"b"}
5: {"w":"b","y":"9","x":"10","z":"1"}
6: {"z":"1","y":"9","x":"10","w":"b"}
6: {"w":"b","x":"10","y":"9","z":"1"}
10: {"z":"1","x":"10","y":"9","w":"b"}
10: {"w":"b","y":"9","x":"10","z":"1"}
14: {"z":"1","y":"9","x":"10","w":"b"}
14: {"w":"b","x":"10","y":"9","z":"1"}
[1,10,2,3,33]
[33,3,2,10,1]
[1,2,3,10,33]
["1","10","2","9","B","a","b"]
1
["b","B","a","1","2","9","10"]
["1","2","9","10","B","a","b"]
bool(true)
["b1","b9","b10"]
"##,
    );
}

/// array_unique() compares under `$flags`, with SORT_REGULAR the Laravel default.
#[test]
fn test_array_unique_flags_compare_like_php() {
    assert_matches_php(
        r##"<?php
function show(array $a): void { echo json_encode($a), "\n"; }
show(array_unique([1, "1", 2, 2.0, "a", "A", "a"], SORT_REGULAR));
show(array_unique(["10", "1e1", 10, "abc", "ABC"], SORT_NUMERIC));
show(array_unique(["10", "1e1", 10, "abc", "ABC"], SORT_STRING));
show(array_unique(["a", "A", "b", "B", "a"], SORT_STRING | SORT_FLAG_CASE));
show(array_unique(["img1", "IMG1", "img01"], SORT_NATURAL | SORT_FLAG_CASE));
show(array_unique(["x" => 1, "y" => "1", "z" => 2], SORT_REGULAR));
$flag = $argc > 5 ? SORT_STRING : SORT_REGULAR;
show(array_unique(["a", "b", "a", 0, "0"], $flag));
show(array_unique([3, 1, 3, 2, 1]));
show(array_unique([3, 1, 3, 2, 1], SORT_STRING));
show(array_unique([], SORT_REGULAR));
class Bag {
    protected $items = [];
    public function __construct($items) { $this->items = $items; }
    public function unique() { return array_unique($this->items, SORT_REGULAR); }
}
show((new Bag([1, 1, "1", [1], [1], "x"]))->unique());
"##,
        r##"{"0":1,"2":2,"4":"a","5":"A"}
{"0":"10","3":"abc"}
{"0":"10","1":"1e1","3":"abc","4":"ABC"}
{"0":"a","2":"b"}
{"0":"img1","2":"img01"}
{"x":1,"z":2}
{"0":"a","1":"b","3":0}
{"0":3,"1":1,"3":2}
{"0":3,"1":1,"3":2}
[]
{"0":1,"3":[1],"5":"x"}
"##,
    );
}

/// array_pad(), array_reverse(), the _assoc set operations, the comparator set operations, array_merge_recursive() and a self-recursive array_merge() accept operands only known at run time.
#[test]
fn test_gradual_array_operands_are_checked_at_run_time() {
    assert_matches_php(
        r##"<?php
class Items {
    protected $items = [];
    public function __construct($items = []) { $this->items = $this->arrayable($items); }
    protected function arrayable($items) {
        if (is_array($items)) { return $items; }
        return (array) $items;
    }
    public function pad($size, $value) { return array_pad($this->items, $size, $value); }
    public function reverse() { return array_reverse($this->items, true); }
    public function reverseWith($keep) { return array_reverse($this->items, $keep); }
    public function diffAssoc($other) { return array_diff_assoc($this->items, $this->arrayable($other)); }
    public function intersectAssoc($other) { return array_intersect_assoc($this->items, $this->arrayable($other)); }
    public function diffUsing($other, callable $callback) { return array_udiff($this->items, $this->arrayable($other), $callback); }
    public function intersectUsing($other, callable $callback) { return array_uintersect($this->items, $this->arrayable($other), $callback); }
}
function show($value): void { echo json_encode($value), "\n"; }
$mixed = new Items(['a' => 1, 'b' => [2], 3, 4]);
show($mixed->pad(6, 0)); show($mixed->pad(-6, 'p')); show($mixed->pad(2, 0));
show($mixed->reverse()); show($mixed->reverseWith(false)); show($mixed->reverseWith(true));
$list = new Items([1, 2, 3, 4]);
show($list->reverse());
show($list->diffAssoc([1, 5, 3])); show($list->intersectAssoc([1, 5, 3]));
show($list->diffAssoc(['a' => 1, 1 => 2]));
show($list->diffUsing([2, 3], fn ($a, $b) => $a <=> $b));
show($list->intersectUsing([2, 3], fn ($a, $b) => $a <=> $b));
$words = new Items(['x' => 'apple', 'y' => 'pear', 'z' => 'plum']);
show($words->diffAssoc(['x' => 'apple', 'y' => 'fig']));
show($words->intersectAssoc(['x' => 'apple', 'y' => 'fig']));
show($words->diffUsing(['PEAR'], 'strcasecmp'));
show(array_merge_recursive());
show(array_merge_recursive(['a' => [1], 'b' => 2]));
show(array_merge_recursive(['a' => [1]], ['a' => [2]], ['a' => [3], 'b' => 4]));
show(array_merge_recursive(['a' => 1], ['a' => 2], ['a' => 3], ['a' => 4]));
"##,
        r##"{"a":1,"b":[2],"0":3,"1":4,"2":0,"3":0}
{"0":"p","1":"p","a":1,"b":[2],"2":3,"3":4}
{"a":1,"b":[2],"0":3,"1":4}
{"1":4,"0":3,"b":[2],"a":1}
{"0":4,"1":3,"b":[2],"a":1}
{"1":4,"0":3,"b":[2],"a":1}
{"3":4,"2":3,"1":2,"0":1}
{"1":2,"3":4}
{"0":1,"2":3}
{"0":1,"2":3,"3":4}
{"0":1,"3":4}
{"1":2,"2":3}
{"y":"pear","z":"plum"}
{"x":"apple"}
{"x":"apple","z":"plum"}
[]
{"a":[1],"b":2}
{"a":[1,2,3],"b":4}
{"a":[1,2,3,4]}
"##,
    );
}

/// An untyped method that array_merge()s its own result into an accumulator compiles (its return type was the int seed while its body was being checked).
#[test]
fn test_self_recursive_untyped_method_feeds_array_merge() {
    assert_matches_php_unchecked_heap(
        r##"<?php
function show($value): void { echo json_encode($value), "\n"; }
class Walker {
    protected function resolve($manifest, $chunk, $seen = []) {
        $imports = [];
        foreach ($chunk['imports'] ?? [] as $import) {
            if (isset($seen[$import])) { continue; }
            $seen[$import] = true;
            $imports[] = $import;
            if (isset($manifest[$import])) {
                $imports = array_merge($imports, $this->resolve($manifest, $manifest[$import], $seen));
            }
        }
        return $imports;
    }
    public function go() {
        return $this->resolve(['a' => ['imports' => ['b']], 'b' => ['imports' => ['c']], 'c' => ['imports' => []]], ['imports' => ['a']]);
    }
    public static function group($name, $groups) {
        $results = [];
        foreach ($groups[$name] as $member) {
            if (isset($groups[$member])) {
                $results = array_merge($results, static::group($member, $groups));
                continue;
            }
            $results[] = $member;
        }
        return $results;
    }
}
show((new Walker)->go());
show(Walker::group('web', ['web' => ['auth', 'api'], 'api' => ['throttle', 'bindings']]));
"##,
        r##"["a","b","c"]
["auth","throttle","bindings"]
"##,
    );
}

/// array_map(), array_intersect(), range() and array_merge_recursive() take a spread of unknown length.
#[test]
fn test_spread_of_unknown_length_into_array_builtins() {
    assert_matches_php(
        r##"<?php
function show($value): void { echo json_encode($value), "\n"; }
function mapAll(array $params) { return array_map(...$params); }
show(mapAll([fn ($a) => $a * 2, [1, 2, 3]]));
show(mapAll([fn ($a, $b) => $a . $b, ['x', 'y'], [1, 2]]));
show(mapAll([fn (...$all) => count($all), [1, 2, 3], [4, 5], [6]]));
show(mapAll([null, [1, 2], ['a', 'b']]));
function intersectAll(array $sets) { return array_intersect(...array_values($sets)); }
show(intersectAll([[1, 2, 3, 4], [2, 3, 4], [3, 4, 5]]));
show(intersectAll([['a' => 'x', 'b' => 'y'], ['y', 'z']]));
show(intersectAll([[1, 2]]));
function ranges(string $spec) { return range(...explode('-', $spec)); }
show(ranges('1-3'));
show(ranges('7-10'));
show(ranges('a-e'));
show(ranges('5-5'));
$stepped = [2, 10, 4];
show(range(...$stepped));
function mergeAll(array $sets) { return array_merge_recursive(...$sets); }
show(mergeAll([['a' => [1]], ['a' => [2]], ['b' => 3]]));
show(mergeAll([]));
$plain = [fn ($x) => $x + 1, [1, 2]];
show(array_map(...$plain));
function failures() {
    $one = [fn ($x) => $x, 5];
    $two = [fn ($x, $y) => $x, [1], 5];
    $none = [];
    $scalarFirst = [5, [1]];
    $scalarSecond = [[1], 5];
    $single = [1];
    $zero = [1, 2, 0];
    $cases = [
        fn () => array_map(...$one),
        fn () => array_map(...$two),
        fn () => array_map(...$none),
        fn () => array_intersect(...$scalarFirst),
        fn () => array_intersect(...$scalarSecond),
        fn () => array_intersect(...$none),
        fn () => range(...$single),
        fn () => range(...$zero),
        fn () => array_merge_recursive(...$scalarSecond),
    ];
    foreach ($cases as $case) {
        try { $case(); echo "no error\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
    }
}
failures();
$floats = [0, 1, 0.25];
show(range(...$floats));
$down = [1.5, -1.5, 1];
show(range(...$down));
"##,
        r##"[2,4,6]
["x1","y2"]
[3,3,3]
[[1,"a"],[2,"b"]]
{"2":3,"3":4}
{"b":"y"}
[1,2]
["1","2","3"]
[7,8,9,10]
["a","b","c","d","e"]
["5"]
[2,6,10]
{"a":[1,2],"b":3}
[]
[2,3]
TypeError: array_map(): Argument #2 ($array) must be of type array, int given
TypeError: array_map(): Argument #3 must be of type array, int given
ArgumentCountError: array_map() expects at least 2 arguments, 0 given
TypeError: array_intersect(): Argument #1 ($array) must be of type array, int given
TypeError: array_intersect(): Argument #2 must be of type array, int given
ArgumentCountError: array_intersect() expects at least 1 argument, 0 given
ArgumentCountError: range() expects at least 2 arguments, 1 given
ValueError: range(): Argument #3 ($step) cannot be 0
TypeError: array_merge_recursive(): Argument #2 must be of type array, int given
[0,0.25,0.5,0.75,1]
[1.5,0.5,-0.5,-1.5]
"##,
    );
}

/// A non-array operand raises php's own TypeError when the call runs, not a compile-time refusal.
#[test]
fn test_gradual_array_operand_errors_are_phps() {
    assert_matches_php(
        r##"<?php
class Holder {
    public $value;
    public function __construct($value) { $this->value = $value; }
    public function pad() { return array_pad($this->value, 3, 0); }
    public function reverse() { return array_reverse($this->value, true); }
    public function diff($other) { return array_diff_assoc($this->value, $other); }
    public function inter($other) { return array_intersect_assoc($this->value, $other); }
    public function unique() { return array_unique($this->value, SORT_REGULAR); }
    public function merge() { return array_merge_recursive($this->value, [1], [2]); }
}
function attempt(callable $call): void {
    try { $call(); echo "no error\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
}
$known = new Holder([1]);
foreach ([5, "text", null] as $bad) {
    $holder = new Holder($bad);
    attempt(fn () => $holder->pad());
    attempt(fn () => $holder->reverse());
    attempt(fn () => $holder->diff([1]));
    attempt(fn () => $known->diff($bad));
    attempt(fn () => $known->inter($bad));
    attempt(fn () => $holder->unique());
    attempt(fn () => $holder->merge());
}
function lengths($source) {
    if ($source instanceof Traversable) { $source = iterator_to_array($source); }
    return $source;
}
echo json_encode(lengths(new ArrayIterator([1, 2]))), json_encode(lengths("plain")), "\n";
function scalarSource(string $value) { return iterator_to_array($value); }
attempt(fn () => scalarSource("x"));
"##,
        r##"TypeError: array_pad(): Argument #1 ($array) must be of type array, int given
TypeError: array_reverse(): Argument #1 ($array) must be of type array, int given
TypeError: array_diff_assoc(): Argument #1 ($array) must be of type array, int given
TypeError: array_diff_assoc(): Argument #2 must be of type array, int given
TypeError: array_intersect_assoc(): Argument #2 must be of type array, int given
TypeError: array_unique(): Argument #1 ($array) must be of type array, int given
TypeError: array_merge_recursive(): Argument #1 must be of type array, int given
TypeError: array_pad(): Argument #1 ($array) must be of type array, string given
TypeError: array_reverse(): Argument #1 ($array) must be of type array, string given
TypeError: array_diff_assoc(): Argument #1 ($array) must be of type array, string given
TypeError: array_diff_assoc(): Argument #2 must be of type array, string given
TypeError: array_intersect_assoc(): Argument #2 must be of type array, string given
TypeError: array_unique(): Argument #1 ($array) must be of type array, string given
TypeError: array_merge_recursive(): Argument #1 must be of type array, string given
TypeError: array_pad(): Argument #1 ($array) must be of type array, null given
TypeError: array_reverse(): Argument #1 ($array) must be of type array, null given
TypeError: array_diff_assoc(): Argument #1 ($array) must be of type array, null given
TypeError: array_diff_assoc(): Argument #2 must be of type array, null given
TypeError: array_intersect_assoc(): Argument #2 must be of type array, null given
TypeError: array_unique(): Argument #1 ($array) must be of type array, null given
TypeError: array_merge_recursive(): Argument #1 must be of type array, null given
[1,2]"plain"
TypeError: iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, string given
"##,
    );
}

/// iterator_to_array() takes an array, an Iterator, an IteratorAggregate or a generator held by an untyped property, a `mixed` or a union parameter, and raises php's TypeError for anything else.
#[test]
fn test_iterator_to_array_over_a_value_known_only_at_run_time() {
    assert_matches_php(
        r##"<?php
class Holder {
    public $src;
    public function __construct($src) { $this->src = $src; }
    public function all() { return iterator_to_array($this->src); }
    public function list() { return iterator_to_array($this->src, false); }
}
class Pair implements IteratorAggregate {
    public function getIterator(): Iterator { return new ArrayIterator(['p' => 1, 'q' => 2]); }
}
function generate() { yield 'a' => 1; yield 'b' => 2; }
function unioned(int|iterable $x, bool $keys) { return iterator_to_array($x, $keys); }
function gradual(mixed $x) { return iterator_to_array($x); }
foreach ([0, 1] as $keys) {
    foreach ([new ArrayIterator([1, 2, 3]), generate(), new Pair(), [5, 6], ['k' => 7, 8]] as $source) {
        $holder = new Holder($source);
        echo json_encode($keys ? $holder->all() : $holder->list()), "\n";
    }
}
echo json_encode(unioned(generate(), true)), json_encode(unioned([9, 8], true)), json_encode(unioned(['x' => 1], false)), "\n";
echo json_encode(gradual(new ArrayIterator(['x' => 1]))), "\n";
foreach ([5, 'text', null, 1.5, true] as $bad) {
    try {
        gradual($bad);
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
function sizes(iterable $x) {
    $r = iterator_to_array($x);
    foreach ($r as $k => $v) echo "$k=$v,";
    echo count($r), isset($r[1]) ? 'y' : 'n', "\n";
}
sizes([7, 8]);
sizes(new ArrayIterator([4, 5, 6]));
"##,
        r##"[1,2,3]
[1,2]
[1,2]
[5,6]
[7,8]
[1,2,3]
{"a":1,"b":2}
{"p":1,"q":2}
[5,6]
{"k":7,"0":8}
{"a":1,"b":2}[9,8][1]
{"x":1}
iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, int given
iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, string given
iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, null given
iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, float given
iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, true given
0=7,1=8,2y
0=4,1=5,2=6,3y
"##,
    );
}

/// pcntl_sigprocmask() fills its by-reference old-set argument and restores a saved mask.
#[test]
fn test_pcntl_sigprocmask_old_set_by_reference() {
    assert_matches_php(
        r##"<?php
$blocked = pcntl_sigprocmask(SIG_BLOCK, [SIGUSR1], $old);
var_dump($blocked, $old);
$sets = [SIGUSR2, SIGINT];
var_dump(pcntl_sigprocmask(SIG_BLOCK, $sets, $old2));
var_dump(count($old2) >= 1);
var_dump(pcntl_sigprocmask(SIG_SETMASK, $old, $o3));
var_dump(count($o3) >= 2);
var_dump(pcntl_sigprocmask(SIG_UNBLOCK, [SIGUSR1]));
"##,
        r##"bool(true)
array(0) {
}
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
"##,
    );
}


/// A float offset or length reaches `array_slice()` the way php's weak mode passes it: an
/// integral value as it stands, a fractional one with the "loses precision" deprecation, NaN with
/// the `TypeError` naming the nullable `?int` parameter.
///
/// Laravel's `Collection::split()` slices with `floor()`'s float; the backend refused the call,
/// which left `Collection` and every class built on it to the runtime autoloader. Values from
/// `php -n` 8.5.10.
#[test]
fn test_array_slice_takes_float_offset_and_length_like_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function split(array $items, int $numberOfGroups): array {
    $groups = [];
    $groupSize = floor(count($items) / $numberOfGroups);
    $remain = count($items) % $numberOfGroups;
    $start = 0;
    for ($i = 0; $i < $numberOfGroups; $i++) {
        $size = $groupSize;
        if ($i < $remain) {
            $size++;
        }
        if ($size) {
            $groups[] = array_slice($items, $start, $size);
            $start += $size;
        }
    }
    return $groups;
}
echo json_encode(split([1, 2, 3, 4, 5, 6, 7], 3)), "\n";
echo json_encode(array_slice([1, 2, 3, 4], 1.0, 2.5)), "\n";
try {
    echo count(array_slice([1, 2], 0, NAN));
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "[[1,2,3],[4,5],[6,7]]\n[2,3]\narray_slice(): Argument #3 ($length) must be of type ?int, float given\n",
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("Implicit conversion from float 2.5 to int loses precision"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
