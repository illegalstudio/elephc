//! Purpose:
//! Integration tests for nested packed class fields: a packed field whose type is another packed
//! class is an embedded record reached through its compile-time offset.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers reads, writes and compound assignments through buffer elements and `ptr<T>` typed
//!   pointers, three levels of nesting, layout offsets and stride (including offsets beyond the
//!   AArch64 `add` immediate range), whole-record copies, inner classes declared after the outer
//!   one, pruning of classes named only by fields, and the null-pointer guard on typed pointer
//!   receivers, including literal dynamic names.

use crate::support::*;

/// Verifies nested fields are written and read through buffer elements without touching the
/// sibling record or the other element.
#[test]
fn test_nested_packed_fields_read_and_write_through_buffer_elements() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; public Vec2 $vel; }
$b = buffer_new<Body>(2);
$b[0]->pos->x = 1.25;
$b[0]->pos->y = 2.5;
$b[1]->vel->y = 7.5;
echo $b[0]->pos->x, " ", $b[0]->pos->y, " ", $b[1]->vel->y, "\n";
echo $b[0]->vel->x, " ", $b[0]->vel->y, " ", $b[1]->pos->x, " ", $b[1]->pos->y, "\n";
"#,
    );
    assert_eq!(out, "1.25 2.5 7.5\n0 0 0 0\n");
}

/// Verifies compound assignments, increments and decrements on nested fields, including an int
/// field fed by a counter the checker types as `int|float`.
#[test]
fn test_nested_packed_fields_support_compound_assignment() {
    let out = compile_and_run(
        r#"<?php
packed class Counter { public int $hits; public float $total; }
packed class Stats { public Counter $a; public Counter $b; }
$s = buffer_new<Stats>(1);
$s[0]->a->hits = 4;
$s[0]->a->hits += 3;
$s[0]->a->hits++;
$s[0]->b->hits--;
$s[0]->a->total = 1.5;
$s[0]->a->total *= 4.0;
$s[0]->b->total -= 0.25;
$n = 0;
$n++;
$s[0]->b->hits = $n;
echo $s[0]->a->hits, " ", $s[0]->b->hits, " ", $s[0]->a->total, " ", $s[0]->b->total, "\n";
"#,
    );
    assert_eq!(out, "8 1 6 -0.25\n");
}

/// Verifies three levels of nesting resolve the summed offsets for reads and writes, and that a
/// local holding a nested record aliases the buffer storage like a local holding an element.
#[test]
fn test_nested_packed_fields_resolve_three_levels() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; public Vec2 $vel; public int $id; }
packed class World { public int $tick; public Body $player; }
$w = buffer_new<World>(2);
$w[1]->tick = 11;
$w[1]->player->id = 4;
$w[1]->player->vel->x = 3.5;
$w[1]->player->pos->y = -2.0;
$player = $w[1]->player;
$player->vel->y = 9.0;
echo $w[1]->tick, " ", $w[1]->player->id, " ", $w[1]->player->vel->x, " ", $w[1]->player->pos->y, " ", $w[1]->player->vel->y, "\n";
echo $w[0]->tick, " ", $w[0]->player->id, " ", $w[0]->player->vel->x, "\n";
"#,
    );
    assert_eq!(out, "11 4 3.5 -2 9\n0 0 0\n");
}

/// Verifies the embedded layout: an inner record occupies its own size inside the outer one, the
/// outer size is the sum of its fields, and a buffer of outer records strides by that size.
#[test]
fn test_nested_packed_layout_offsets_and_stride() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern function free(ptr $p): void;
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; public Vec2 $vel; }
echo ptr_sizeof("Vec2"), " ", ptr_sizeof("Body"), "\n";
$raw = calloc(2, ptr_sizeof("Body"));
$second = ptr_cast<Body>(ptr_offset($raw, ptr_sizeof("Body")));
$second->pos->x = 1.0;
$second->pos->y = 2.0;
$second->vel->x = 3.0;
$second->vel->y = 4.0;
echo ptr_read_f64(ptr_offset($raw, 32)), " ", ptr_read_f64(ptr_offset($raw, 40)), " ";
echo ptr_read_f64(ptr_offset($raw, 48)), " ", ptr_read_f64(ptr_offset($raw, 56)), " ";
echo ptr_read_f64($raw), "\n";
free($raw);
$b = buffer_new<Body>(3);
$b[1]->vel->y = 5.0;
echo $b[0]->vel->y, " ", $b[1]->vel->y, " ", $b[2]->pos->x, "\n";
"#,
    );
    assert_eq!(out, "16 32\n1 2 3 4 0\n0 5 0\n");
}

