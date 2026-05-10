//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object property deep chains, including deep mixed property and array chain, method call array access then property access, and property access on array of objects element.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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
