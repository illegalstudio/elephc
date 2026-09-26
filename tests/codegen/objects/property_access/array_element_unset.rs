//! Purpose:
//! Integration tests for `unset()` of an ELEMENT of an array held in an object or static
//! property (issue #750): `unset($this->data[$k])`, `unset($o->items[$k])` and
//! `unset(self::$cache[$k])`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected output was produced by php 8.5 on the same source.
//! - A declared `array` property and an associative property lower directly. The refusals of a
//!   packed-list property and an untyped static array are EIR lowering diagnostics, pinned in
//!   `src/ir_lower/tests/property_element_unset.rs`.

use super::*;

/// The issue's reproduction: an `ArrayAccess` class whose `offsetUnset` removes the key from its
/// private declared-`array` property failed in the backend with an unsupported unset shape.
#[test]
fn test_unset_declared_array_property_element_from_offset_unset() {
    let out = compile_and_run(
        r#"<?php
class Bag implements ArrayAccess {
    private array $data = ["a" => 1, "b" => 2];
    public function offsetExists(mixed $o): bool { return isset($this->data[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->data[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->data[$o] = $v; }
    public function offsetUnset(mixed $o): void { unset($this->data[$o]); }
}
$b = new Bag();
unset($b["a"]);
var_dump(isset($b["a"]));
var_dump(isset($b["b"]));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\n");
}

/// Associative and declared-array properties, through `$this`, an outside receiver and a
/// receiver reached through another object; a missing key is a no-op, surviving keys keep
/// their order and numbering, and a later append continues after the highest key.
#[test]
fn test_unset_property_array_element_keeps_surviving_keys() {
    let out = compile_and_run(
        r#"<?php
class Store {
    public $items = ["x" => "one", "y" => "two", "z" => "three"];
    public array $typed = ["p" => 1, "q" => 2, "r" => 3];
    public function drop(string $k): void { unset($this->items[$k], $this->typed[$k]); }
}
class Holder {
    public Store $store;
    public function __construct() { $this->store = new Store(); }
}
$s = new Store();
$s->drop("y");
$s->drop("q");
unset($s->items["missing"], $s->typed["missing"]);
foreach ($s->items as $k => $v) { echo "$k=$v,"; }
echo "|";
foreach ($s->typed as $k => $v) { echo "$k=$v,"; }
echo "\n";
$h = new Holder();
unset($h->store->items["x"], $h->store->typed["r"]);
echo implode(",", array_keys($h->store->items)), "|", implode(",", array_keys($h->store->typed)), "\n";
$s->typed = [10, 20, 30];
unset($s->typed[1]);
$s->typed[] = 40;
foreach ($s->typed as $k => $v) { echo "$k=$v,"; }
echo "\n";
"#,
    );
    assert_eq!(out, "x=one,z=three,|p=1,r=3,\ny,z|p,q\n0=10,2=30,3=40,\n");
}

/// A copy taken before the removal keeps every element: the property's container is separated
/// before the key is removed. Repeated insert/remove cycles leave only the original keys.
#[test]
fn test_unset_property_array_element_separates_earlier_copy() {
    let out = compile_and_run(
        r#"<?php
class Store {
    public $items = ["x" => 1, "y" => 2];
    public array $typed = ["p" => 1, "q" => 2];
}
$s = new Store();
$items = $s->items;
$typed = $s->typed;
unset($s->items["x"], $s->typed["p"]);
echo count($items), count($typed), count($s->items), count($s->typed), "\n";
echo implode(",", array_keys($items)), "|", implode(",", array_keys($s->items)), "\n";
echo implode(",", array_keys($typed)), "|", implode(",", array_keys($s->typed)), "\n";
for ($i = 0; $i < 3; $i++) {
    $s->items["k$i"] = $i;
    $s->typed["k$i"] = $i;
    unset($s->items["k$i"], $s->typed["k$i"]);
}
$s->items["tail"] = 9;
echo implode(",", array_keys($s->items)), "|", implode(",", array_keys($s->typed)), "\n";
"#,
    );
    assert_eq!(out, "2211\nx,y|y\np,q|q\ny,tail|q\n");
}

/// PHP evaluates the receiver and the key BEFORE it fetches the property for the removal, so a
/// key expression that writes the same property is seen by the unset.
#[test]
fn test_unset_property_array_element_fetches_property_after_key() {
    let out = compile_and_run(
        r#"<?php
class C {
    public $a = ["x" => 1, "y" => 2];
    public array $b = ["x" => 1, "y" => 2];
    function k() { $this->a["z"] = 3; $this->b["z"] = 3; return "x"; }
}
function mk() { echo "mk\n"; return new C; }
function key_of() { echo "key\n"; return "y"; }
$c = new C;
unset($c->a[$c->k()]);
unset($c->b[$c->k()]);
echo implode(",", array_keys($c->a)), "|", implode(",", array_keys($c->b)), "\n";
unset(mk()->a[key_of()]);
"#,
    );
    assert_eq!(out, "y,z|y,z\nmk\nkey\n");
}

/// A removed object's destructor runs during the unset, in PHP's order, and whatever it writes
/// into the same property survives the removal.
#[test]
fn test_unset_property_array_element_keeps_destructor_writes() {
    let out = compile_and_run(
        r#"<?php
class Reg {
    public $objs = [];
    public array $typed = [];
}
class Back {
    public function __construct(public Reg $reg, public string $name) {}
    public function __destruct() {
        echo "destruct {$this->name}\n";
        $this->reg->objs["from_" . $this->name] = "x";
        $this->reg->typed["from_" . $this->name] = "y";
    }
}
$r = new Reg();
$r->objs["a"] = new Back($r, "a");
$r->typed["b"] = new Back($r, "b");
unset($r->objs["a"]);
echo "after objs\n";
unset($r->typed["b"]);
echo implode(",", array_keys($r->objs)), "|", implode(",", array_keys($r->typed)), "\n";
"#,
    );
    assert_eq!(
        out,
        "destruct a\nafter objs\ndestruct b\nfrom_a,from_b|from_a,from_b\n"
    );
}

/// A property holding an `ArrayAccess` object dispatches `unset($this->bag[$k])` to that
/// object's `offsetUnset()`.
#[test]
fn test_unset_array_access_object_property_element_calls_offset_unset() {
    let out = compile_and_run(
        r#"<?php
class Bag implements ArrayAccess {
    private array $data = ["a" => 1, "b" => 2];
    public function offsetExists(mixed $o): bool { return isset($this->data[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->data[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->data[$o] = $v; }
    public function offsetUnset(mixed $o): void { echo "offsetUnset($o)\n"; unset($this->data[$o]); }
    public function keys(): array { return array_keys($this->data); }
}
class Owner {
    public Bag $bag;
    public function __construct() { $this->bag = new Bag(); }
    public function drop(string $k): void { unset($this->bag[$k]); }
}
$o = new Owner();
$o->drop("a");
unset($o->bag["b"]);
echo count($o->bag->keys()), "\n";
"#,
    );
    assert_eq!(out, "offsetUnset(a)\noffsetUnset(b)\n0\n");
}

/// An element of a declared `array` static property is removed through `self::`, `static::` and
/// the class name, and a copy taken before the removal keeps its elements.
#[test]
fn test_unset_static_array_property_element() {
    let out = compile_and_run(
        r#"<?php
class Cache {
    public static array $cache = ["a" => 1, "b" => 2, "c" => 3];
    public static function forget(string $k): void { unset(self::$cache[$k]); }
}
class SubCache extends Cache {
    public static function drop(string $k): void { unset(static::$cache[$k]); }
}
$before = Cache::$cache;
Cache::forget("b");
SubCache::drop("zz");
Cache::$cache["b"] = 9;
SubCache::drop("a");
unset(Cache::$cache["nope"]);
foreach (Cache::$cache as $k => $v) { echo "$k=$v,"; }
echo "|", count($before), "\n";
"#,
    );
    assert_eq!(out, "c=3,b=9,|3\n");
}

/// Property element removal leaves the heap clean, including a caught exception thrown by the
/// removed value's destructor, which must not strand the property's container.
#[test]
fn test_unset_property_array_element_heap_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Boom {
    public function __destruct() { throw new RuntimeException("boom"); }
}
class Store {
    public $items = ["x" => "one"];
    public array $typed = ["p" => "one"];
    public static array $cache = [];
    public function cycle(int $i): void {
        $this->items["k$i"] = "v$i";
        $this->typed["k$i"] = [$i];
        self::$cache["k$i"] = "s$i";
        $copy = $this->items;
        unset($this->items["k$i"], $this->typed["k$i"], self::$cache["k$i"]);
        unset($this->items["missing" . $i]);
        if (count($copy) !== 2) { echo "copy lost an element\n"; }
    }
}
$s = new Store();
for ($i = 0; $i < 50; $i++) {
    $s->cycle($i);
    $s->items["boom"] = new Boom();
    try { unset($s->items["boom"]); } catch (RuntimeException $e) { $s->typed["caught"] = $e->getMessage(); }
    $s->typed["boom"] = new Boom();
    try { unset($s->typed["boom"]); } catch (RuntimeException $e) { unset($s->typed["caught"]); }
}
echo count($s->items), count($s->typed), count(Store::$cache), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "110\n", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
