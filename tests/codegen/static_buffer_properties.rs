//! Purpose:
//! Integration tests for `buffer<T>`, `ptr` and `ptr<T>` handles held in static properties:
//! stores and loads, element and packed-field access, late static binding, defaults and
//! reassignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A static slot holds the buffer handle or pointer word and owns nothing, like an instance
//!   property, so fixtures free buffers through a local copy when they check heap balance.
//! - Loops that call code able to replace or free the static buffer check that the EIR buffer
//!   resolution hoisting never reuses a handle resolved before the call.

use crate::support::*;

/// Regression test for task 231f2338: a static `buffer<float>` property stores, loads, takes
/// whole-element writes with literal and runtime indices, and answers `buffer_len()`.
#[test]
fn test_static_buffer_property_store_load_and_element_writes() {
    let out = compile_and_run(
        r#"<?php
final class Scratch {
    public static buffer<float> $shared;
}

Scratch::$shared = buffer_new<float>(3);
Scratch::$shared[0] = 1.5;
Scratch::$shared[$argc] = 2.0;
Scratch::$shared[2] = Scratch::$shared[0] + Scratch::$shared[1];
echo buffer_len(Scratch::$shared), "|", Scratch::$shared[0], "|", Scratch::$shared[1], "|", Scratch::$shared[2];
"#,
    );
    assert_eq!(out, "3|1.5|2|3.5");
}

/// Verifies compound assignments, increments and decrements write their result back into an
/// element of a static `buffer<int>` property.
#[test]
fn test_static_buffer_property_compound_and_increment_updates() {
    let out = compile_and_run(
        r#"<?php
final class Counters {
    public static buffer<int> $hits;
}

Counters::$hits = buffer_new<int>(2);
Counters::$hits[0] = 10;
Counters::$hits[0] += 5;
Counters::$hits[1]++;
Counters::$hits[1]++;
Counters::$hits[0]--;
echo Counters::$hits[0], "|", Counters::$hits[1];
"#,
    );
    assert_eq!(out, "14|2");
}

/// Verifies `self::` writes and `static::` reads of a static buffer inside methods, including a
/// method loop that fills every element.
#[test]
fn test_static_buffer_property_self_and_static_access_in_methods() {
    let out = compile_and_run(
        r#"<?php
final class Squares {
    public static buffer<int> $table;

    public static function fill(int $n): void {
        self::$table = buffer_new<int>($n);
        for ($i = 0; $i < $n; $i++) {
            self::$table[$i] = $i * $i;
        }
    }

    public static function sum(): int {
        $sum = 0;
        for ($i = 0; $i < buffer_len(static::$table); $i++) {
            $sum += static::$table[$i];
        }
        return $sum;
    }
}

Squares::fill(5);
echo Squares::sum(), "|", Squares::$table[4];
"#,
    );
    assert_eq!(out, "30|16");
}

/// Verifies packed record fields read and write through a static `buffer<Vec2>` property,
/// including a compound field update and a postfix increment from outside the class.
#[test]
fn test_static_buffer_property_packed_field_access() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }

final class World {
    public static buffer<Vec2> $points;

    public static function init(int $n): void {
        self::$points = buffer_new<Vec2>($n);
        for ($i = 0; $i < $n; $i++) {
            self::$points[$i]->x = $i * 1.5;
            self::$points[$i]->y = 2.0;
        }
    }

    public static function sumX(): float {
        $sum = 0.0;
        for ($i = 0; $i < buffer_len(static::$points); $i++) {
            $sum += static::$points[$i]->x;
        }
        return $sum;
    }
}

World::init(4);
World::$points[1]->y += 0.5;
World::$points[2]->y++;
echo World::sumX(), "|", World::$points[1]->y, "|", World::$points[2]->y, "|", World::$points[3]->x;
"#,
    );
    assert_eq!(out, "9|2.5|3|4.5");
}

/// Verifies an untyped static property refined to a buffer accepts element writes, as an
/// untyped instance property does (issue #625).
#[test]
fn test_static_buffer_property_untyped_slot_writes_elements() {
    let out = compile_and_run(
        r#"<?php
final class UntypedStore {
    public static $values;
}

UntypedStore::$values = buffer_new<int>(2);
UntypedStore::$values[0] = 42;
UntypedStore::$values[1] = 7;
echo UntypedStore::$values[0], "|", UntypedStore::$values[1];
"#,
    );
    assert_eq!(out, "42|7");
}

