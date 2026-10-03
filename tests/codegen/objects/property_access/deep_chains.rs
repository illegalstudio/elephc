//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object property deep chains, including deep mixed property and array chain, method call array access then property access, and property access on array of objects element.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests deeply chained property access through an array: `$this->palette->colors[$i]->r`.
/// Verifies object→array→property chain where `$palette` is an object, `colors` is an array
/// of Color objects, and `r` is a public property on Color. Compilation and stdout checked.
#[test]
fn test_deep_mixed_property_and_array_chain() {
    let out = compile_and_run(
        r#"<?php
class Color {
    public $r;

    public function __construct($r) {
        $this->r = $r;
    }
}

class Palette {
    public $colors;

    public function __construct() {
        $this->colors = [];
        $this->colors[] = new Color(4);
        $this->colors[] = new Color(9);
    }
}

class Catalog {
    public $palette;

    public function __construct() {
        $this->palette = new Palette();
    }

    public function sample(): int {
        $i = 1;
        return $this->palette->colors[$i]->r;
    }
}

$catalog = new Catalog();
echo $catalog->sample();
"#,
    );
    assert_eq!(out, "9");
}

/// Tests method call returning array, then array-offset access, then property access:
/// `$shop->getItems()[0]->name`. Verifies chained call→array→property chain. Compilation and stdout checked.
#[test]
fn test_method_call_array_access_then_property_access() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }
}

class Shop {
    public $items;

    public function __construct() {
        $this->items = [];
        $this->items[] = new Item("apple");
        $this->items[] = new Item("banana");
    }

    public function getItems() {
        return $this->items;
    }
}

$shop = new Shop();
echo $shop->getItems()[0]->name;
"#,
    );
    assert_eq!(out, "apple");
}

/// Tests property access on an array of objects element: `$this->entries[$i]->name`.
/// Verifies array-of-objects element access and property read. Compilation and stdout checked.
#[test]
fn test_property_access_on_array_of_objects_element() {
    let out = compile_and_run(
        r#"<?php
class Entry {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }
}

class Wad {
    public $entries;

    public function __construct() {
        $this->entries = $this->loadEntries();
    }

    public function loadEntries(): array {
        return [new Entry("PLAYPAL"), new Entry("COLORMAP")];
    }

    public function secondName(): string {
        $i = 1;
        return $this->entries[$i]->name;
    }
}

$wad = new Wad();
echo $wad->secondName();
"#,
    );
    assert_eq!(out, "COLORMAP");
}

/// Tests write to a deeply chained property after array access:
/// `$this->palette->colors[$i]->r = 12`. Verifies object→array→property write chain and read-back.
/// Compilation and stdout checked.
#[test]
fn test_deep_property_assign_after_array_access() {
    let out = compile_and_run(
        r#"<?php
class Color {
    public $r;

    public function __construct($r) {
        $this->r = $r;
    }
}

class Palette {
    public $colors;

    public function __construct() {
        $this->colors = [];
        $this->colors[] = new Color(4);
        $this->colors[] = new Color(9);
    }
}

class Catalog {
    public $palette;

    public function __construct() {
        $this->palette = new Palette();
    }

    public function repaint(): int {
        $i = 1;
        $this->palette->colors[$i]->r = 12;
        return $this->palette->colors[$i]->r;
    }
}

$catalog = new Catalog();
echo $catalog->repaint();
"#,
    );
    assert_eq!(out, "12");
}

/// Tests write to a nested array property after array access:
/// `$this->palette->colors[$i]->shades[1] = 7`. Verifies object→array→object→array write chain and read-back.
/// Compilation and stdout checked.
#[test]
fn test_deep_property_array_assign_after_array_access() {
    let out = compile_and_run(
        r#"<?php
class Color {
    public $shades;

    public function __construct() {
        $this->shades = [1, 2];
    }
}

class Palette {
    public $colors;

    public function __construct() {
        $this->colors = [];
        $this->colors[] = new Color();
    }
}

class Catalog {
    public $palette;

    public function __construct() {
        $this->palette = new Palette();
    }

    public function repaint(): int {
        $i = 0;
        $this->palette->colors[$i]->shades[1] = 7;
        return $this->palette->colors[$i]->shades[1];
    }
}

$catalog = new Catalog();
echo $catalog->repaint();
"#,
    );
    assert_eq!(out, "7");
}

/// Tests push to a nested array property after array access:
/// `$this->palette->colors[$i]->shades[] = 7`. Verifies object→array→object→array push chain and read-back.
/// Compilation and stdout checked.
#[test]
fn test_deep_property_array_push_after_array_access() {
    let out = compile_and_run(
        r#"<?php
class Color {
    public $shades;

    public function __construct() {
        $this->shades = [1, 2];
    }
}

class Palette {
    public $colors;

    public function __construct() {
        $this->colors = [];
        $this->colors[] = new Color();
    }
}

class Catalog {
    public $palette;

    public function __construct() {
        $this->palette = new Palette();
    }

    public function repaint(): int {
        $i = 0;
        $this->palette->colors[$i]->shades[] = 7;
        return $this->palette->colors[$i]->shades[2];
    }
}

$catalog = new Catalog();
echo $catalog->repaint();
"#,
    );
    assert_eq!(out, "7");
}

