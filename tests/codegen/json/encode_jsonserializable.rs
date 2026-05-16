//! Purpose:
//! Provides JsonSerializable encode dispatch tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Implementing classes must call jsonSerialize() before recursive encoding.

use super::*;

#[test]
fn test_jsonserialize_dispatched_for_string_return() {
    let out = compile_and_run(
        r#"<?php
class Custom implements JsonSerializable {
    public string $hidden = "no";
    public function jsonSerialize(): mixed { return "served"; }
}
echo json_encode(new Custom());
"#,
    );
    assert_eq!(out, r#""served""#);
}

#[test]
fn test_jsonserialize_dispatched_for_int_return() {
    let out = compile_and_run(
        r#"<?php
class Wrapped implements JsonSerializable {
    public string $public = "ignored";
    public function jsonSerialize(): mixed { return 42; }
}
echo json_encode(new Wrapped());
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_jsonserialize_dispatched_for_assoc_return() {
    let out = compile_and_run(
        r#"<?php
class Custom implements JsonSerializable {
    public string $hidden = "no";
    public function jsonSerialize(): mixed {
        return ["public" => 1, "from_jsonSerialize" => true];
    }
}
echo json_encode(new Custom());
"#,
    );
    assert_eq!(out, r#"{"public":1,"from_jsonSerialize":true}"#);
}

#[test]
fn test_jsonserialize_dispatched_inside_array() {
    let out = compile_and_run(
        r#"<?php
class Box implements JsonSerializable {
    public int $value;
    public function __construct(int $v) { $this->value = $v; }
    public function jsonSerialize(): mixed { return $this->value * 10; }
}
echo json_encode([new Box(1), new Box(2), new Box(3)]);
"#,
    );
    assert_eq!(out, "[10,20,30]");
}

#[test]
fn test_jsonserialize_dispatched_inside_assoc() {
    let out = compile_and_run(
        r#"<?php
class Tag implements JsonSerializable {
    public string $name;
    public function __construct(string $n) { $this->name = $n; }
    public function jsonSerialize(): mixed { return strtoupper($this->name); }
}
echo json_encode(["a" => new Tag("hi"), "b" => new Tag("world")]);
"#,
    );
    assert_eq!(out, r#"{"a":"HI","b":"WORLD"}"#);
}

#[test]
fn test_jsonserialize_nested_json_encode_does_not_clobber_pretty_flags() {
    let out = compile_and_run(
        r#"<?php
class Bad implements JsonSerializable {
    public function jsonSerialize(): mixed {
        json_encode([1, 2], 0);
        return ["inner" => [3, 4]];
    }
}
echo json_encode(new Bad(), JSON_PRETTY_PRINT);
"#,
    );
    assert_eq!(out, "{\n    \"inner\": [\n        3,\n        4\n    ]\n}");
}

#[test]
fn test_jsonserialize_nested_json_encode_error_does_not_poison_outer_result() {
    let out = compile_and_run(
        r#"<?php
class Noisy implements JsonSerializable {
    public function jsonSerialize(): mixed {
        json_encode(INF);
        return ["ok" => 1];
    }
}
echo json_encode(new Noisy()) . "\n" . json_last_error();
"#,
    );
    assert_eq!(out, "{\"ok\":1}\n0");
}

#[test]
fn test_class_without_jsonserializable_walks_public_props() {
    // Sanity test: a class that does NOT implement JsonSerializable should
    // emit its public properties verbatim, not call any jsonSerialize stub.
    let out = compile_and_run(
        r#"<?php
class Plain {
    public int $a = 1;
    public function jsonSerialize(): mixed { return "should-not-fire"; }
}
echo json_encode(new Plain());
"#,
    );
    assert_eq!(out, r#"{"a":1}"#);
}
