use crate::support::*;

// =============================================================================
// Class edge cases
// =============================================================================

#[test]
fn test_class_empty() {
    // Empty class with no properties or methods
    let out = compile_and_run(
        r#"<?php
class Blank {}
$e = new Blank();
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_object_aliasing() {
    // Assigning object to another variable shares the same instance
    let out = compile_and_run(
        r#"<?php
class Box { public $val = 0; }
$a = new Box();
$a->val = 42;
$b = $a;
echo $b->val;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_gc_array_alias_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$b = $a;
unset($a);
echo $b[0];
echo $b[1];
echo $b[2];
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_gc_returned_array_alias_survives_caller_unset() {
    let out = compile_and_run(
        r#"<?php
function share($arr) {
    return $arr;
}

$a = [7, 8];
$b = share($a);
unset($a);
echo $b[0];
echo $b[1];
"#,
    );
    assert_eq!(out, "78");
}

#[test]
fn test_gc_returned_object_alias_survives_caller_unset() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val = 0; }

function share($box) {
    return $box;
}

$a = new Box();
$a->val = 41;
$b = share($a);
unset($a);
echo $b->val;
"#,
    );
    assert_eq!(out, "41");
}

#[test]
fn test_gc_array_push_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [9];
$outer = [];
$outer[] = $inner;
unset($inner);
echo $outer[0][0];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_gc_indexed_array_literal_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [3, 4];
$outer = [$inner];
unset($inner);
echo $outer[0][1];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_array_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4];
$outer = [[1], [2]];
$outer[1] = $inner;
unset($inner);
echo $outer[1][0];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_property_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
class Holder { public $value; }

$inner = [7];
$h = new Holder();
$h->value = $inner;
unset($inner);
$saved = $h->value;
echo $saved[0];
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_gc_static_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
function hold_once() {
    static $saved = [];
    $tmp = [5];
    $saved = $tmp;
    unset($tmp);
    echo $saved[0];
}

hold_once();
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_gc_spread_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [8];
$src = [$inner];
$dst = [...$src];
unset($src);
unset($inner);
echo $dst[0][0];
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_gc_array_merge_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [6];
$left = [$inner];
$right = [[7]];
$merged = array_merge($left, $right);
unset($left);
unset($inner);
echo $merged[0][0] . "|" . $merged[1][0];
"#,
    );
    assert_eq!(out, "6|7");
}

#[test]
fn test_gc_array_chunk_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [5];
$rows = [$inner, [9]];
$chunks = array_chunk($rows, 1);
unset($rows);
unset($inner);
echo $chunks[0][0][0] . "|" . $chunks[1][0][0];
"#,
    );
    assert_eq!(out, "5|9");
}

#[test]
fn test_gc_array_slice_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [2];
$src = [[1], $inner, [3]];
$slice = array_slice($src, 1, 1);
unset($src);
unset($inner);
echo $slice[0][0];
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_gc_array_reverse_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4];
$src = [[1], $inner, [7]];
$rev = array_reverse($src);
unset($src);
unset($inner);
echo $rev[1][0];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_array_pad_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [5];
$src = [[1]];
$padded = array_pad($src, 3, $inner);
unset($src);
unset($inner);
echo $padded[1][0] . "|" . $padded[2][0];
"#,
    );
    assert_eq!(out, "5|5");
}

#[test]
fn test_gc_array_unique_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [3];
$src = [$inner, $inner, [4]];
$uniq = array_unique($src);
unset($src);
unset($inner);
echo count($uniq) . "|" . $uniq[0][0] . "|" . $uniq[1][0];
"#,
    );
    assert_eq!(out, "2|3|4");
}

#[test]
fn test_gc_array_splice_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [7];
$src = [[1], $inner, [9]];
$removed = array_splice($src, 1, 1);
unset($src);
unset($inner);
echo $removed[0][0];
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_gc_array_diff_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [6];
$left = [$inner, [8]];
$right = [[8]];
$diff = array_diff($left, $right);
unset($left);
unset($inner);
echo $diff[0][0];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_gc_array_intersect_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [9];
$left = [[1], $inner];
$right = [$inner];
$both = array_intersect($left, $right);
unset($left);
unset($right);
unset($inner);
echo $both[0][0];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_gc_array_filter_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
function keep_pair($x) { return count($x) == 2; }
$inner = [10, 11];
$rows = [[1], $inner, [2, 3]];
$filtered = array_filter($rows, "keep_pair");
unset($rows);
unset($inner);
echo $filtered[0][1] . "|" . $filtered[1][0];
"#,
    );
    assert_eq!(out, "11|2");
}

