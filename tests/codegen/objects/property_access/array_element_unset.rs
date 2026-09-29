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
//! - A declared `array` property, an associative property and a boxed `Mixed` property lower
//!   directly; an `ArrayAccess` property dispatches to `offsetUnset()` after the key (also when
//!   the object sits in a boxed `mixed` cell), and a readonly array property throws. The refusals of a packed-list property and an untyped
//!   static array are EIR lowering diagnostics, pinned in
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

/// PHP fetches an `ArrayAccess` property only after the key of `unset($o->bag[$key])`: a key
/// expression that stores a NEW object into the property reaches that object's `offsetUnset()`,
/// for a variable receiver, a call receiver (evaluated first) and `$this`. A plain variable key
/// and a literal key keep the same dispatch. Before the fix the synthetic `offsetUnset` call read
/// the property first and removed the key from the replaced object.
#[test]
fn test_unset_array_access_property_element_fetches_property_after_key() {
    let out = compile_and_run(
        r#"<?php
class Bag implements ArrayAccess {
    public function __construct(public string $name) {}
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetSet(mixed $o, mixed $v): void {}
    public function offsetUnset(mixed $o): void { echo "offsetUnset($o) on {$this->name}\n"; }
}
class Holder {
    public Bag $bag;
    public function __construct() { $this->bag = new Bag("first"); }
    public function drop(): void { unset($this->bag[$this->replace("inner")]); }
    public function replace(string $name): string { echo "key\n"; $this->bag = new Bag($name); return "k"; }
}
function mk(Holder $h): Holder { echo "receiver\n"; return $h; }
$h = new Holder();
unset($h->bag[$h->replace("second")]);
unset(mk($h)->bag[$h->replace("third")]);
$h->drop();
$k = "plain";
unset($h->bag[$k], $h->bag["literal"]);
echo $h->bag->name, "\n";
"#,
    );
    assert_eq!(
        out,
        "key\noffsetUnset(k) on second\nreceiver\nkey\noffsetUnset(k) on third\nkey\n\
         offsetUnset(k) on inner\noffsetUnset(plain) on inner\noffsetUnset(literal) on inner\ninner\n"
    );
}

