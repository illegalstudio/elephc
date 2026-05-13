use super::*;

#[test]
fn test_json_encode_empty_object() {
    let out = compile_and_run(
        r#"<?php
class Empty1 {}
echo json_encode(new Empty1());
"#,
    );
    assert_eq!(out, "{}");
}

#[test]
fn test_json_encode_object_int_property() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public int $value;
    public function __construct(int $v) { $this->value = $v; }
}
echo json_encode(new Counter(42));
"#,
    );
    assert_eq!(out, r#"{"value":42}"#);
}

#[test]
fn test_json_encode_object_multiple_properties() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public string $name;
    public int $qty;
    public bool $active;
    public function __construct(string $n, int $q, bool $a) {
        $this->name = $n; $this->qty = $q; $this->active = $a;
    }
}
echo json_encode(new Item("widget", 7, true));
"#,
    );
    assert_eq!(out, r#"{"name":"widget","qty":7,"active":true}"#);
}

#[test]
fn test_json_encode_object_string_property_escaping() {
    let out = compile_and_run(
        r#"<?php
class Note {
    public string $text;
    public function __construct(string $t) { $this->text = $t; }
}
echo json_encode(new Note("line\nbreak"));
"#,
    );
    assert_eq!(out, r#"{"text":"line\nbreak"}"#);
}

#[test]
fn test_json_encode_object_skips_private_properties() {
    let out = compile_and_run(
        r#"<?php
class Mixed1 {
    public string $visible = "yes";
    private string $hidden = "no";
}
echo json_encode(new Mixed1());
"#,
    );
    assert_eq!(out, r#"{"visible":"yes"}"#);
}

#[test]
fn test_json_encode_object_skips_protected_properties() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $a = 1;
    protected int $b = 2;
    public int $c = 3;
}
echo json_encode(new Box());
"#,
    );
    assert_eq!(out, r#"{"a":1,"c":3}"#);
}

#[test]
fn test_json_encode_object_with_float_property() {
    let out = compile_and_run(
        r#"<?php
class Vec2 { public float $x = 1.5; public float $y = 2.5; }
echo json_encode(new Vec2());
"#,
    );
    assert_eq!(out, r#"{"x":1.5,"y":2.5}"#);
}

#[test]
fn test_json_encode_nested_objects() {
    let out = compile_and_run(
        r#"<?php
class Inner { public int $x = 5; }
class Outer {
    public Inner $inner;
    public string $tag;
    public function __construct() {
        $this->inner = new Inner();
        $this->tag = "T";
    }
}
echo json_encode(new Outer());
"#,
    );
    assert_eq!(out, r#"{"inner":{"x":5},"tag":"T"}"#);
}

#[test]
fn test_json_encode_array_of_objects() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public int $id;
    public string $name;
    public function __construct(int $i, string $n) {
        $this->id = $i; $this->name = $n;
    }
}
echo json_encode([new Row(1, "a"), new Row(2, "b")]);
"#,
    );
    assert_eq!(out, r#"[{"id":1,"name":"a"},{"id":2,"name":"b"}]"#);
}

#[test]
fn test_json_encode_assoc_with_object_values() {
    let out = compile_and_run(
        r#"<?php
class Item { public int $n = 5; public string $s = "hi"; }
$h = ["a" => new Item(), "b" => new Item()];
echo json_encode($h);
"#,
    );
    assert_eq!(out, r#"{"a":{"n":5,"s":"hi"},"b":{"n":5,"s":"hi"}}"#);
}
