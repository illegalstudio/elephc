//! Purpose:
//! Regression tests for array_push on associative receivers and their reference places.
//!
//! Called from:
//! - The codegen integration test harness through the arrays module.
//!
//! Key details:
//! - Fixtures cover next-integer keys, COW, growth, null receivers and heap ownership.

use crate::support::*;

/// Live nested receiver promotion remains visible through returns and conditional control flow.
#[test]
fn test_associative_push_nested_parent_storage_survives_function_boundaries() {
    for (name, source, expected) in [
        ("indexed return and repeated push", r#"<?php
function makeIndexedPushRows() { $rows = [[1]]; array_push($rows[0], 2); array_push($rows[0], 3); return $rows; }
$rows = makeIndexedPushRows(); echo $rows[0][0], "|", $rows[0][1], "|", json_encode($rows);
"#, "1|2|[[1,2,3]]"),
        ("hash return", r#"<?php
function makeHashPushRows() { $rows = ["inner" => [3]]; array_push($rows["inner"], 4); return $rows; }
$rows = makeHashPushRows(); echo $rows["inner"][0], "|", $rows["inner"][1], "|", json_encode($rows);
"#, "3|4|{\"inner\":[3,4]}"),
        ("native reference alias", r#"<?php
function makeAliasedPushRows() { $rows = [[9]]; $alias =& $rows; array_push($rows[0], 10); return $alias; }
echo json_encode(makeAliasedPushRows());
"#, "[[9,10]]"),
        ("hash reference alias", r#"<?php
function makeAliasedHashPushRows() {
    $rows = ["row" => ["value" => 1], "other" => ["value" => 7]];
    $alias =& $rows; array_push($rows["row"], 2); return $alias;
}
$rows = makeAliasedHashPushRows(); echo $rows["row"][0], "|", $rows["other"]["value"];
"#, "2|7"),
        ("callee read", r#"<?php
function showPromotedPushRows($rows) { echo $rows[0][0]; }
$rows = [[11]]; array_push($rows[0], 12); showPromotedPushRows($rows);
"#, "11"),
    ] {
        eprintln!("nested push boundary case: {name}");
        let out = compile_and_run_with_heap_debug(source);
        assert_eq!(out.stdout, expected, "{name}, stderr: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{name}: {}", out.stderr);
    }
}

/// Conditional push promotion agrees with return inference on executed and skipped paths.
#[test]
fn test_associative_push_nested_parent_storage_survives_control_flow() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function makeConditionalPushRows(bool $push) {
    $rows = [[5]];
    if ($push) { array_push($rows[0], 6); }
    return $rows;
}
function makeShortCircuitPushRows(bool $push) {
    $rows = [[7]];
    $push && array_push($rows[0], 8);
    return $rows;
}
function makeLoopPushRows(int $count) {
    $rows = [[9]];
    for ($i = 0; $i < $count; $i++) { array_push($rows[0], 10); }
    return $rows;
}
echo json_encode(makeConditionalPushRows($argc > 0)), "|", json_encode(makeConditionalPushRows($argc < 0)), "\n";
echo json_encode(makeShortCircuitPushRows($argc > 0)), "|", json_encode(makeShortCircuitPushRows($argc < 0)), "\n";
echo json_encode(makeLoopPushRows(2)), "|", json_encode(makeLoopPushRows(0)), "\n";
"#);
    assert_eq!(out.stdout, "[[5,6]]|[[5]]\n[[7,8]]|[[7]]\n[[9,10,10]]|[[9]]\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The full `array_push()` surface over an associative receiver.
///
/// Iteration verifies the mixed integer/string key sequence as well as the values.
#[test]
fn test_array_push_appends_to_an_associative_receiver() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => 1, "b" => 2]; $n = \ARRAY_PUSH($a, 3);       echo implode(",", $a), "|", $n, "\n";
$b = ["a" => 1];           $n = array_push($b, 3, 4, 5); echo implode(",", $b), "|", $n, "\n";
$c = ["a" => 1];           $n = array_push($c);          echo implode(",", $c), "|", $n, "\n";
$d = [5 => "x", "k" => "y"];  array_push($d, "z");       foreach ($d as $k => $v) { echo $k, "="; } echo "\n";
$e = [-3 => "a", 7 => "b"];   array_push($e, "c");       foreach ($e as $k => $v) { echo $k, "="; } echo "\n";
function viaRef(array &$r): int { return array_push($r, "new"); }
$p = ["k" => "v"]; $n = viaRef($p); echo implode(",", $p), "|", $n, "\n";
class PushBox { public array $items = ["a" => 1]; }
$box = new PushBox(); $n = array_push($box->items, 2); echo implode(",", $box->items), "|", $n, "\n";
class PushShelf { public static array $items = ["a" => 1]; }
$n = array_push(PushShelf::$items, 2); echo implode(",", PushShelf::$items), "|", $n, "\n";
$rows = ["inner" => ["a" => 1]]; $n = array_push($rows["inner"], 2); echo implode(",", $rows["inner"]), "|", $n, "\n";
$g = ["a" => 1]; echo array_push($g, 2, 3) + 10, "\n";
$loop = ["k" => "v"]; for ($i = 0; $i < 5; $i++) { array_push($loop, (string) $i); }
foreach ($loop as $k => $v) { echo $k, "="; } echo "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "1,2,3|3\n",
            "1,3,4,5|4\n",
            "1|1\n",
            "5=k=6=\n",
            "-3=7=8=\n",
            "v,new|2\n",
            "1,2|2\n",
            "1,2|2\n",
            "1,2|2\n",
            "13\n",
            "k=0=1=2=3=4=\n",
        )
    );
}

/// The receiver is republished after EVERY insert, not once at the end.
///
/// A hash insert can split the table for copy-on-write, so the pointer a later insert must
/// address is the previous insert's result. Publishing once at the end wrote the pre-split
/// pointer back and the appends vanished, `count()` stayed at its original value while the
/// copy was, correctly, left alone. Both directions are asserted because only the pair
/// distinguishes a lost write from a split that never happened.
#[test]
fn test_array_push_on_a_shared_associative_receiver_separates_it() {
    let out = compile_and_run(
        r#"<?php
$h = ["a" => 1]; $b = $h; array_push($h, 2); echo count($h), ",", count($b), "\n";
$i = ["a" => 1]; $c = $i; array_push($c, 2); echo count($i), ",", count($c), "\n";
"#,
    );
    assert_eq!(out, "2,1\n1,2\n");
}

/// Appending to an associative receiver in a loop leaves no heap behind.
///
/// Each insert goes through `__rt_hash_set`, which owns the growth and the copy-on-write split,
/// and the caller republishes the table pointer afterwards. A missed release on the split-away
/// table accumulates per iteration rather than showing up once.
#[test]
fn test_array_push_on_an_associative_receiver_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 32; $i++) {
    $a = ["x" => 1, "y" => 2];
    array_push($a, 3, 4, 5);
    $b = ["x" => "aa"];
    $c = $b;
    array_push($b, "bb", "cc");
}
echo count($a), count($b), count($c), "\n";
"#,
    );
    assert_eq!(out.stdout, "531\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "associative array_push leaked: {}",
        out.stderr
    );
}