/// Verifies nested and scalar fields through `ptr<T>` typed pointers, including function
/// parameters typed as `ptr<T>` and `buffer<T>`, and an int field fed by an `int|float` counter.
#[test]
fn test_nested_packed_fields_through_typed_pointers() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern function free(ptr $p): void;
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; public Vec2 $vel; public int $id; }
function tag(ptr<Body> $p, int $id): void {
    $p->id = $id;
    $p->vel->x = 0.5;
}
function step(buffer<Body> $b, int $n, float $dt): void {
    for ($i = 0; $i < $n; $i++) {
        $b[$i]->pos->x += $b[$i]->vel->x * $dt;
    }
}
$raw = calloc(1, ptr_sizeof("Body"));
$p = ptr_cast<Body>($raw);
tag($p, 12);
$p->pos->y = 1.5;
$p->pos->y += 1.0;
echo $p->id, " ", $p->vel->x, " ", $p->pos->y, "\n";
$n = 0;
$n++;
$p->id = $n;
echo $p->id, "\n";
free($raw);
$b = buffer_new<Body>(2);
$b[1]->vel->x = 4.0;
step($b, 2, 0.5);
echo $b[0]->pos->x, " ", $b[1]->pos->x, "\n";
"#,
    );
    assert_eq!(out, "12 0.5 2.5\n1\n0 2\n");
}

/// Verifies a packed class reached only through `ptr_cast<T>` keeps its layout after declaration
/// pruning, so field access through the typed pointer compiles and runs.
#[test]
fn test_packed_class_named_only_by_ptr_cast_is_kept() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern function free(ptr $p): void;
packed class Pair { public int $left; public int $right; }
$raw = calloc(1, 16);
$pair = ptr_cast<Pair>($raw);
$pair->right = 42;
$pair->left = $pair->right - 1;
echo $pair->left, ":", $pair->right, ":", ptr_get(ptr_offset($raw, 8)), "\n";
free($raw);
"#,
    );
    assert_eq!(out, "41:42:42\n");
}

/// Verifies whole-record assignment copies the inner record's bytes from a buffer element, from
/// another nested field and from a `ptr<T>`, and that the copy does not alias its source.
#[test]
fn test_nested_packed_field_whole_record_assignment_copies_bytes() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern function free(ptr $p): void;
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; public Vec2 $vel; public int $id; }
packed class World { public int $tick; public Body $player; }
$points = buffer_new<Vec2>(1);
$points[0]->x = 1.5;
$points[0]->y = 2.5;
$b = buffer_new<Body>(2);
$b[0]->pos = $points[0];
$points[0]->x = 99.0;
echo $b[0]->pos->x, " ", $b[0]->pos->y, "\n";
$b[0]->vel = $b[0]->pos;
$b[0]->vel = $b[0]->vel;
echo $b[0]->vel->x, " ", $b[0]->vel->y, "\n";
$raw = calloc(1, ptr_sizeof("Vec2"));
$v = ptr_cast<Vec2>($raw);
$v->y = -4.0;
$b[1]->pos = $v;
echo $b[1]->pos->x, " ", $b[1]->pos->y, "\n";
free($raw);
$b[1]->id = 7;
$w = buffer_new<World>(1);
$w[0]->tick = 3;
$w[0]->player = $b[1];
$w[0]->player->pos = $b[0]->vel;
echo $w[0]->tick, " ", $w[0]->player->id, " ", $w[0]->player->pos->x, " ", $w[0]->player->pos->y, " ", $b[1]->pos->y, "\n";
"#,
    );
    assert_eq!(out, "1.5 2.5\n1.5 2.5\n0 -4\n3 7 1.5 2.5 -4\n");
}

/// Verifies a field read through a null `ptr<T>` aborts with the pointer extension's fatal
/// instead of faulting.
#[test]
fn test_typed_pointer_field_read_on_null_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
$p = ptr_cast<Vec2>(ptr_null());
echo $p->x;
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}

/// Verifies a field write through a null `ptr<T>` aborts with the pointer extension's fatal.
#[test]
fn test_typed_pointer_field_write_on_null_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
$p = ptr_cast<Vec2>(ptr_null());
$p->y = 1.0;
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}