#[test]
fn test_gc_array_fill_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [12];
$filled = array_fill(0, 2, $inner);
unset($inner);
echo $filled[0][0] . "|" . $filled[1][0];
"#,
    );
    assert_eq!(out, "12|12");
}

#[test]
fn test_gc_array_combine_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [13];
$keys = ["keep"];
$vals = [$inner];
$map = array_combine($keys, $vals);
unset($vals);
unset($inner);
$saved = $map["keep"];
echo $saved[0];
"#,
    );
    assert_eq!(out, "13");
}

#[test]
fn test_gc_array_fill_keys_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [14];
$keys = ["a", "b"];
$map = array_fill_keys($keys, $inner);
unset($inner);
$first = $map["a"];
$second = $map["b"];
echo $first[0] . "|" . $second[0];
"#,
    );
    assert_eq!(out, "14|14");
}

#[test]
fn test_class_constructor_calls_method() {
    // Constructor calling another method on the same object
    let out = compile_and_run(
        r#"<?php
class Init { public $ready = 0;
    public function __construct() { $this->setup(); }
    public function setup() { $this->ready = 1; }
}
$i = new Init();
echo $i->ready;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_class_multiple_classes_composing() {
    // Two classes where one holds an instance of the other
    let out = compile_and_run(
        r#"<?php
class Address { public $city;
    public function __construct($c) { $this->city = $c; }
}
class Person { public $name; public $address;
    public function __construct($n, $addr) { $this->name = $n; $this->address = $addr; }
    public function info() { return $this->name . " from " . $this->address->city; }
}
$addr = new Address("Rome");
$p = new Person("Marco", $addr);
echo $p->info();
"#,
    );
    assert_eq!(out, "Marco from Rome");
}

#[test]
fn test_class_empty_string_property() {
    // Empty string property and strlen on it
    let out = compile_and_run(
        r#"<?php
class Tag { public $label = "";
    public function __construct($l) { $this->label = $l; }
}
$t = new Tag("");
echo strlen($t->label) . "|" . $t->label . "|done";
"#,
    );
    assert_eq!(out, "0||done");
}

#[test]
fn test_class_long_string_property() {
    // String property holding a long (1000 char) string
    let out = compile_and_run(
        r#"<?php
class Buffer { public $data;
    public function __construct($d) { $this->data = $d; }
}
$b = new Buffer(str_repeat("x", 1000));
echo strlen($b->data);
"#,
    );
    assert_eq!(out, "1000");
}

#[test]
fn test_class_string_concat_in_method() {
    // Method concatenating multiple string properties
    let out = compile_and_run(
        r#"<?php
class Row { public $a; public $b; public $c;
    public function __construct($a, $b, $c) { $this->a = $a; $this->b = $b; $this->c = $c; }
    public function csv() { return $this->a . "," . $this->b . "," . $this->c; }
}
$r = new Row("x", "y", "z");
echo $r->csv();
"#,
    );
    assert_eq!(out, "x,y,z");
}

#[test]
fn test_class_bool_property() {
    // Boolean property used in ternary
    let out = compile_and_run(
        r#"<?php
class Flag { public $on;
    public function __construct($v) { $this->on = $v; }
}
$f = new Flag(true);
echo $f->on ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_class_array_property() {
    // Array property with count()
    let out = compile_and_run(
        r#"<?php
class Stack { public $items;
    public function __construct() { $this->items = [1, 2, 3]; }
    public function size() { return count($this->items); }
}
$s = new Stack();
echo $s->size();
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_class_1000_objects_in_loop() {
    // Stress test: create 1000 objects in a loop
    let out = compile_and_run(
        r#"<?php
class Obj { public $id;
    public function __construct($id) { $this->id = $id; }
}
$last = new Obj(0);
for ($i = 1; $i < 1000; $i++) {
    $last = new Obj($i);
}
echo $last->id;
"#,
    );
    assert_eq!(out, "999");
}

#[test]
fn test_class_many_properties() {
    // Object with 10 properties and a method summing them
    let out = compile_and_run(
        r#"<?php
class Big { public $a; public $b; public $c; public $d; public $e;
    public $f; public $g; public $h; public $i; public $j;
    public function __construct() {
        $this->a = 1; $this->b = 2; $this->c = 3; $this->d = 4; $this->e = 5;
        $this->f = 6; $this->g = 7; $this->h = 8; $this->i = 9; $this->j = 10;
    }
    public function sum() {
        return $this->a + $this->b + $this->c + $this->d + $this->e +
               $this->f + $this->g + $this->h + $this->i + $this->j;
    }
}
$b = new Big();
echo $b->sum();
"#,
    );
    assert_eq!(out, "55");
}

#[test]
fn test_magic_tostring_supports_echo_concat_and_cast() {
    let out = compile_and_run(
        r#"<?php
class User {
    public $name;
    public function __construct($name) { $this->name = $name; }
    public function __toString() { return "@" . $this->name; }
}
$u = new User("nahime");
echo $u;
echo "|" . $u;
echo "|" . (string)$u;
"#,
    );
    assert_eq!(out, "@nahime|@nahime|@nahime");
}

#[test]
fn test_magic_tostring_missing_method_is_runtime_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Plain {}
$p = new Plain();
echo $p;
"#,
    );
    assert!(err.contains("could not be converted to string"), "{err}");
}

#[test]
fn test_magic_get_handles_missing_property_reads() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public function __get($name) {
        return "[" . $name . "]";
    }
}
$b = new Bag();
echo $b->title . "|" . $b->slug;
"#,
    );
    assert_eq!(out, "[title]|[slug]");
}

#[test]
fn test_magic_get_merges_return_types_across_top_level_branches() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public $flip = false;
    public function __get($name) {
        if ($this->flip) {
            return "[" . $name . "]";
        }
        $this->flip = true;
        return 123;
    }
}
$b = new Bag();
echo $b->id . "|" . $b->slug;
"#,
    );
    assert_eq!(out, "123|[slug]");
}