/// Tests 3-level array chain on a plain PHP array (no objects): `$data[0]["tags"][1]`.
/// Verifies multi-level array-offset chaining with string keys. Compilation and stdout checked.
#[test]
fn test_nested_3_level_chained() {
    let out = compile_and_run(
        r#"<?php
$data = [["tags" => ["php", "rust", "asm"]]];
echo $data[0]["tags"][1];
"#,
    );
    assert_eq!(out, "rust");
}

/// Tests access to a private static property inside its class via `self::$code`.
/// Verifies static property resolution and access within the declaring class context.
/// Compilation and stdout checked.
#[test]
fn test_private_static_property_access_inside_class() {
    let out = compile_and_run(
        r#"<?php
class Secret {
    private static int $code = 7;
    public static function reveal() {
        return self::$code;
    }
}
echo Secret::reveal();
"#,
    );
    assert_eq!(out, "7");
}

/// Writing through a nullable object property link (`$h->next->n += 1` with `?Box $next`) leaves
/// the heap clean. Two references leaked per statement (#1643):
/// - the `$h->next` receiver read out of its slot was never released after the store, which
///   also leaked for a call receiver such as `mk()->n = 5`;
/// - the value boxed for the nullable receiver was copied into the scalar slot and never retired.
///
/// Covers `+=`, `=`, `++`, `.=`, a write through a local alias of the link, and a call receiver,
/// in a loop under `--heap-debug`.
#[test]
fn test_write_through_nullable_property_link_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public int $n = 7; public string $s = 'a'; public ?Box $next = null; }
function mk(): Box { return new Box(); }
function p(Box $h): int {
    if ($h->next === null) { $h->next = new Box(); }
    $h->next->n += 1;
    $h->next->n = $h->next->n + 1;
    ++$h->next->n;
    $h->next->s .= 'b';
    $b = $h->next;
    $b->n += 1;
    return $h->next->n + strlen($h->next->s);
}
$s = 0;
for ($i = 0; $i < 40 + ($argc > 5 ? 1 : 0); $i++) {
    $h = new Box();
    $s += p($h) + p($h) + (mk()->n = 5);
}
echo $s, "\n";
"#,
    );
    assert!(out.success, "program exited non-zero: {}", out.stderr);
    assert_eq!(out.stdout, "1440\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The sibling write shapes of #1643 through an owning receiver leave the heap clean, and the
/// expression form of `??=` on a property yields the value PHP does:
/// - `$h->next->m ??= 5` as a statement and as an expression. The expression form double-freed
///   its probe temp, which read back empty and tripped `--heap-debug`'s bad-refcount check;
/// - property-array element writes, pushes and compound updates through a typed link and a
///   call result;
/// - runtime-name writes `$h->next->{$k} = v` and `mk()->{$k} = v`;
/// - writes that THROW: a weak-mode `TypeError` through the nullable link and through a call
///   result, and a set hook that throws on a call-result receiver.
#[test]
fn test_write_sibling_shapes_through_owning_receivers_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box {
    public int $n = 7;
    public ?int $m = null;
    public array $arr = [1, 2];
    public ?Box $next = null;
}
class G { public Box $b; public function __construct() { $this->b = new Box(); } }
class Hooked { public int $v = 0 { set(int $x) { if ($x < 0) { throw new RuntimeException("neg"); } $this->v = $x; } } }
function mk(): Box { return new Box(); }
function mkhooked(): Hooked { return new Hooked(); }
function linked(): Box { $h = new Box(); $h->next = new Box(); return $h; }
$s = 0;
for ($i = 0; $i < 30 + ($argc > 5 ? 1 : 0); $i++) {
    $h = linked();
    $h->next->m ??= 5;
    $s += $h->next->m;
    $g = linked();
    $s += ($g->next->m ??= 6);
    $c = new G();
    $c->b->arr[0] = 9;
    $c->b->arr[] = 3;
    $c->b->arr[1] += 1;
    $s += $c->b->arr[0] + $c->b->arr[1] + count($c->b->arr);
    mk()->arr[0] = 9;
    mk()->arr[] = 9;
    $k = "n";
    $h->next->{$k} = 2;
    mk()->{$k} = 4;
    $s += $h->next->n;
    $v = $i > 1000 ? 1 : "x";
    try { $h->next->n = $v; } catch (TypeError $e) { $s += 1; }
    try { mk()->n = $v; } catch (TypeError $e) { $s += 1; }
    try { mkhooked()->v = -1; } catch (RuntimeException $e) { $s += 1; }
}
echo $s, "\n";
"#,
    );
    assert!(out.success, "program exited non-zero: {}", out.stderr);
    assert_eq!(out.stdout, "930\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