/// An element unset on an initialized readonly array property raises PHP's
/// "Cannot indirectly modify" `Error` naming the declaring class, from inside and outside the
/// class, for a missing key too, and after the key was evaluated; an uninitialized readonly
/// property is left alone.
#[test]
fn test_unset_readonly_array_property_element_throws() {
    let out = compile_and_run(
        r#"<?php
class P {
    public readonly array $ro;
    public function __construct() {
        try { unset($this->ro["a"]); echo "uninitialized: no-op\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
        $this->ro = ["a" => 1, "b" => 2];
        try { unset($this->ro["a"]); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
    }
    public function drop(string $k): void { unset($this->ro[$k]); }
}
class Q extends P {}
function key_of(string $k): string { echo "key $k\n"; return $k; }
$q = new Q();
try { $q->drop("b"); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { unset($q->ro[key_of("missing")]); } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo implode(",", array_keys($q->ro)), "\n";
"#,
    );
    assert_eq!(
        out,
        "uninitialized: no-op\nError: Cannot indirectly modify readonly property P::$ro\n\
         Cannot indirectly modify readonly property P::$ro\nkey missing\n\
         Cannot indirectly modify readonly property P::$ro\na,b\n"
    );
}

/// A property stored as a boxed `Mixed` cell lowers too: an untyped property widened by
/// `unset($w->items)`, a `?array` and a `mixed` property. An element unset of a removed property
/// recreates it as null, as PHP does; null is a no-op, a string and the other scalars raise PHP's
/// `Error`s, and an earlier copy keeps its elements.
#[test]
fn test_unset_boxed_mixed_property_element() {
    let out = compile_and_run(
        r#"<?php
class W {
    public $items = ["a" => 1, "b" => 2];
    public ?array $maybe = null;
    public mixed $any = null;
}
$w = new W();
unset($w->items);
unset($w->items["gone"]);
var_dump($w->items);
echo implode(",", array_keys(get_object_vars($w))), "\n";
$w->items = ["x" => 1, "y" => 2];
unset($w->items["x"]);
echo implode(",", array_keys($w->items)), "\n";
$w->items = [10, 20, 30];
$copy = $w->items;
unset($w->items[1]);
echo json_encode($w->items), " ", json_encode($copy), "\n";
unset($w->maybe["k"]);
$w->maybe = ["k" => 1, "j" => 2];
unset($w->maybe["k"]);
echo json_encode($w->maybe), "\n";
foreach ([null, "str", 7, 1.5, true, ["p" => 1, "q" => 2]] as $value) {
    $w->any = $value;
    try {
        unset($w->any["p"]);
        echo "ok ", json_encode($w->any), "\n";
    } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "NULL\nitems,maybe,any\ny\n{\"0\":10,\"2\":30} [10,20,30]\n{\"j\":2}\nok null\n\
         Error: Cannot unset string offsets\nError: Cannot unset offset in a non-array variable\n\
         Error: Cannot unset offset in a non-array variable\n\
         Error: Cannot unset offset in a non-array variable\nok {\"q\":2}\n"
    );
}

/// The `ArrayAccess`, readonly and boxed-`Mixed` element paths leave the heap clean, including an
/// `offsetUnset()` that throws while the property holds a fresh object each iteration (the
/// fetched object is parked in an owner slot the unwinder releases) and a readonly refusal on a
/// call receiver.
#[test]
fn test_unset_property_element_review_paths_heap_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bag implements ArrayAccess {
    public array $data = [];
    public function __construct(public string $name) {}
    public function offsetExists(mixed $o): bool { return isset($this->data[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->data[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->data[$o] = $v; }
    public function offsetUnset(mixed $o): void {
        if ($o === "boom") { throw new RuntimeException("boom " . $this->name); }
        unset($this->data[$o]);
    }
}
class Holder {
    public Bag $bag;
    public $items = ["a" => "x"];
    public ?array $maybe = null;
    public function __construct(public readonly array $ro = ["k" => "v"]) { $this->bag = new Bag("first"); }
}
function swap(Holder $h, int $i): string { $h->bag = new Bag("n" . $i); $h->bag["k$i"] = "v$i"; return "k$i"; }
function mk(Holder $h): Holder { return $h; }
$h = new Holder();
unset($h->items);
for ($i = 0; $i < 40; $i++) {
    unset($h->bag[swap($h, $i)]);
    unset(mk($h)->bag["x" . $i]);
    try { unset($h->bag["bo" . ($i < 100 ? "om" : "")]); } catch (RuntimeException $e) { }
    try { unset(mk($h)->ro["k" . $i]); } catch (Error $e) { }
    $h->items = ["p$i" => "q$i", "r" => [$i]];
    unset($h->items["p$i"], $h->items["zz"]);
    unset($h->items);
    unset($h->items["gone" . $i]);
    $h->items = "str$i";
    try { unset($h->items[0]); } catch (Error $e) { }
    $h->maybe = ["m$i" => new Bag("m")];
    unset($h->maybe["m$i"]);
    $h->maybe = null;
    unset($h->maybe["m$i"]);
}
unset($e);
echo count($h->bag->data), count($h->ro), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "01\n", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A variable receiver is read after the key, as PHP's delayed unset fetch does: a key that
/// reassigns the variable removes the element from the NEW object, and the old one keeps it.
/// Before, the receiver was read first, which also left a dangling receiver when the key dropped
/// the last reference. Review follow-up for #750.
#[test]
fn test_unset_property_element_reads_variable_receiver_after_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public array $items = ["x" => 1, "y" => 2]; }
function run(): void {
    $c = new C();
    $other = new C();
    $keep = $c;
    unset($c->items[(($c = $other) === null ? "y" : "x")]);
    echo "1:", implode(",", array_keys($keep->items)), "|", implode(",", array_keys($other->items)), "\n";
}
run();
"#,
    );
    assert!(out.success, "program exited non-zero: {}", out.stderr);
    assert_eq!(out.stdout, "1:x,y|y\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Removing an element of a typed property that was never initialized is a silent no-op, as in
/// PHP, instead of the write fetch's "must not be accessed before initialization" Error.
/// Review follow-up for #750.
#[test]
fn test_unset_element_of_uninitialized_typed_property_is_a_no_op() {
    let out = compile_and_run(
        r#"<?php
class U { public array $items; public mixed $m; }
$u = new U();
try { unset($u->items["k"]); echo "2:silent\n"; } catch (Error $e) { echo "2:", $e->getMessage(), "\n"; }
try { unset($u->m["k"]); echo "3:silent\n"; } catch (Error $e) { echo "3:", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "2:silent\n3:silent\n");
}

/// A readonly property holding an `ArrayAccess` object still has its `offsetUnset()` called:
/// readonly forbids reassigning the property, not mutating the object it holds. A plain object in
/// a boxed `mixed` property refuses the removal with PHP's class-naming message.
/// Review follow-up for #750.
#[test]
fn test_unset_element_through_readonly_array_access_and_mixed_object() {
    let out = compile_and_run(
        r#"<?php
class Bag implements ArrayAccess {
    public array $data = ["a" => 1];
    public function offsetExists(mixed $o): bool { return isset($this->data[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->data[$o] ?? null; }
    public function offsetSet(mixed $o, mixed $v): void { $this->data[$o] = $v; }
    public function offsetUnset(mixed $o): void { echo "offsetUnset\n"; unset($this->data[$o]); }
}
class C { public readonly Bag $bag; public function __construct() { $this->bag = new Bag(); } }
$c = new C();
try { unset($c->bag["a"]); echo "ok\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
class W { public mixed $any = null; }
$w = new W();
$w->any = new stdClass;
try { unset($w->any["p"]); echo "ok\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "offsetUnset\nok\nError: Cannot use object of type stdClass as array\n"
    );
}

/// An `ArrayAccess` object reached through a boxed `mixed` or untyped property, or a by-reference
/// `mixed` local, has its `offsetUnset()` called. That covers a PHP class (with an inherited
/// method), `SplFixedArray` and `SplDoublyLinkedList`, while a `stdClass` still raises PHP's
/// class-naming `Error`. A typed `ArrayAccess` property that was never initialized is left alone,
/// as PHP's quiet unset fetch does. Runs under `--heap-debug`: a PHP method borrows the boxed
/// offset and the SPL helpers consume it. Review follow-up for #750.
#[test]
fn test_unset_offset_of_array_access_object_in_boxed_cell_calls_offset_unset() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bag implements ArrayAccess {
    public array $d = ["a" => 1, "b" => 2, 3 => "three"];
    public function offsetExists(mixed $o): bool { return isset($this->d[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->d[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->d[$o] = $v; }
    public function offsetUnset(mixed $o): void { echo "offsetUnset(", var_export($o, true), ")\n"; unset($this->d[$o]); }
}
class SubBag extends Bag {}
class Typed { public Bag $bag; }
class Loose { public $bag; public mixed $m; }
function drop_key(mixed &$r, $k): void { unset($r[$k]); }

$t = new Typed();
unset($t->bag["a"]);
echo "typed uninitialized: no-op\n";
$t->bag = new Bag();
unset($t->bag["a"]);

$l = new Loose();
$l->bag = new SubBag();
$l->m = new Bag();
unset($l->bag["b"], $l->bag[3]);
unset($l->m["a"]);
echo implode(",", array_keys($l->bag->d)), "|", implode(",", array_keys($l->m->d)), "\n";

$fixed = new SplFixedArray(3);
$fixed[0] = "x"; $fixed[1] = "y";
$l->m = $fixed;
unset($l->m[1]);
var_dump($fixed[1]);

$list = new SplDoublyLinkedList();
$list->push("p"); $list->push("q"); $list->push("r");
$l->m = $list;
unset($l->m[0]);
echo count($list), " ", $list[0], "\n";

$l->m = new stdClass();
try { unset($l->m["k"]); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }

$r = new Bag();
drop_key($r, "b");
echo implode(",", array_keys($r->d)), "\n";

$n = 0;
for ($i = 0; $i < 50 + ($argc > 5 ? 1 : 0); $i++) {
    $l->m = new Bag();
    ob_start();
    unset($l->m["a"], $l->m[3]);
    ob_end_clean();
    $n += count($l->m->d);
}
echo $n, "\n";
"#,
    );
    assert!(out.success, "program exited non-zero: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "typed uninitialized: no-op\noffsetUnset('a')\noffsetUnset('b')\noffsetUnset(3)\n",
            "offsetUnset('a')\na|b,3\nNULL\n2 q\n",
            "Error: Cannot use object of type stdClass as array\n",
            "offsetUnset('b')\na,3\n50\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Removing an offset of a `false` held in a boxed `mixed` property leaves it `false`. PHP also
/// prints an `Automatic conversion of false to array` deprecation, which elephc does not (see
/// `docs/php/types.md`). Review follow-up for #750.
#[test]
fn test_unset_offset_of_false_mixed_property_keeps_false() {
    let out = compile_and_run(
        r#"<?php
class F { public mixed $m = false; }
$f = new F();
unset($f->m["k"]);
var_dump($f->m);
"#,
    );
    assert_eq!(out, "bool(false)\n");
}

/// `offsetUnset()` receives the offset exactly as written when the object sits in a boxed
/// property: `"12"` stays a string, `1.5` a float, `true` a bool and `null` null, rather than the
/// normalized array key. An `SplFixedArray` in the same cell still accepts the numeric string
/// `"1"`. Review follow-up for #750.
#[test]
fn test_unset_offset_of_boxed_array_access_passes_the_original_key() {
    let out = compile_and_run(
        r#"<?php
class Bag implements ArrayAccess {
    public array $log = [];
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetSet(mixed $o, mixed $v): void {}
    public function offsetUnset(mixed $o): void { $this->log[] = gettype($o) . ":" . var_export($o, true); }
}
class Loose { public $bag; }
class Holder { public mixed $bag; }
$l = new Loose();
$l->bag = new Bag();
$m = $argc > 50 ? 3 : "k" . $argc;
unset($l->bag["12"], $l->bag[12], $l->bag[1.5], $l->bag[true], $l->bag[null], $l->bag[$m], $l->bag["x" . $argc]);
echo implode(" ", $l->bag->log), "\n";
$f = new SplFixedArray(3);
$f[1] = "one";
$h = new Holder();
$h->bag = $f;
unset($h->bag["1"]);
var_dump($f[1]);
"#,
    );
    assert_eq!(
        out,
        "string:'12' integer:12 double:1.5 boolean:true NULL:NULL string:'k1' string:'x1'\nNULL\n"
    );
}

/// Boxing the original offset for `offsetUnset()` leaves the heap clean over a loop, for literal,
/// computed and `mixed` keys alike. Review follow-up for #750.
#[test]
fn test_unset_offset_of_boxed_array_access_original_key_heap_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bag implements ArrayAccess {
    public array $log = [];
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetSet(mixed $o, mixed $v): void {}
    public function offsetUnset(mixed $o): void { $this->log[] = gettype($o); }
}
class Loose { public $bag; }
function run(int $argc): string {
    $l = new Loose();
    $l->bag = new Bag();
    $m = $argc > 50 ? 3 : "k" . $argc;
    unset($l->bag["12"], $l->bag[12], $l->bag[1.5], $l->bag[true], $l->bag[null], $l->bag[$m], $l->bag["x" . $argc]);
    return implode(" ", $l->bag->log);
}
$out = "";
for ($i = 0; $i < 40 + ($argc > 5 ? 1 : 0); $i++) { $out = run($argc); }
echo $out, "\n";
"#,
    );
    assert!(out.success, "program exited non-zero: {}", out.stderr);
    assert_eq!(out.stdout, "string integer double boolean NULL string string\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A float offset reaches a boxed `ArrayAccess` object's `offsetUnset()` without PHP's float-to-int
/// deprecation, which PHP only raises for a real array key. The warning used to fire while the key
/// was normalized for the SPL arm, and an error handler that replaced the property ran between the
/// payload check and the receiver read, which then treated `null` as the object. Review follow-up
/// for #750.
#[test]
fn test_unset_float_offset_of_boxed_array_access_raises_no_deprecation() {
    let out = compile_and_run_capture(
        r#"<?php
class B implements ArrayAccess {
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetSet(mixed $o, mixed $v): void {}
    public function offsetUnset(mixed $o): void { echo "unset:", var_export($o, true), "\n"; }
}
class L { public $bag; }
$l = new L();
$l->bag = new B();
unset($l->bag[1.5]);
$m = $argc > 5 ? 1 : 2.5;
unset($l->bag[$m]);
set_error_handler(function () use ($l) { $l->bag = null; return true; });
unset($l->bag[1.5]);
echo "survived\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "unset:1.5\nunset:2.5\nunset:1.5\nsurvived\n");
    assert!(!out.stderr.contains("Deprecated"), "{}", out.stderr);
}