#[test]
fn test_magic_set_handles_missing_property_writes() {
    let out = compile_and_run(
        r#"<?php
class Recorder {
    public $log = "";
    public function __set($name, $value) {
        $this->log = $this->log . $name . "=" . $value . ";";
    }
}
$r = new Recorder();
$r->count = 42;
$r->label = "ok";
echo $r->log;
"#,
    );
    assert_eq!(out, "count=42;label=ok;");
}

#[test]
fn test_magic_get_and_set_can_work_together() {
    let out = compile_and_run(
        r#"<?php
class Meta {
    public $last = "";
    public function __set($name, $value) { $this->last = $name . ":" . $value; }
    public function __get($name) { return $this->last . "|" . $name; }
}
$m = new Meta();
$m->answer = 99;
echo $m->answer;
"#,
    );
    assert_eq!(out, "answer:99|answer");
}

// =============================================================================
// Non-class regression edge cases
// =============================================================================

#[test]
fn test_deeply_nested_string_function_calls() {
    // Deeply nested function calls building nested HTML tags
    let out = compile_and_run(
        r#"<?php
function wrap($s, $tag) { return "<" . $tag . ">" . $s . "</" . $tag . ">"; }
echo wrap(wrap(wrap("hello", "b"), "i"), "p");
"#,
    );
    assert_eq!(out, "<p><i><b>hello</b></i></p>");
}

#[test]
fn test_recursive_string_building() {
    // Recursive function that builds a string via concatenation
    let out = compile_and_run(
        r#"<?php
function repeat_str($s, $n) {
    if ($n <= 0) { return ""; }
    return $s . repeat_str($s, $n - 1);
}
echo repeat_str("ab", 5);
"#,
    );
    assert_eq!(out, "ababababab");
}

