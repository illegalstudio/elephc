use super::*;

#[test]
fn test_class_chained_property_access() {
    let out = compile_and_run(
        r#"<?php
class Node {
    public $value;
    public $next;
    public function __construct($v) { $this->value = $v; }
}
$a = new Node(1);
$b = new Node(2);
$a->next = $b;
echo $a->next->value;
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_nullsafe_property_access_returns_property_or_null() {
    let out = compile_and_run(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile;
}
$with = new User();
$with->profile = new Profile();
$without = new User();
echo $with->profile?->name ?? "none";
echo "|";
echo $without->profile?->name ?? "none";
"#,
    );
    assert_eq!(out, "Ada|none");
}

#[test]
fn test_nullsafe_method_call_skips_arguments_when_receiver_is_null() {
    let out = compile_and_run(
        r#"<?php
function side() {
    echo "bad";
    return "side";
}
class Box {
    public function label($value): string {
        return $value;
    }
}
?Box $box = null;
echo $box?->label(side()) ?? "none";
"#,
    );
    assert_eq!(out, "none");
}

#[test]
fn test_nullsafe_method_call_evaluates_receiver_before_arguments() {
    let out = compile_and_run(
        r#"<?php
function receiver() {
    echo "receiver|";
    return new Box();
}
function side() {
    echo "arg|";
    return "value";
}
class Box {
    public function label($value): string {
        echo "method|";
        return $value;
    }
}
echo receiver()?->label(side());
"#,
    );
    assert_eq!(out, "receiver|arg|method|value");
}

#[test]
fn test_method_call_evaluates_receiver_before_arguments() {
    let out = compile_and_run(
        r#"<?php
function receiver() {
    echo "receiver|";
    return new Box();
}
function side() {
    echo "arg|";
    return "value";
}
class Box {
    public function label($value): string {
        echo "method|";
        return $value;
    }
}
echo receiver()->label(side());
"#,
    );
    assert_eq!(out, "receiver|arg|method|value");
}

#[test]
fn test_nullsafe_chained_access_short_circuits_each_hop() {
    let out = compile_and_run(
        r#"<?php
class Address {
    public string $city = "Rome";
}
class Profile {
    public ?Address $address;
}
class User {
    public ?Profile $profile;
}
$with = new User();
$profile = new Profile();
$profile->address = new Address();
$with->profile = $profile;
$without = new User();
echo $with?->profile?->address?->city ?? "none";
echo "|";
echo $without?->profile?->address?->city ?? "none";
"#,
    );
    assert_eq!(out, "Rome|none");
}

#[test]
fn test_nullsafe_chained_method_result_short_circuits() {
    let out = compile_and_run(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile;
    public function profile(): ?Profile {
        return $this->profile;
    }
}
$with = new User();
$with->profile = new Profile();
$without = new User();
echo $with?->profile()?->name ?? "none";
echo "|";
echo $without?->profile()?->name ?? "none";
"#,
    );
    assert_eq!(out, "Ada|none");
}

#[test]
fn test_nullsafe_static_null_receiver_keeps_receiver_side_effects() {
    let out = compile_and_run(
        r#"<?php
function none() {
    echo "receiver|";
    return null;
}
function arg() {
    echo "arg|";
    return "value";
}
echo none()?->name ?? "none";
echo "|";
echo none()?->label(arg()) ?? "none";
"#,
    );
    assert_eq!(out, "receiver|none|receiver|none");
}

#[test]
fn test_class_array_of_objects_property_access() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public $name;
    public $price;
    public function __construct($n, $p) { $this->name = $n; $this->price = $p; }
}
$items = [];
$items[] = new Item("Apple", 1);
$items[] = new Item("Banana", 2);
$total = 0;
for ($i = 0; $i < count($items); $i++) {
    $total = $total + $items[$i]->price;
}
echo $total;
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_class_property_array_push() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items;

    public function __construct() {
        $this->items = [1, 2];
    }

    public function add($value) {
        $this->items[] = $value;
    }

    public function last(): int {
        return $this->items[2];
    }
}

$bucket = new Bucket();
$bucket->add(7);
echo $bucket->last();
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_class_property_array_assign() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items;

    public function __construct() {
        $this->items = [1, 2, 3];
    }

    public function replaceFirst($value) {
        $this->items[0] = $value;
    }

    public function first(): int {
        return $this->items[0];
    }
}

$bucket = new Bucket();
$bucket->replaceFirst(9);
echo $bucket->first();
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_class_property_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $value = 10;
}

$counter = new Counter();
$counter->value += 5;
$counter->value *= 3;
echo $counter->value;
"#,
    );
    assert_eq!(out, "45");
}

#[test]
fn test_class_property_compound_assign_evaluates_receiver_once() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $value = 10;
}

function passthrough($counter) {
    echo "r";
    return $counter;
}

$counter = new Counter();
passthrough($counter)->value += 5;
echo ":" . $counter->value;
"#,
    );
    assert_eq!(out, "r:15");
}

#[test]
fn test_class_property_array_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items = [2, 4, 8];
}

$bucket = new Bucket();
$bucket->items[1] += 6;
$bucket->items[2] >>= 1;
echo $bucket->items[1] . "|" . $bucket->items[2];
"#,
    );
    assert_eq!(out, "10|4");
}

#[test]
fn test_class_property_array_compound_assign_evaluates_receiver_and_index_once() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items = [2, 4, 8];
}

function passthrough($bucket) {
    echo "r";
    return $bucket;
}

function idx() {
    echo "i";
    return 2;
}

$bucket = new Bucket();
passthrough($bucket)->items[idx()] -= 3;
echo ":" . $bucket->items[2];
"#,
    );
    assert_eq!(out, "ri:5");
}

#[test]
fn test_readonly_property_null_coalesce_assignment_keeps_initialized_value() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public readonly int $value;

    public function __construct() {
        $this->value = 7;
    }
}

function fallback() {
    echo "fallback";
    return 9;
}

$box = new Box();
$box->value ??= fallback();
echo $box->value;
"#,
    );
    assert_eq!(out, "7");
}

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