/// Verifies copying a whole record from a null `ptr<T>` aborts instead of reading address zero.
#[test]
fn test_nested_packed_record_copy_from_null_pointer_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; }
$b = buffer_new<Body>(1);
$b[0]->pos = ptr_cast<Vec2>(ptr_null());
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}

/// Verifies a packed class may embed packed classes declared after it, at any depth: each outer
/// layout is sized once the record it embeds exists.
#[test]
fn test_nested_packed_class_may_be_declared_after_its_outer_class() {
    let out = compile_and_run(
        r#"<?php
packed class World { public Body $player; public int $tick; }
packed class Body { public Vec2 $pos; public int $id; }
packed class Vec2 { public float $x; public float $y; }
$w = buffer_new<World>(2);
$w[1]->player->pos->y = 2.5;
$w[1]->player->id = 7;
$w[1]->tick = 3;
echo ptr_sizeof("World"), " ", ptr_sizeof("Body"), " ", $w[1]->player->pos->y, " ";
echo $w[1]->player->id, " ", $w[1]->tick, " ", $w[0]->player->pos->y, "\n";
"#,
    );
    assert_eq!(out, "32 24 2.5 7 3 0\n");
}

/// Verifies packed pointer fields may name their own class or a later packed declaration without
/// introducing a by-value layout dependency.
#[test]
fn test_packed_pointer_fields_accept_self_and_forward_targets() {
    let out = compile_and_run(
        r#"<?php
packed class Node { public int $value; public ptr<Node> $next; }
packed class Holder { public ptr<Target> $target; }
packed class Target { public int $value; }
$nodes = buffer_new<Node>(2);
$nodes[1]->value = 9;
$nodes[0]->next = $nodes[1];
$holders = buffer_new<Holder>(1);
$targets = buffer_new<Target>(1);
$targets[0]->value = 7;
$holders[0]->target = $targets[0];
echo ptr_sizeof("Node"), " ", ptr_sizeof("Holder"), " ";
echo $nodes[0]->next->value, " ", $holders[0]->target->value, "\n";
"#,
    );
    assert_eq!(out, "16 8 9 7\n");
}

/// Verifies packed record addresses flow into returns, pointer fields, extern calls, and pointer
/// builtins for both buffer elements and nested records.
#[test]
fn test_packed_record_addresses_work_at_pointer_sinks() {
    let out = compile_and_run(
        r#"<?php
extern function memcmp(ptr<Vec2> $left, ptr<Vec2> $right, u64 $size): i32;
packed class Vec2 { public int $x; public int $y; }
packed class Body { public Vec2 $pos; }
packed class Holder { public ptr<Vec2> $value; }
function first(buffer<Vec2> $values): ptr<Vec2> { return $values[0]; }
$values = buffer_new<Vec2>(1);
$values[0]->x = 11;
$values[0]->y = 22;
$holders = buffer_new<Holder>(1);
$holders[0]->value = $values[0];
$bodies = buffer_new<Body>(1);
$bodies[0]->pos->x = 33;
$bodies[0]->pos->y = 44;
echo ptr_is_null(first($values)) ? "null " : "live ";
echo $holders[0]->value->y, " ", memcmp($values[0], first($values), ptr_sizeof("Vec2")), "\n";
echo ptr_get($values[0]), " ", ptr_get(ptr_offset($values[0], 8)), " ";
echo ptr_get($bodies[0]->pos), " ", ptr_get(ptr_offset($bodies[0]->pos, 8)), "\n";
"#,
    );
    assert_eq!(out, "live 22 0\n11 22 33 44\n");
}

/// Verifies a packed record address compares by address identity with `===` and `!==`, both
/// against another packed address and against a `ptr<T>` that stored it.
#[test]
fn test_packed_record_addresses_compare_by_identity() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
packed class Holder { public ptr<Vec2> $p; }
$values = buffer_new<Vec2>(2);
$holder = buffer_new<Holder>(1);
$holder[0]->p = $values[0];
echo $values[0] === $holder[0]->p ? "same " : "different ";
echo $values[1] === $holder[0]->p ? "same " : "different ";
echo $values[0] !== $values[1] ? "distinct " : "equal ";
echo $values[1] === $values[1] ? "same" : "different", "\n";
"#,
    );
    assert_eq!(out, "same different distinct same\n");
}