#[test]
fn test_closure_capturing_object() {
    // Closure capturing an object via use()
    let out = compile_and_run(
        r#"<?php
class Counter { public $n = 0; public function inc() { $this->n = $this->n + 1; } }
$c = new Counter();
$c->inc();
$c->inc();
$fn = function() use ($c) { return $c; };
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_float_property_via_method() {
    let out = compile_and_run(
        r#"<?php
class Circle {
    public $radius;
    public function __construct($r) { $this->radius = $r; }
    public function area() { return 3.14159 * $this->radius * $this->radius; }
}
$c = new Circle(5.0);
echo $c->area();
"#,
    );
    assert_eq!(out, "78.53975");
}

#[test]
fn test_class_method_returns_float_property() {
    let out = compile_and_run(
        r#"<?php
class Foo {
    public $x;
    public function __construct($v) { $this->x = $v; }
    public function getX() { return $this->x; }
}
$f = new Foo(3.14);
echo $f->getX();
"#,
    );
    assert_eq!(out, "3.14");
}

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
fn test_class_static_method_string_param() {
    let out = compile_and_run(
        r#"<?php
class Utils {
    public static function greet($name) { return "Hello " . $name; }
}
echo Utils::greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_class_method_returns_this() {
    let out = compile_and_run(
        r#"<?php
class Builder {
    public $parts = "";
    public function add($s) { $this->parts = $this->parts . $s; return $this; }
}
$b = new Builder();
$b->add("hello");
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_private_property_via_method() {
    let out = compile_and_run(
        r#"<?php
class Secret {
    private $value;
    public function __construct($value) { $this->value = $value; }
    public function reveal() { return $this->value; }
}
$s = new Secret("ok");
echo $s->reveal();
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_readonly_property() {
    let out = compile_and_run(
        r#"<?php
class User {
    public readonly $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$u = new User(7);
echo $u->id();
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_class_static_and_instance() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $n;
    public function __construct($n) { $this->n = $n; }
    public function next() { return $this->n + 1; }
    public static function make($n) { return new Counter($n); }
}
$c = Counter::make(4);
echo $c->next();
"#,
    );
    assert_eq!(out, "5");
}

// === Nested array access tests ===

#[test]
fn test_nested_indexed_assoc_direct() {
    let out = compile_and_run(
        r#"<?php
$data = [["name" => "Alice"]];
echo $data[0]["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_nested_assoc_indexed() {
    let out = compile_and_run(
        r#"<?php
$map = ["items" => [10, 20, 30]];
$items = $map["items"];
echo $items[1];
"#,
    );
    assert_eq!(out, "20");
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
fn test_nested_int_assoc_in_indexed() {
    let out = compile_and_run(
        r#"<?php
$scores = [["math" => 90, "eng" => 85]];
$s = $scores[0];
echo $s["math"] . "|" . $s["eng"];
"#,
    );
    assert_eq!(out, "90|85");
}

#[test]
fn test_nested_string_assoc_loop() {
    let out = compile_and_run(
        r#"<?php
$contacts = [
    ["name" => "Alice", "email" => "alice@test"],
    ["name" => "Bob", "email" => "bob@test"]
];
for ($i = 0; $i < 2; $i++) {
    $c = $contacts[$i];
    echo $c["name"] . "|" . $c["email"] . "\n";
}
"#,
    );
    assert_eq!(out, "Alice|alice@test\nBob|bob@test\n");
}

#[test]
fn test_nested_assoc_of_indexed() {
    let out = compile_and_run(
        r#"<?php
$groups = ["fruits" => ["apple", "banana"], "vegs" => ["carrot", "pea"]];
$f = $groups["fruits"];
echo $f[0] . "|" . $f[1];
"#,
    );
    assert_eq!(out, "apple|banana");
}

#[test]
fn test_nested_dynamic_building() {
    let out = compile_and_run(
        r#"<?php
function make_user($name, $email) {
    return ["name" => $name, "email" => $email];
}
$users = [];
$users[] = make_user("Alice", "a@t");
$users[] = make_user("Bob", "b@t");
for ($i = 0; $i < count($users); $i++) {
    $u = $users[$i];
    echo $u["name"] . "|" . $u["email"] . "\n";
}
"#,
    );
    assert_eq!(out, "Alice|a@t\nBob|b@t\n");
}

#[test]
fn test_nested_explode_to_assoc() {
    let out = compile_and_run(
        r#"<?php
function parse_row($line) {
    $parts = explode("|", $line);
    return ["name" => $parts[0], "email" => $parts[1]];
}
$r = parse_row("Alice|alice@test");
echo $r["name"] . " <" . $r["email"] . ">";
"#,
    );
    assert_eq!(out, "Alice <alice@test>");
}

#[test]
fn test_nested_foreach_of_assoc() {
    let out = compile_and_run(
        r#"<?php
$people = [["name" => "Alice"], ["name" => "Bob"]];
foreach ($people as $p) {
    echo $p["name"] . " ";
}
"#,
    );
    assert_eq!(out, "Alice Bob ");
}

#[test]
fn test_nested_objects_in_assoc() {
    let out = compile_and_run(
        r#"<?php
class Item { public $name;
    public function __construct($n) { $this->name = $n; }
}
$data = ["items" => [new Item("Sword"), new Item("Shield")]];
$items = $data["items"];
$first = $items[0];
echo $first->name;
"#,
    );
    assert_eq!(out, "Sword");
}

#[test]
fn test_switch_return_string() {
    let out = compile_and_run(
        r#"<?php
function classify($n) {
    switch ($n % 3) {
        case 0: return "fizz";
        case 1: return "buzz";
        default: return "none";
    }
}
$r = classify(0);
echo $r . " ";
$r = classify(1);
echo $r . " ";
$r = classify(2);
echo $r;
"#,
    );
    assert_eq!(out, "fizz buzz none");
}

#[test]
fn test_switch_return_int() {
    let out = compile_and_run(
        r#"<?php
function score($grade) {
    switch ($grade) {
        case 1: return 100;
        case 2: return 80;
        case 3: return 60;
        default: return 0;
    }
}
echo score(1) . "|" . score(2) . "|" . score(3) . "|" . score(9);
"#,
    );
    assert_eq!(out, "100|80|60|0");
}
