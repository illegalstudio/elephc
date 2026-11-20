//! Purpose:
//! Regression coverage for whole-record packed buffer element assignment.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime buffer_record_copy`.
//!
//! Key details:
//! - Checks byte copies, generic growth, evaluation order, bounds and handle liveness.
//! - Runs successful fixtures with EIR optimization both enabled and disabled.

use crate::support::*;

/// Copies nested records independently, including through instance and static buffer slots.
#[test]
fn test_buffer_record_copy_preserves_complete_records() {
    let source = r#"<?php
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; public int $id; public ptr<Vec2> $link; }
class Store {
    public buffer<Body> $items;
    public static buffer<Body> $saved;
}
$links = buffer_new<Vec2>(1);
$links[0]->x = 6.5;
$src = buffer_new<Body>(2);
$dst = buffer_new<Body>(3);
$src[1]->pos->x = 2.5;
$src[1]->pos->y = -4.5;
$src[1]->id = 7;
$src[1]->link = $links[0];
$dst[1]->id = 99;
$dst[2] = $dst[0] = $src[1];
$dst[2] = $dst[2];
$src[1]->id = 42;
$dst[0]->pos->x = 8.5;
$store = new Store();
$store->items = $dst;
Store::$saved = buffer_new<Body>(1);
Store::$saved[0] = $store->items[2];
$store->items[0] = Store::$saved[0];
buffer_free($src);
echo $dst[0]->pos->x, "|", $dst[2]->pos->y, "|", $dst[2]->id, "|", $dst[1]->id, "|", $dst[2]->link->x;
$saved = Store::$saved;
buffer_free($saved);
buffer_free($dst);
buffer_free($links);
"#;
    assert_eq!(compile_and_run(source), "2.5|-4.5|7|99|6.5");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "2.5|-4.5|7|99|6.5");
}

/// Copies narrow-field tails, nested record sources, typed pointers and zero-size records.
#[test]
fn test_buffer_record_copy_handles_narrow_and_pointer_sources() {
    let source = r#"<?php
namespace CopyTest;
packed class Pixel { public u8 $r; public u8 $g; public u8 $b; }
packed class Pair { public Pixel $left; public Pixel $right; }
packed class Marker {}
$pairs = buffer_new<Pair>(1);
$pixels = buffer_new<Pixel>(3);
$pairs[0]->right->r = 201;
$pairs[0]->right->g = 102;
$pairs[0]->right->b = 53;
$pixels[1] = $pairs[0]->right;
ptr<Pixel> $p = $pixels[1];
$pixels[2] = $p;
$pixels[0] = buffer_ptr($pixels);
$pixels[0] = $pixels[2];
$pairs[0]->left = $pixels[0];
$pixels[2]->b = 1;
echo $pixels[0]->r, "|", $pixels[1]->g, "|", $pairs[0]->left->b;
$markers = buffer_new<Marker>(2);
$markers[1] = $markers[0];
echo "|", buffer_len($markers);
buffer_free($markers);
buffer_free($pixels);
buffer_free($pairs);
"#;
    assert_eq!(compile_and_run(source), "201|102|53|2");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "201|102|53|2");
}

/// Evaluates computed indices once in source order and reads a bare destination index late.
#[test]
fn test_buffer_record_copy_evaluates_indices_once() {
    let source = r#"<?php
packed class Record { public int $id; }
function destination(): int { echo "d"; return 0; }
function origin(): int { echo "s"; return 1; }
function change(int &$index): int { $index = 1; return 1; }
$src = buffer_new<Record>(2);
$dst = buffer_new<Record>(2);
$src[1]->id = 17;
$dst[destination()] = $src[origin()];
$index = 0;
$dst[$index] = $src[change($index)];
echo "|", $dst[0]->id, "|", $dst[1]->id;
"#;
    assert_eq!(compile_and_run(source), "ds|17|17");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "ds|17|17");
}

/// Grows and removes from a generic list specialized for packed records and scalar elements.
#[test]
fn test_buffer_record_copy_generic_growable_list() {
    let source = include_str!("../../examples/buffer-list/main.php").to_owned() + r#"
$self = new BufferList<Vec2>();
$single = buffer_new<Vec2>(1);
$single[0]->x = 9.5;
$self->pushFrom($single, 0);
$self->pushFrom($self->data, 0);
$self->pushFrom($self->data, 1);
echo "self: ", $self->data[2]->x, "\n";
$self->free();
buffer_free($single);
"#;
    let expected = "points: 3 capacity: 8 first: 3.5 last: 2.5\nids: 2 first: 10 last: 30\ncleared: 0\nself: 9.5\n";
    assert_eq!(compile_and_run(&source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(&source)), expected);
}

/// Preserves live integer and float accumulators across optimized record and pointer copies.
#[test]
fn test_buffer_record_copy_loop_preserves_live_values() {
    let source = r#"<?php
packed class Record { public int $id; public float $x; }
function copyRecords(buffer<Record> $dst, buffer<Record> $src, ptr<Record> $p, int $n): void {
    $a = 10.0; $b = 20.0; $c = 30.0; $d = 40.0;
    $total = 0;
    for ($i = 0; $i < $n; $i++) {
        $dst[$i] = $src[$i];
        $dst[$i] = $p;
        $a += 0.5; $b += 1.5; $c += 2.5; $d += 3.5;
        $total += $dst[$i]->id;
    }
    echo $a, "|", $b, "|", $c, "|", $d, "|", $total, "|", $dst[2]->x;
}
$src = buffer_new<Record>(3);
$src[0]->id = 7;
$src[0]->x = 1.25;
$dst = buffer_new<Record>(3);
copyRecords($dst, $src, $src[0], 3);
"#;
    let expected = "11.5|24.5|37.5|50.5|21|1.25";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(source)), expected);
}

/// Rejects invalid source and destination indices and stale aliases before copying bytes.
#[test]
fn test_buffer_record_copy_checks_bounds_and_freed_handles() {
    let prefix = "<?php packed class Record { public int $id; } $src = buffer_new<Record>(1); $dst = buffer_new<Record>(1); ";
    for (operation, expected) in [
        ("$dst[0] = $src[1];", "buffer index out of bounds"),
        ("$dst[-1] = $src[0];", "buffer index out of bounds"),
        ("$dst[1] = $src[0];", "buffer index out of bounds"),
        ("$alias = $src; buffer_free($src); $dst[0] = $alias[0];", "use of buffer after buffer_free()"),
        ("$alias = $dst; buffer_free($dst); $alias[0] = $src[0];", "use of buffer after buffer_free()"),
    ] {
        let source = format!("{prefix}{operation}");
        let error = compile_and_run_expect_failure(&source);
        assert!(error.contains(expected), "{operation}: {error}");
        let error = without_ir_opt(|| compile_and_run_expect_failure(&source));
        assert!(error.contains(expected), "unoptimized {operation}: {error}");
    }
}

/// Applies the pointer null guard even when a packed record has no payload bytes.
#[test]
fn test_buffer_record_copy_rejects_null_pointer_sources() {
    for fields in ["public int $id;", ""] {
        let source = format!("<?php packed class Record {{ {fields} }} $dst = buffer_new<Record>(1); ptr<Record> $p = ptr_null(); $dst[0] = $p;");
        let error = compile_and_run_expect_failure(&source);
        assert!(error.contains("null pointer"), "{error}");
    }
}
