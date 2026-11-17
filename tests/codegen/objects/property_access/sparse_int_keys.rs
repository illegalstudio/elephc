//! Purpose:
//! Integration tests for integer keys that skip slots of an untyped array property.
//!
//! Called from:
//! - `cargo test` through Rust's test harness, via `objects/property_access/mod.rs`.
//!
//! Key details:
//! - PHP promotes a packed array to a hash the moment an integer key skips a slot. An untyped
//!   property slot is typed from every write in the class, so only a key provably `0` may stay
//!   on packed storage; any other integer key must not zero-fill the gap.

use super::*;

/// A literal key written on a fresh object stays sparse although another function writes slot 0.
#[test]
fn test_untyped_property_literal_key_skips_slot_on_fresh_object() {
    let out = compile_and_run(r#"<?php
class H { public $items = []; }
function w0(H $h, int $n): void { $h->items[0] = $n; }
function w1(H $h, int $n): void { $h->items[1] = $n; }
$h = new H();
w1($h, 6);
echo json_encode($h->items), "\n";
$g = new H();
w0($g, 5);
w1($g, 6);
echo json_encode($g->items), "\n";
class M { public $items = []; }
function m0(M $m): void { $m->items[0] = "a"; }
function m1(M $m, int $n): void { $m->items[3] = $n; }
$m = new M();
m1($m, 6);
m0($m);
echo json_encode($m->items), "\n";
"#);
    assert_eq!(out, "{\"1\":6}\n[5,6]\n{\"3\":6,\"0\":\"a\"}\n");
}

/// A variable key, a foreach value key and a negative key keep PHP's sparse hash shape.
#[test]
fn test_untyped_property_variable_key_skips_slots() {
    let out = compile_and_run(r#"<?php
class K { public $items = []; }
function put(K $h, int $k, int $n): void { $h->items[$k] = $n; }
$q = new K();
put($q, 3, 6);
put($q, -2, 7);
echo json_encode($q->items), " ", count($q->items), "\n";
class Rows {
    public $rows = [];
    public function load(array $ids): void {
        foreach ($ids as $id) { $this->rows[$id] = "row" . $id; }
    }
}
$r = new Rows();
$r->load([101, 102, 205]);
echo count($r->rows), " ", json_encode($r->rows), "\n";
foreach ($r->rows as $k => $v) { echo $k, "=", $v, " "; }
echo "\n";
"#);
    assert_eq!(
        out,
        "{\"3\":6,\"-2\":7} 2\n3 {\"101\":\"row101\",\"102\":\"row102\",\"205\":\"row205\"}\n101=row101 102=row102 205=row205 \n"
    );
}

/// An append after a skipping key continues from the hash's next integer key.
#[test]
fn test_untyped_property_append_after_skipping_key() {
    let out = compile_and_run(r#"<?php
class N { public $items = []; }
$n = new N();
$n->items[2] = 7;
$n->items[] = 8;
echo json_encode($n->items), "\n";
$n->items[-5] = 9;
$n->items[] = 10;
echo json_encode($n->items), "\n";
$m = new N();
$m->items[] = 1;
$m->items[] = 2;
echo json_encode($m->items), " ", implode(",", $m->items), "\n";
"#);
    assert_eq!(
        out,
        "{\"2\":7,\"3\":8}\n{\"2\":7,\"3\":8,\"-5\":9,\"4\":10}\n[1,2] 1,2\n"
    );
}

/// A non-empty default keeps its entries when a later key skips past its end.
#[test]
fn test_untyped_property_non_empty_default_skipping_key() {
    let out = compile_and_run(r#"<?php
class D {
    public $items = [1, 2];
    public function set(int $k, int $v): void { $this->items[$k] = $v; }
}
$d = new D();
$d->set(1, 20);
echo json_encode($d->items), "\n";
$d->set(5, 3);
echo json_encode($d->items), " ", $d->items[0] + $d->items[5], "\n";
"#);
    assert_eq!(out, "[1,20]\n{\"0\":1,\"1\":20,\"5\":3} 4\n");
}