/// Verifies a packed record address reaches an extern parameter and a return that name another
/// pointer class (both accept any pointer), `ptr_cast<T>` of a buffer element, and the stored
/// word of `ptr_set`. A packed `ptr<T>` field keeps rejecting another class at type check.
#[test]
fn test_packed_record_addresses_reach_cross_target_sinks_casts_and_ptr_set() {
    let out = compile_and_run(
        r#"<?php
extern function memset(ptr<Other> $target, int $byte, u64 $size): ptr;
packed class Vec2 { public int $x; public int $y; }
packed class Other { public int $z; }
packed class Holder { public ptr<Vec2> $p; }
function as_other(buffer<Vec2> $values): ptr<Other> { return $values[1]; }
$values = buffer_new<Vec2>(2);
$values[0]->x = 5;
$values[1]->x = 6;
memset($values[0], 0, 8);
echo $values[0]->x, " ", as_other($values)->z, " ";
$cast = ptr_cast<Vec2>($values[1]);
$cast->y = 9;
echo $values[1]->y, " ", ptr_get(ptr_cast<int>($values[1])), " ";
$holder = buffer_new<Holder>(1);
ptr_set($holder[0], $values[1]);
echo $holder[0]->p->x, "\n";
"#,
    );
    assert_eq!(out, "0 6 9 6 6\n");
}

/// Verifies a packed record address keeps its pointer word through ternary, `??` and `match`
/// merges with `ptr_null()`, and works as the operand of `if`, `empty`, `switch`, `echo` and
/// every string conversion (`(string)`, `strval`, `.`, `sprintf("%s")`), like the `ptr<T>` the
/// checker types it as.
#[test]
fn test_packed_record_addresses_survive_merges_and_scalar_contexts() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
$values = buffer_new<Vec2>(2);
$values[0]->x = 7;
$values[1]->x = 8;
$p = ($argc > 0) ? $values[0] : ptr_null();
echo $p->x, " ";
$q = $values[1] ?? ptr_null();
echo $q->x, " ";
$r = match ($argc) { 1 => $values[1], default => ptr_null() };
echo $r->x, " ";
$s = match ($argc) { 1 => $values[1], default => $values[0] };
echo $s->x, " ";
$t = match ($argc) { 2 => ptr_null(), default => $values[0] };
echo $t->x, " ";
if ($values[0]) { echo "live "; }
echo empty($values[0]) ? "empty " : "set ";
switch ($values[0]) {
    case $values[1]:
        echo "second";
        break;
    case $values[0]:
        echo "first";
        break;
    default:
        echo "none";
}
echo "\n", $values[0], "\n";
echo (string) $values[0], "\n", strval($values[0]), "\n";
echo "x" . $values[0], "\n", sprintf("%s", $values[0]), "\n";
"#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.first(), Some(&"7 8 8 8 7 live set first"), "{out}");
    let address = lines[1];
    assert!(address.starts_with("0x"), "{out}");
    assert_eq!(&lines[2..], &[address, address, &format!("x{address}"), address], "{out}");
}

/// Verifies a nested packed field read keeps its record address through ternary, `?:`, `??`
/// and `match` merges, alone and against another nested field or `ptr_null()`.
#[test]
fn test_nested_packed_field_addresses_survive_merges() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
packed class Body { public Vec2 $pos; public Vec2 $vel; }
$bodies = buffer_new<Body>(2);
$bodies[0]->pos->x = 4;
$bodies[1]->pos->x = 5;
$bodies[0]->vel->x = 6;
$a = ($argc > 0) ? $bodies[0]->pos : ptr_null();
$b = $bodies[1]->pos ?: ptr_null();
$c = $bodies[0]->vel ?? $bodies[1]->pos;
$d = match ($argc) { 1 => $bodies[1]->pos, default => $bodies[0]->pos };
$e = ($argc > 0) ? $bodies[0]->vel : $bodies[1]->pos;
echo $a->x, " ", $b->x, " ", $c->x, " ", $d->x, " ", $e->x, "\n";
$first = $bodies[0]->pos;
$second = $bodies[1]->pos;
$f = ($argc > 0) ? $first : $second;
$g = match ($argc) { 1 => $second, default => $first };
echo $f->x, " ", $g->x, "\n";
"#,
    );
    assert_eq!(out, "4 5 6 5 6\n4 5\n");
}

