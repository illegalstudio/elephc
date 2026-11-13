//! Purpose:
//! Regression tests for writing subclass and interface-implementation instances into
//! declared `array<Base>` builders (Code Journal task ce96bde7).
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` harness via `crate::support`.
//!
//! Key details:
//! - Every object element is one counted object pointer, so a nominal upcast keeps the declared
//!   object element storage. Before the fix, EIR widened the builder to boxed `mixed` slots
//!   (`prop_set assigning Array(Mixed)`), and in loops the checker's storage fixed point boxed
//!   it before the header (`expects array<Actor>, got array<mixed>`).
//! - The literal, declared-element-local and declared-parameter shapes compiled before the fix;
//!   they stay here as virtual-dispatch controls beside the repaired direct and loop shapes.
//! - Fixtures run under heap debug so the kept object storage also proves balanced ownership.

use crate::support::*;

/// Shared `Actor` hierarchy: an abstract base with two final subclasses.
const ACTORS: &str = r#"<?php
abstract class Actor { abstract public function value(): int; }
final class Player extends Actor { public function value(): int { return 42; } }
final class Enemy extends Actor { public function value(): int { return 7; } }
"#;

/// Compiles `ACTORS` followed by `body` under heap debug and asserts stdout and a clean heap.
fn assert_actor_program(body: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(&format!("{ACTORS}{body}"));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `new Player()` appended straight into a declared `array<Actor>` builder keeps object
/// storage, so storing the builder into an `array<Actor>` property compiles and dispatches.
#[test]
fn test_declared_object_array_keeps_storage_for_direct_subclass_append() {
    assert_actor_program(r#"
final class ActorList
{
    private array<Actor> $items = [];
    public function fill(): void
    {
        array<Actor> $items = [];
        $items[] = new Player();
        $this->items = $items;
    }
    public function summary(): void { echo count($this->items), ':', $this->items[0]->value(), "\n"; }
}
function run(): void { $list = new ActorList(); $list->fill(); $list->summary(); }
run();
"#, "1:42\n");
}

/// The polymorphic lifetime loop: a declared `Actor` local holding `new Player()` is appended
/// on every iteration, and the loop must not box the builder before its header.
#[test]
fn test_declared_object_array_keeps_storage_for_loop_base_local_append() {
    assert_actor_program(r#"
final class ActorList
{
    private array<Actor> $items = [];
    public function fill(): void
    {
        array<Actor> $items = [];
        for ($i = 0; $i < 2; $i++) {
            Actor $player = new Player();
            $items[] = $player;
        }
        $this->items = $items;
    }
    public function summary(): void { echo count($this->items), ':', $this->items[0]->value(), "\n"; }
}
function run(): void { $list = new ActorList(); $list->fill(); $list->summary(); }
run();
"#, "2:42\n");
}

/// Two sibling subclasses appended in one loop both keep the declared `array<Actor>` storage
/// the typed return requires, and each element still dispatches on its runtime class.
#[test]
fn test_declared_object_array_return_keeps_storage_for_sibling_subclass_appends() {
    assert_actor_program(r#"
function build(int $n): array<Actor>
{
    array<Actor> $items = [];
    for ($i = 0; $i < $n; $i++) {
        if ($i % 2 === 0) { $items[] = new Player(); } else { $items[] = new Enemy(); }
    }
    return $items;
}
function run(): void
{
    $sum = 0;
    foreach (build(5) as $actor) { $sum += $actor->value(); }
    echo $sum, "\n";
}
run();
"#, "140\n");
}

/// Offset writes and the `array_push()` builtin take the same upcast as the `[]` append.
#[test]
fn test_declared_object_array_keeps_storage_for_offset_write_and_array_push() {
    assert_actor_program(r#"
function build(): array<Actor>
{
    array<Actor> $items = [new Enemy()];
    $items[0] = new Player();
    for ($i = 0; $i < 2; $i++) { array_push($items, new Enemy()); }
    return $items;
}
function run(): void
{
    $items = build();
    echo count($items), ':', $items[0]->value(), ':', $items[2]->value(), "\n";
}
run();
"#, "3:42:7\n");
}

/// An interface element accepts a class that implements it through its parent and a class
/// that implements it directly, in straight-line code and in a loop.
#[test]
fn test_declared_interface_array_keeps_storage_for_implementations() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface Shape { public function area(): int; }
abstract class Base implements Shape {}
final class Square extends Base { public function area(): int { return 4; } }
final class Dot implements Shape { public function area(): int { return 1; } }
final class Shapes
{
    private array<Shape> $items = [];
    public function fill(): void
    {
        array<Shape> $items = [];
        $items[] = new Square();
        for ($i = 0; $i < 2; $i++) { $items[] = new Dot(); }
        $this->items = $items;
    }
    public function summary(): void { echo count($this->items), ':', $this->items[0]->area(), ':', $this->items[2]->area(), "\n"; }
}
function run(): void { $s = new Shapes(); $s->fill(); $s->summary(); }
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3:4:1\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The shapes that already compiled before the fix keep working: a typed literal, an
/// explicitly declared `Actor` local appended straight-line, and an `Actor` parameter
/// appended into the property.
#[test]
fn test_declared_object_array_controls_keep_virtual_dispatch() {
    assert_actor_program(r#"
final class ActorList
{
    private array<Actor> $items = [];
    public function literal(): void { array<Actor> $items = [new Player()]; $this->items = $items; }
    public function baseSlot(): void
    {
        Actor $player = new Player();
        array<Actor> $items = [];
        $items[] = $player;
        $this->items = $items;
    }
    public function parameter(): void { $this->items = []; $this->append(new Player()); }
    private function append(Actor $actor): void { $this->items[] = $actor; }
    public function summary(): void { echo count($this->items), ':', $this->items[0]->value(), "|"; }
}
function run(): void
{
    $list = new ActorList();
    $list->literal(); $list->summary();
    $list->baseSlot(); $list->summary();
    $list->parameter(); $list->summary();
}
run();
"#, "1:42|1:42|1:42|");
}