/// Verifies a static buffer passes to and returns from functions by handle, a local copy
/// aliases the same storage, and reassigning the slot replaces only the handle: the previous
/// buffer stays readable through its other copy.
#[test]
fn test_static_buffer_property_passing_aliasing_and_reassignment() {
    let out = compile_and_run(
        r#"<?php
function total(buffer<int> $values): int {
    $sum = 0;
    for ($i = 0; $i < buffer_len($values); $i++) {
        $sum += $values[$i];
    }
    return $sum;
}

final class Lut {
    public static buffer<int> $table;

    public static function table(): buffer<int> {
        return self::$table;
    }
}

Lut::$table = buffer_new<int>(2);
Lut::$table[0] = 5;
Lut::$table[1] = 6;
echo total(Lut::$table), "|", total(Lut::table()), "|";
$old = Lut::$table;
Lut::$table = buffer_new<int>(3);
Lut::$table[0] = 8;
echo $old[0], "|", buffer_len($old), "|", Lut::$table[0], "|", buffer_len(Lut::$table), "|";
$alias = Lut::$table;
$alias[1] = 4;
echo Lut::$table[1];
buffer_free($old);
"#,
    );
    assert_eq!(out, "11|11|5|2|8|3|4");
}

/// Verifies a typed static buffer property without a default is uninitialized: `isset()`
/// answers false and a read throws the catchable typed-static-property `Error` until the first
/// assignment.
#[test]
fn test_static_buffer_property_starts_uninitialized() {
    let out = compile_and_run(
        r#"<?php
final class Lazy {
    public static buffer<float> $data;
}

var_dump(isset(Lazy::$data));
try {
    echo buffer_len(Lazy::$data);
} catch (\Error $e) {
    echo $e->getMessage(), "\n";
}
Lazy::$data = buffer_new<float>(2);
var_dump(isset(Lazy::$data));
echo buffer_len(Lazy::$data);
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nTyped static property Lazy::$data must not be accessed before initialization\nbool(true)\n2"
    );
}

/// Verifies `static::` element writes and reads of a static buffer select the slot of the
/// called class when a subclass redeclares the property.
#[test]
fn test_static_buffer_property_late_static_binding_selects_redeclared_slot() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static buffer<int> $slots;

    public static function put(int $i, int $v): void {
        static::$slots[$i] = $v;
    }

    public static function get(int $i): int {
        return static::$slots[$i];
    }
}

final class ChildRegistry extends Registry {
    public static buffer<int> $slots;
}

Registry::$slots = buffer_new<int>(2);
ChildRegistry::$slots = buffer_new<int>(2);
Registry::put(0, 11);
ChildRegistry::put(0, 22);
echo Registry::get(0), "|", ChildRegistry::get(0), "|", Registry::$slots[0], "|", ChildRegistry::$slots[0];
"#,
    );
    assert_eq!(out, "11|22|11|22");
}

/// Verifies a loop that reads a static buffer sees the buffer a call inside the loop stored in
/// the slot: the handle is read again after the call instead of a resolution hoisted before it.
#[test]
fn test_static_buffer_property_loop_sees_buffer_replaced_by_call() {
    let out = compile_and_run(
        r#"<?php
final class Pool {
    public static buffer<int> $live;

    public static function swap(): void {
        $next = buffer_new<int>(4);
        for ($i = 0; $i < 4; $i++) {
            $next[$i] = ($i + 1) * 10;
        }
        self::$live = $next;
    }
}

Pool::$live = buffer_new<int>(4);
for ($i = 0; $i < 4; $i++) {
    Pool::$live[$i] = $i + 1;
}
$sum = 0;
for ($i = 0; $i < 4; $i++) {
    if ($i === 2) {
        Pool::swap();
    }
    $sum += Pool::$live[$i];
}
echo $sum;
"#,
    );
    assert_eq!(out, "73");
}

/// Verifies freeing a static buffer through a copy inside a loop body makes the next access
/// through the static slot abort with the use-after-free fatal, and bounds stay checked.
#[test]
fn test_static_buffer_property_use_after_free_and_bounds_are_fatal() {
    let freed = compile_and_run_expect_failure(
        r#"<?php
final class Pool {
    public static buffer<int> $live;
}

function drop(): void {
    $alias = Pool::$live;
    buffer_free($alias);
}

Pool::$live = buffer_new<int>(2);
for ($i = 0; $i < 2; $i++) {
    if ($i === 1) {
        drop();
    }
    echo Pool::$live[$i], ",";
}
"#,
    );
    assert!(
        freed.contains("use of buffer after buffer_free()"),
        "{}",
        freed
    );

    let bounds = compile_and_run_expect_failure(
        r#"<?php
final class Small {
    public static buffer<int> $values;
}

Small::$values = buffer_new<int>(1);
Small::$values[1] = 9;
"#,
    );
    assert!(bounds.contains("buffer index out of bounds"), "{}", bounds);
}

/// Verifies repeated element writes and reads through a static buffer leave a clean heap once
/// the buffer is freed through a local copy.
#[test]
fn test_static_buffer_property_write_loop_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class Grid {
    public static buffer<int> $cells;
}

Grid::$cells = buffer_new<int>(500);
for ($i = 0; $i < 500; $i++) {
    Grid::$cells[$i] = $i;
}
$sum = 0;
for ($i = 0; $i < 500; $i++) {
    $sum += Grid::$cells[$i];
}
echo $sum;
$cells = Grid::$cells;
buffer_free($cells);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "124750");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected static buffer writes to leave a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies `ptr`, `ptr<T>` and `?ptr<T>` static properties store and load pointer words, a
/// `= null` default on a plain `ptr` starts as the null pointer instead of uninitialized, and a
/// field write through a `ptr<T>` static reaches the record it points at.
#[test]
fn test_static_pointer_properties_store_load_and_null_default() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }

final class Cursor {
    public static ptr $raw = null;
    public static ptr<Vec2> $at;
    public static ?ptr<Vec2> $maybe = null;
}

echo ptr_is_null(Cursor::$raw) ? "null" : "set", "|";
$items = buffer_new<Vec2>(2);
$items[1]->x = 7;
Cursor::$at = $items[1];
Cursor::$raw = buffer_ptr($items);
Cursor::$maybe = $items[0];
echo Cursor::$at->x, "|", ptr_is_null(Cursor::$raw) ? "null" : "set", "|", Cursor::$maybe === null ? "none" : "some";
Cursor::$at->y = 9;
echo "|", $items[1]->y;
"#,
    );
    assert_eq!(out, "null|7|set|some|9");
}

/// Verifies a typed `ptr<T>` static property without a default is uninitialized until its first
/// assignment, like every typed static property.
#[test]
fn test_static_pointer_property_without_default_starts_uninitialized() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }

final class Anchor {
    public static ptr<Vec2> $at;
}

try {
    echo Anchor::$at->x;
} catch (\Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Typed static property Anchor::$at must not be accessed before initialization"
    );
}

/// Verifies ternary, `match` and `?:` merges whose arms read static buffer properties keep the
/// chosen buffer's handle (the merge rule of PR #2042), so writes through the result reach that
/// buffer, a static buffer read in a condition tests its handle, and indexed and keyed literals
/// of static buffers hold the handles.
#[test]
fn test_static_buffer_property_merges_choose_the_buffer() {
    let out = compile_and_run(
        r#"<?php
final class Frames {
    public static buffer<float> $front;
    public static buffer<float> $back;
    public static int $frame = 0;

    public static function current(): buffer<float> {
        return self::$frame % 2 === 0 ? self::$front : self::$back;
    }

    public static function next(): buffer<float> {
        return match (self::$frame % 2) {
            0 => self::$back,
            default => self::$front,
        };
    }
}

Frames::$front = buffer_new<float>(2);
Frames::$back = buffer_new<float>(2);
Frames::$front[0] = 1.5;
$next = Frames::next();
$next[0] = Frames::current()[0] + 1.0;
$pick = $argc > 1 ? Frames::$front : Frames::$back;
$pick[1] = 7.5;
$either = Frames::$front ?: Frames::$back;
echo Frames::$back[0], "|", Frames::$back[1], "|", $either[0], "|", Frames::$front ? "set" : "unset";
$list = [Frames::$front, Frames::$back];
$named = ["front" => Frames::$front];
echo "|", $list[1][0], "|", $named["front"][0];
"#,
    );
    assert_eq!(out, "2.5|7.5|1.5|set|2.5|1.5");
}

/// Verifies static `ptr<T>` and `?ptr<T>` properties exchange values with locals that hold
/// `null` and record addresses as one pointer word (PR #2046): a loop result stored into a
/// `?ptr<T>` static, a static read into a local that starts as `null` and is stored back into a
/// static, and a ternary of a record address and a `ptr<T>` static that stays a pointer. The
/// branch-assigned local is not dereferenced directly: DCE sinks the statements after its `if`
/// into the arm that keeps `null`, where a field read fails in codegen (task f1688e8f).
#[test]
fn test_static_pointer_properties_with_null_or_pointer_locals() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }

final class Cursor {
    public static ptr<Vec2> $at;
    public static ?ptr<Vec2> $best = null;
}

$items = buffer_new<Vec2>(3);
$items[0]->x = 4;
$items[1]->x = 9;
$items[2]->x = 6;
$best = null;
for ($i = 0; $i < buffer_len($items); $i++) {
    if ($best === null || $items[$i]->x > $best->x) {
        $best = $items[$i];
    }
}
Cursor::$best = $best;
echo Cursor::$best === null ? "none" : Cursor::$best->x, "|";
Cursor::$at = $items[2];
$cur = null;
if ($argc > 0) {
    $cur = Cursor::$at;
}
echo $cur === null ? "none" : "set", "|";
Cursor::$best = $cur;
echo Cursor::$best === null ? "none" : Cursor::$best->x, "|";
$pick = $argc > 1 ? $items[0] : Cursor::$at;
echo $pick->x;
"#,
    );
    assert_eq!(out, "9|set|6|6");
}

/// Verifies the static-buffer-tables example compiles and prints its documented output.
#[test]
fn test_static_buffer_tables_example() {
    let out = compile_and_run(include_str!("../../examples/static-buffer-tables/main.php"));
    assert_eq!(
        out,
        "square(5)=25\nsum of squares=140\nring[3]=1.5,6.5\nring[0].x=10\nscratch sum=15\ncursor y=1\n"
    );
}