/// Verifies two locals holding packed buffer element addresses merge through a ternary and a
/// `match` without boxing, and that `gettype` names a packed address `pointer` like the
/// `ptr<T>` it becomes at a pointer return.
#[test]
fn test_packed_record_address_locals_merge_and_name_as_pointers() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function id(ptr<Vec2> $p): ptr<Vec2> { return $p; }
$values = buffer_new<Vec2>(2);
$values[0]->x = 7;
$values[1]->x = 8;
$a = $values[0];
$b = $values[1];
$s = ($argc > 0) ? $a : $b;
$m = match ($argc) { 1 => $b, default => $a };
echo $s->x, " ", $m->x, " ", gettype($values[0]), " ", gettype(id($values[0])), "\n";
"#,
    );
    assert_eq!(out, "7 8 pointer pointer\n");
}

/// Verifies a packed `ptr<T>` field and an element of a buffer returned by a function keep their
/// pointer word through ternary, `match` and `?:` merges with `ptr_null()`.
#[test]
fn test_pointer_fields_and_returned_buffer_elements_survive_merges() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
packed class Holder { public ptr<Vec2> $value; }
function make(buffer<Vec2> $values): buffer<Vec2> { return $values; }
$values = buffer_new<Vec2>(1);
$values[0]->x = 7;
$holders = buffer_new<Holder>(1);
$holders[0]->value = $values[0];
$a = ($argc > 0) ? $holders[0]->value : ptr_null();
$b = match ($argc) { 1 => $holders[0]->value, default => ptr_null() };
$c = $holders[0]->value ?: ptr_null();
$d = ($argc > 0) ? make($values)[0] : ptr_null();
$e = match ($argc) { 1 => make($values)[0], default => ptr_null() };
echo $a->x, " ", $b->x, " ", $c->x, " ", $d->x, " ", $e->x, "\n";
"#,
    );
    assert_eq!(out, "7 7 7 7 7\n");
}

/// Verifies a nested record read through a call returning `ptr<T>` keeps its pointer word
/// through ternary, `match` and `?:` merges.
#[test]
fn test_nested_fields_through_pointer_returning_calls_survive_merges() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
packed class Body { public Vec2 $pos; }
function first(buffer<Body> $bodies): ptr<Body> { return $bodies[0]; }
$bodies = buffer_new<Body>(1);
$bodies[0]->pos->x = 9;
$a = ($argc > 0) ? first($bodies)->pos : ptr_null();
$b = match ($argc) { 1 => first($bodies)->pos, default => ptr_null() };
$c = first($bodies)->pos ?: ptr_null();
echo $a->x, " ", $b->x, " ", $c->x, "\n";
"#,
    );
    assert_eq!(out, "9 9 9\n");
}

/// Verifies a nested record placed beyond the AArch64 `add` immediate range (byte offset 4104)
/// is addressed correctly for reads, writes and compound assignments.
#[test]
fn test_nested_packed_field_beyond_the_add_immediate_range() {
    let fields: String = (0..513).map(|i| format!("public int $f{}; ", i)).collect();
    let source = format!(
        r#"<?php
packed class Vec2 {{ public float $x; public float $y; }}
packed class Big {{ {fields}public Vec2 $tail; }}
$b = buffer_new<Big>(1);
$b[0]->tail->x = 1.5;
$b[0]->tail->y += 2.0;
$b[0]->f512 = 7;
echo $b[0]->tail->x, " ", $b[0]->tail->y, " ", $b[0]->f512, " ", ptr_sizeof("Big"), "\n";
"#
    );
    let out = compile_and_run(&source);
    assert_eq!(out, "1.5 2 7 4120\n");
}