/// Enough values to force `__rt_hash_grow`, which FREES the old table rather than merely
/// decrementing it.
///
/// A fresh one-entry literal is allocated with sixteen slots and grows once the count reaches
/// three quarters of that, so a five-value push never reaches the path at all: the earlier
/// fixtures exercise only the copy-on-write split. Growth is where a stale republished pointer
/// is a use-after-free instead of a stale read, so the shared receiver is included to make the
/// sequence split-then-grow rather than grow alone.
#[test]
fn test_array_push_growing_an_associative_receiver_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["a" => 1];
$n = array_push($a, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21);
$b = ["a" => 1];
$shared = $b;
$m = array_push($b, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21);
class GrowBox { public array $h = ["a" => 1]; }
$o = new GrowBox();
$p = array_push($o->h, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21);
echo $n, ",", $m, ",", count($shared), ",", $p, "\n";
"#,
    );
    assert_eq!(out.stdout, "21,21,1,21\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "growing an associative receiver leaked: {}",
        out.stderr
    );
}

/// A receiver that is a MISSED hash read is the in-band null-container sentinel, not a table.
///
/// An `AssocArray`-typed value can hold that sentinel at run time, and dereferencing it
/// segfaulted, in the key scan when values were pushed, and in the count read when they were
/// not, since with no values no insert happens and there is nothing to have made the receiver
/// real. PHP answers with a `TypeError`, and so does this now, with php-src's wording.
#[test]
fn test_array_push_on_a_missed_associative_read_raises_a_type_error() {
    let out = compile_and_run(
        r#"<?php
$h = ["a" => ["x" => 1]];
try { array_push($h["missing"], 2); } catch (TypeError $e) { echo "with:", $e->getMessage(), "\n"; }
try { array_push($h["absent"]); } catch (TypeError $e) { echo "without:", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        concat!(
            "with:array_push(): Argument #1 ($array) must be of type array, null given\n",
            "without:array_push(): Argument #1 ($array) must be of type array, null given\n",
        )
    );
}

/// All argument values observe the receiver before this call appends its first value.
#[test]
fn test_associative_push_evaluates_values_before_mutation() {
    let out = compile_and_run(
        r#"<?php
$h = ["a" => 1];
echo array_push($h, 2, count($h)), "|", json_encode($h), "\n";
$nested = ["a" => 1];
$copy = $nested;
echo array_push($nested, array_push($nested, 2, 3), count($nested)), "|";
echo json_encode($nested), "|", json_encode($copy), "\n";
"#,
    );
    assert_eq!(out, concat!(
        "3|{\"a\":1,\"0\":2,\"1\":1}\n",
        "5|{\"a\":1,\"0\":2,\"1\":3,\"2\":3,\"3\":3}|{\"a\":1}\n",
    ));
}

/// Every insertion through a reference alias keeps a balanced consuming owner and COW copy.
#[test]
fn test_associative_push_reference_alias_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 32; $i++) {
    $h = ["a" => 1];
    $alias =& $h;
    $copy = $h;
    array_push($alias, 2, 3, 4);
}
echo count($h), ",", count($alias), ",", count($copy), "\n";
"#,
    );
    assert_eq!(out.stdout, "4,4,1\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Named zero-value calls and positional unpacks share the associative append path.
#[test]
fn test_associative_push_named_and_unpacked_values() {
    let out = compile_and_run(
        r#"<?php
$h = ["a" => 1];
echo ARRAY_PUSH(array: $h), "|";
echo array_push($h, ...[2, 3]), "|", json_encode($h), "\n";
"#,
    );
    assert_eq!(out, "1|3|{\"a\":1,\"0\":2,\"1\":3}\n");
}

/// A temporary call result can provide an indexed receiver, matching PHP's reference semantics.
#[test]
fn test_array_push_temporary_indexed_receiver_returns_count() {
    let out = compile_and_run(r#"<?php
function temporaryPushRows(): array<array<int>> { return [[1]]; }
echo array_push(temporaryPushRows()[0], 2);
"#);
    assert_eq!(out, "2");
}

/// Local buffer allocation and factories preserve native property storage and release its owners.
#[test]
fn test_array_push_native_property_accepts_local_buffer_factories() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function localPushPage(int $n, int $start): buffer<int> {
    $page = buffer_new<int>($n);
    for ($i = 0; $i < $n; $i++) { $page[$i] = $start + $i; }
    return $page;
}
function blankPushPage(int $n): buffer<int> { return buffer_new<int>($n); }
class LocalBufferPushPool { public array<buffer<int> > $pages = []; }
$pool = new LocalBufferPushPool();
$length = $argc > 0 ? 2 : 1;
echo array_push($pool->pages, buffer_new<int>(1), localPushPage(2, 30), blankPushPage($length)), "|";
echo buffer_len($pool->pages[0]), "|", $pool->pages[1][1], "|", buffer_len($pool->pages[2]), "\n";
foreach ($pool->pages as $page) { buffer_free($page); }
unset($pool);
"#);
    assert_eq!(out.stdout, "3|1|31|2\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Mixed-entry tables preserve the actual tags of scalar, null and nested-array values.
#[test]
fn test_associative_push_mixed_values_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 32; $i++) {
    $h = ["a" => 1, "b" => "x"];
    $copy = $h;
    array_push($h, 2.5, false, null, ["n" => 3]);
}
echo json_encode($h), "|", json_encode($copy), "\n";
"#,
    );
    assert_eq!(out.stdout,
        "{\"a\":1,\"b\":\"x\",\"0\":2.5,\"1\":false,\"2\":null,\"3\":{\"n\":3}}|{\"a\":1,\"b\":\"x\"}\n",
        "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Append history survives deleted keys and a key-exhaustion Error without leaking references.
#[test]
fn test_associative_push_append_history_and_errors_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$h = ["a" => 1, 9 => 2];
unset($h[9]);
array_push($h, 3);
echo json_encode(array_keys($h)), "\n";
$negative = ["a" => 1, -5 => 2];
unset($negative[-5]);
array_push($negative, 3);
echo json_encode(array_keys($negative)), "\n";
$limit = ["a" => 1, PHP_INT_MAX - 1 => 2];
$alias =& $limit;
$copy = $limit;
try { array_push($alias, 3, 4); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
echo count($limit), ",", count($copy), ",", $limit[PHP_INT_MAX], "\n";
"#,
    );
    assert_eq!(out.stdout, concat!(
        "[\"a\",10]\n",
        "[\"a\",-4]\n",
        "Cannot add element to the array as the next element is already occupied\n",
        "3,2,3\n",
    ), "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime unpacks append each captured value, rather than appending their collector array.
#[test]
fn test_associative_push_runtime_unpack_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function runtimePushTail(int $n): array { return [$n, $n + 1]; }
for ($i = 0; $i < 16; $i++) {
    $h = ["a" => 1, "b" => "x"];
    $tail = [2, 3];
    $copy = $h;
    $n = array_push($h, ...$tail);
    $m = array_push($h, 4, ...runtimePushTail(5), ...[]);
    $sparse = [7 => 7, 9 => 8];
    array_push($h, ...$sparse);
}
echo $n, ",", $m, "|", json_encode($h), "|", json_encode($copy), "\n";
"#);
    assert_eq!(out.stdout,
        "4,7|{\"a\":1,\"b\":\"x\",\"0\":2,\"1\":3,\"2\":4,\"3\":5,\"4\":6,\"5\":7,\"6\":8}|{\"a\":1,\"b\":\"x\"}\n",
        "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime unpack classifies canonical integer strings as positional for generators and iterators.
#[test]
fn test_associative_push_runtime_unpack_numeric_string_keys_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function numericStringKeyGenerator(): Generator {
    yield "10" => 7;
    yield "-2" => 8;
    yield "0" => 9;
    yield "-9223372036854775808" => 10;
}
$generated = ["a" => 1, "b" => "x"];
echo array_push($generated, ...numericStringKeyGenerator()), "|", json_encode($generated), "\n";
class NumericStringKeyBox { public array $values = ["a" => 1, "b" => "x"]; }
$box = new NumericStringKeyBox();
echo array_push($box->values, ...numericStringKeyGenerator()), "|", json_encode($box->values), "\n";
class NumericStringKeyIterator implements Iterator {
    private int $position = 0;
    public function rewind(): void { $this->position = 0; }
    public function valid(): bool { return $this->position < 3; }
    public function current(): mixed { return 20 + $this->position; }
    public function key(): mixed { return ["10", "-2", "0"][$this->position]; }
    public function next(): void { $this->position = $this->position + 1; }
}
function opaqueNumericStringKeyIterator(): mixed { return new NumericStringKeyIterator(); }
$iterated = ["a" => 1, "b" => "x"];
echo array_push($iterated, ...opaqueNumericStringKeyIterator()), "|", json_encode($iterated), "\n";
function invalidStringKeyGenerator(string $key): Generator { yield $key => 99; }
foreach (["01", "+10", "9223372036854775808", "-9223372036854775809"] as $key) {
    $unchanged = ["a" => 1, "b" => "x"];
    try { array_push($unchanged, ...invalidStringKeyGenerator($key)); }
    catch (ArgumentCountError $error) {
        echo $key, "|", $error->getMessage(), "|", json_encode($unchanged), "\n";
    }
}
"#);
    assert_eq!(out.stdout, concat!(
        "6|{\"a\":1,\"b\":\"x\",\"0\":7,\"1\":8,\"2\":9,\"3\":10}\n",
        "6|{\"a\":1,\"b\":\"x\",\"0\":7,\"1\":8,\"2\":9,\"3\":10}\n",
        "5|{\"a\":1,\"b\":\"x\",\"0\":20,\"1\":21,\"2\":22}\n",
        "01|array_push() does not accept unknown named parameters|{\"a\":1,\"b\":\"x\"}\n",
        "+10|array_push() does not accept unknown named parameters|{\"a\":1,\"b\":\"x\"}\n",
        "9223372036854775808|array_push() does not accept unknown named parameters|{\"a\":1,\"b\":\"x\"}\n",
        "-9223372036854775809|array_push() does not accept unknown named parameters|{\"a\":1,\"b\":\"x\"}\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Later arguments update the selected live property, static property or element before pushing.
#[test]
fn test_associative_push_nonlocal_argument_effects_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class LivePushBox { public array $h = ["a" => 1]; }
$o = new LivePushBox();
$n = array_push($o->h, (function () use ($o) { array_push($o->h, 5); return 9; })(), 8);
echo $n, "|", json_encode($o->h), "\n";
class LivePushStatic { public static array $h = ["a" => 1]; }
$n = array_push(LivePushStatic::$h, (function () { array_push(LivePushStatic::$h, 5); return 9; })(), 8);
echo $n, "|", json_encode(LivePushStatic::$h), "\n";
$rows = ["inner" => ["a" => 1]];
$n = array_push($rows["inner"], (function () use (&$rows) { array_push($rows["inner"], 5); return 9; })(), 8);
echo $n, "|", json_encode($rows["inner"]), "\n";
"#);
    assert_eq!(out.stdout, "4|{\"a\":1,\"0\":5,\"1\":9,\"2\":8}\n".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Unpack failures leave the receiver unchanged and release captured values and iterator owners.
#[test]
fn test_associative_push_runtime_unpack_errors_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$h = ["a" => 1, "b" => "x"];
$named = ["bad" => 2];
try { array_push($h, 9, ...$named); }
catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
echo json_encode($h), "\n";
$sources = ["ok" => [2]];
try { array_push($h, ...$sources["missing"]); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo json_encode($h), "\n";
function invalidPushSource(): mixed { return new stdClass; }
try { array_push($h, ...invalidPushSource()); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo json_encode($h), "\n";
"#);
    assert_eq!(out.stdout, concat!(
        "array_push() does not accept unknown named parameters\n",
        "{\"a\":1,\"b\":\"x\"}\n",
        "Only arrays and Traversables can be unpacked, null given\n",
        "{\"a\":1,\"b\":\"x\"}\n",
        "Only arrays and Traversables can be unpacked, stdClass given\n",
        "{\"a\":1,\"b\":\"x\"}\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scoped receiver aliases keep object identity and the originally selected key.
#[test]
fn test_associative_push_scoped_place_selection_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class SelectedPushBox { public array $h = ["a" => 1]; }
$o = new SelectedPushBox();
$held = $o;
array_push($o->h, (function () use (&$o) { $o = new SelectedPushBox(); return 2; })());
echo json_encode($held->h), "|", json_encode($o->h), "\n";
$rows = ["inner" => ["a" => 1], "other" => ["b" => 9]];
$key = "inner";
array_push($rows[$key], (function () use (&$key) { $key = "other"; return 2; })());
echo json_encode($rows), "\n";
"#);
    assert_eq!(out.stdout, concat!(
        "{\"a\":1,\"0\":2}|{\"a\":1}\n",
        "{\"inner\":{\"a\":1,\"0\":2},\"other\":{\"b\":9}}\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Unpacking captures reference values before later argument effects and checks runtime names.
#[test]
fn test_associative_push_unpack_evaluation_order_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function laterPushTail(): array { echo "later|"; return [8]; }
$h = ["a" => 1, "b" => "x"];
$tail = [2];
$v =& $tail[0];
array_push($h, ...$tail, ...(function () use (&$v) { $v = 9; return [3]; })());
echo json_encode($h), "|", $v, "\n";
array_push($h, ...laterPushTail());
echo json_encode($h), "\n";
$unknown = ["bad" => 2];
try { array_push($h, ...$unknown, ...laterPushTail()); }
catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
$duplicate = ["array" => [2]];
try { array_push($h, ...$duplicate, ...laterPushTail()); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
$positional = ["bad" => 2, 0 => 3];
try { array_push($h, ...$positional); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
try { array_push($h, ...$unknown, ...$unknown, ...laterPushTail()); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
"#);
    assert_eq!(out.stdout, concat!(
        "{\"a\":1,\"b\":\"x\",\"0\":2,\"1\":3}|9\n",
        "later|{\"a\":1,\"b\":\"x\",\"0\":2,\"1\":3,\"2\":8}\n",
        "later|array_push() does not accept unknown named parameters\n",
        "Named parameter $array overwrites previous argument\n",
        "Cannot use positional argument after named argument during unpacking\n",
        "Named parameter $bad overwrites previous argument\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A caught append failure preserves earlier writes through a promoted native property.
#[test]
fn test_associative_push_property_partial_failure_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class LimitPushBox { public array $h = ["a" => 1]; }
$o = new LimitPushBox();
$o->h = ["a" => 1, PHP_INT_MAX - 1 => 2];
try { array_push($o->h, 3, 4); }
catch (Error $e) { echo $e->getMessage(), "|", count($o->h), ",", $o->h[PHP_INT_MAX], "\n"; }
"#);
    assert_eq!(out.stdout, "Cannot add element to the array as the next element is already occupied|3,3\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Replacing a parent preserves the leased element without adding to explicit-reference owners.
/// The same parent-replacement program with a persistent PHP reference already retains a
/// container graph, so compare that control instead of hiding the existing heap baseline.
#[test]
fn test_associative_push_parent_replacement_matches_reference_ownership_baseline() {
    let baseline = compile_and_run_with_heap_debug(r#"<?php
$rows = ["inner" => ["a" => 1]];
$r =& $rows["inner"];
$n = (function () use (&$rows) { $rows = ["inner" => ["z" => 9]]; return 2; })();
$r[] = $n;
unset($r);
echo json_encode($rows), "\n";
"#);
    let out = compile_and_run_with_heap_debug(r#"<?php
$rows = ["inner" => ["a" => 1]];
$n = array_push($rows["inner"], (function () use (&$rows) { $rows = ["inner" => ["z" => 9]]; return 2; })());
echo $n, "|", json_encode($rows), "\n";
"#);
    assert_eq!(baseline.stdout, "{\"inner\":{\"z\":9}}\n", "{}", baseline.stderr);
    assert_eq!(out.stdout, "2|{\"inner\":{\"z\":9}}\n", "{}", out.stderr);
    let summary = |stderr: &str| stderr.lines()
        .find(|line| line.starts_with("HEAP DEBUG: leak summary:"))
        .expect("heap debug must report its final ownership summary").to_owned();
    assert_eq!(summary(&out.stderr), summary(&baseline.stderr), "{}\ncontrol: {}", out.stderr, baseline.stderr);
}

/// Checker-selected classes survive opaque closure effects before a later partial append.
#[test]
fn test_associative_push_erased_receiver_class_preserves_partial_writes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ErasedPushBox { public array $h = ["a" => 1]; }
$o = new ErasedPushBox();
$held = $o;
array_push($o->h, (function () use (&$o) { $o = new ErasedPushBox(); return 2; })());
$o->h = ["a" => 1, PHP_INT_MAX - 1 => 2];
try { array_push($o->h, 3, 4); }
catch (Error $e) { echo count($o->h), ",", $o->h[PHP_INT_MAX], "\n"; }
"#);
    assert_eq!(out.stdout, "3,3\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Readonly property reference binding refuses direct, nested, zero-value and constructor calls.
#[test]
fn test_associative_push_readonly_property_errors_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function rejectedPushValue(): int { echo "value|"; return 2; }
function rejectedPushKey(): string { echo "key|"; return "inner"; }
class ReadonlyPushBox {
    public readonly array $h;
    public function __construct() { $this->h = ["inner" => [1]]; }
}
$o = new ReadonlyPushBox();
try { array_push($o->h, rejectedPushValue()); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
try { array_push($o->h[rejectedPushKey()], rejectedPushValue()); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
try { array_push($o->h); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
echo json_encode($o->h), "\n";
class UninitializedPushBox { public readonly array $h; }
$empty = new UninitializedPushBox();
try { array_push($empty->h, rejectedPushValue()); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
class ConstructorPushBox {
    public readonly array $h;
    public function __construct() {
        $this->h = ["a" => 1];
        try { array_push($this->h, rejectedPushValue()); }
        catch (Error $e) { echo $e->getMessage(), "\n"; }
    }
}
$inside = new ConstructorPushBox();
"#);
    assert_eq!(out.stdout, concat!(
        "Cannot indirectly modify readonly property ReadonlyPushBox::$h\n",
        "key|Cannot indirectly modify readonly property ReadonlyPushBox::$h\n",
        "Cannot indirectly modify readonly property ReadonlyPushBox::$h\n",
        "{\"inner\":[1]}\n",
        "Cannot indirectly modify readonly property UninitializedPushBox::$h\n",
        "Cannot indirectly modify readonly property ConstructorPushBox::$h\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Setter visibility guards indirect writes while class and descendant scopes keep legal access.
#[test]
fn test_associative_push_set_visibility_errors_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class PrivatePushBox {
    public private(set) array $h = ["a" => 1];
    public function add(): int { return array_push($this->h, 2); }
}
$o = new PrivatePushBox();
try { array_push($o->h, 3); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
echo $o->add(), "|", json_encode($o->h), "\n";
class ProtectedPushBox { public protected(set) array $h = ["a" => 1]; }
class DescendantPushBox extends ProtectedPushBox {
    public function add(): int { return array_push($this->h, 2); }
}
$child = new DescendantPushBox();
try { array_push($child->h, 3); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
echo $child->add(), "|", json_encode($child->h), "\n";
"#);
    assert_eq!(out.stdout, concat!(
        "Cannot indirectly modify private(set) property PrivatePushBox::$h from global scope\n",
        "2|{\"a\":1,\"0\":2}\n",
        "Cannot indirectly modify protected(set) property ProtectedPushBox::$h from global scope\n",
        "2|{\"a\":1,\"0\":2}\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// By-value property hooks reject indirect mutation after getter and dimension effects only.
#[test]
fn test_associative_push_hook_errors_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function hookedPushValue(): int { echo "value|"; return 3; }
function hookedPushKey(): string { echo "key|"; return "inner"; }
class SetHookPushBox {
    public array $h = [1, 2] { set { echo "set|"; $this->h = $value; } }
}
$o = new SetHookPushBox();
try { array_push($o->h, hookedPushValue()); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
echo json_encode($o->h), "\n";
class GetHookPushBox {
    public array $h { get { echo "get|"; return ["inner" => [1]]; } }
}
$get = new GetHookPushBox();
try { array_push($get->h[hookedPushKey()], hookedPushValue()); }
catch (Error $e) { echo $e->getMessage(), "\n"; }
"#);
    assert_eq!(out.stdout, concat!(
        "Indirect modification of SetHookPushBox::$h is not allowed\n",
        "[1,2]\n",
        "key|get|Indirect modification of GetHookPushBox::$h is not allowed\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reference getters lend their returned cell without invoking a setter or losing COW isolation.
#[test]
fn test_associative_push_reference_getters_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function referenceHookValue(): int { echo "value|"; return 3; }
class SelfReferencePushBox {
    public array $h = [1] { &get { echo "self-get|"; return $this->h; } }
}
$self = new SelfReferencePushBox();
$copy = $self->h;
echo array_push($self->h, referenceHookValue()), "|", json_encode($self->h), "|", json_encode($copy), "\n";
class OtherReferencePushBox {
    public array $last = ["a" => 1];
    public array $h { &get { echo "other-get|"; return $this->last; } }
}
$other = new OtherReferencePushBox();
echo array_push($other->h, referenceHookValue()), "|", json_encode($other->last), "\n";
class BothReferencePushBox {
    public array $last = ["a" => 1];
    public private(set) array $h {
        &get { echo "both-get|"; return $this->last; }
        set { echo "set|"; $this->last = $value; }
    }
}
$both = new BothReferencePushBox();
echo array_push($both->h), "|", json_encode($both->last), "\n";
echo array_push($both->h, referenceHookValue()), "|", json_encode($both->last), "\n";
"#);
    assert_eq!(out.stdout, concat!(
        "self-get|self-get|value|2|self-get|[1,3]|[1]\n",
        "other-get|value|2|{\"a\":1,\"0\":3}\n",
        "both-get|1|{\"a\":1}\n",
        "both-get|value|2|{\"a\":1,\"0\":3}\n",
    ), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