/// Verifies a packed class named only as the target of a `ptr<T>` field of a reachable packed
/// class keeps its layout after declaration pruning, so the field access through it compiles
/// and reaches the null-pointer guard at run time.
#[test]
fn test_packed_class_named_only_by_a_pointer_field_is_kept() {
    let err = compile_and_run_expect_failure(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
packed class Holder { public ptr<Vec2> $p; }
function first_x(buffer<Holder> $h): float { return $h[0]->p->x; }
$h = buffer_new<Holder>(1);
echo first_x($h);
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}

/// Verifies a packed class named only as the `ptr<T>` return type of a reachable extern function
/// keeps its layout after declaration pruning, so field access through the returned pointer
/// compiles and runs.
#[test]
fn test_packed_class_named_only_by_an_extern_signature_is_kept() {
    let out = compile_and_run(
        r#"<?php
extern function calloc(u64 $count, u64 $size): ptr<Pair>;
extern function free(ptr $p): void;
packed class Pair { public int $left; public int $right; }
$pair = calloc(1, 16);
$pair->right = 42;
$pair->left = $pair->right - 1;
echo $pair->left, ":", $pair->right, "\n";
free($pair);
"#,
    );
    assert_eq!(out, "41:42\n");
}

/// Verifies a packed class named only as the `ptr<T>` type of an extern global keeps its layout
/// after declaration pruning: the field read through the global sees the same first word as
/// `ptr_get` on it.
#[test]
fn test_packed_class_named_only_by_an_extern_global_is_kept() {
    let out = compile_and_run(
        r#"<?php
packed class Slot { public int $first; }
extern global ptr<Slot> $environ;
echo $environ->first == ptr_get($environ) ? "same" : "diff", "\n";
"#,
    );
    assert_eq!(out, "same\n");
}

/// Verifies `ptr_cast<T>` and an extern global's `ptr<T>` inside a namespace name that
/// namespace's packed class, not a same-named global one with other fields: `App\Pair` and
/// `App\Slot` fields resolve, and the global classes keep their own sizes.
#[test]
fn test_namespaced_pointer_targets_do_not_bind_a_same_named_global_packed_class() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
packed class Pair { public int $left; public int $right; public int $extra; }
packed class Slot { public int $wrong; }
require "pair.php";
echo App\demo(calloc(1, 16)), " ", App\slot_matches(), " ", ptr_sizeof("Pair"), "\n";
"#,
            ),
            (
                "pair.php",
                r#"<?php
namespace App;
packed class Pair { public int $only; public int $other; }
packed class Slot { public int $first; }
extern global ptr<Slot> $environ;
function demo(ptr $raw): string {
    $p = ptr_cast<Pair>($raw);
    $p->other = 5;
    return $p->only . ":" . $p->other . ":" . ptr_get(ptr_offset($raw, 8)) . ":"
        . ptr_sizeof("App\\Pair");
}
function slot_matches(): string {
    global $environ;
    return $environ->first == ptr_get($environ) ? "same" : "diff";
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "0:5:5:16 same 24\n");
}

/// Verifies a fully qualified `ptr_cast<\T>` inside a namespace names the rooted class, while a
/// bare `ptr_cast<T>` names the current namespace's one: three same-named packed classes with
/// different fields each resolve through their own spelling.
#[test]
fn test_rooted_ptr_cast_targets_skip_the_current_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
packed class Pair { public int $left; public int $right; public int $extra; }
require "other.php";
require "app.php";
echo App\demo(calloc(1, 24)), "\n";
"#,
            ),
            (
                "other.php",
                r#"<?php
namespace Other;
packed class Pair { public int $a; public int $b; }
"#,
            ),
            (
                "app.php",
                r#"<?php
namespace App;
packed class Pair { public int $only; public int $other; }
function demo(ptr $raw): string {
    $global = ptr_cast<\Pair>($raw);
    $global->extra = 3;
    $other = ptr_cast<\Other\Pair>($raw);
    $other->b = 4;
    $local = ptr_cast<Pair>($raw);
    return $global->extra . ":" . $other->b . ":" . $local->other . ":"
        . ptr_get(ptr_offset($raw, 16));
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "3:4:4:3\n");
}

/// Verifies a `ptr<T>` return type of a global extern function resolves a `use` alias to the
/// namespaced packed class it imports, so field access through the returned pointer compiles.
#[test]
fn test_extern_pointer_target_resolves_a_use_alias() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
use App\Pair as NsPair;
require "pair.php";
extern function calloc(u64 $count, u64 $size): ptr<NsPair>;
$pair = calloc(1, 16);
$pair->other = 9;
echo $pair->only, ":", $pair->other, "\n";
"#,
            ),
            (
                "pair.php",
                r#"<?php
namespace App;
packed class Pair { public int $only; public int $other; }
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "0:9\n");
}

/// Verifies a field read through a null `ptr<T>` spelled with a literal dynamic name
/// (`$p->{'x'}`) aborts with the same fatal as `$p->x` instead of faulting.
#[test]
fn test_typed_pointer_dynamic_name_read_on_null_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
function read_x(ptr<Vec2> $p): float { return $p->{'x'}; }
echo read_x(ptr_cast<Vec2>(ptr_null()));
"#,
    );
    assert!(err.contains("null pointer dereference"), "{}", err);
}
