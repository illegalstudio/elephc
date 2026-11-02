//! Purpose:
//! Provides JSON codegen test module wiring.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Submodules are grouped by JSON surface area so focused filters can run one feature slice.

use crate::support::*;

#[path = "json/compatibility_constants.rs"]
mod compatibility_constants;
#[path = "json/constants.rs"]
mod constants;
#[path = "json/last_error.rs"]
mod last_error;
#[path = "json/last_error_msg.rs"]
mod last_error_msg;
#[path = "json/validate.rs"]
mod validate;
#[path = "json/extended_signatures.rs"]
mod extended_signatures;
#[path = "json/jsonserializable.rs"]
mod jsonserializable;
#[path = "json/encode_object.rs"]
mod encode_object;
#[path = "json/encode_jsonserializable.rs"]
mod encode_jsonserializable;
#[path = "json/exception.rs"]
mod exception;
#[path = "json/encode_flags.rs"]
mod encode_flags;
#[path = "json/encode_inf_nan.rs"]
mod encode_inf_nan;
#[path = "json/encode_float_precision.rs"]
mod encode_float_precision;
#[path = "json/encode_depth.rs"]
mod encode_depth;
#[path = "json/encode_invalid_utf8.rs"]
mod encode_invalid_utf8;
#[path = "json/encode_control_chars.rs"]
mod encode_control_chars;
#[path = "json/encode_list_shape.rs"]
mod encode_list_shape;
#[path = "json/encode_integer_tree.rs"]
mod encode_integer_tree;
#[path = "json/decode_mixed.rs"]
mod decode_mixed;
#[path = "json/decode_stdclass.rs"]
mod decode_stdclass;
#[path = "json/mixed_index_access.rs"]
mod mixed_index_access;
#[path = "json/decode_errors.rs"]
mod decode_errors;
#[path = "json/decode_bigint.rs"]
mod decode_bigint;
#[path = "json/case_insensitive.rs"]
mod case_insensitive;
#[path = "json/evaluation_order.rs"]
mod evaluation_order;
#[path = "json/error_location.rs"]
mod error_location;
#[path = "json/diagnostic_regressions.rs"]
mod diagnostic_regressions;

#[path = "json/coordinate_regressions.rs"]
mod coordinate_regressions;

#[path = "json/token_dispatch.rs"]
mod token_dispatch;

#[path = "json/packed_integer.rs"]
mod packed_integer;
#[path = "json/encode_large_string.rs"]
mod encode_large_string;
#[path = "json/decode_ownership.rs"]
mod decode_ownership;

/// `json_decode()` takes depth and flags forwarded from untyped parameters
/// (`Filesystem::json($path, $flags = 0)`). Expected output verified against php 8.5.
#[test]
fn test_json_decode_accepts_depth_and_flags_forwarded_from_parameters() {
    let out = compile_and_run(
        r#"<?php
class F { function json($s, $flags = 0) { return json_decode($s, true, 512, $flags); } function d($s, $assoc = false, $depth = 512) { return json_decode($s, $assoc, $depth); } }
$f = new F;
var_dump($f->json('{"a":1}'), $f->json('{"a":1}', JSON_BIGINT_AS_STRING), $f->d('[1,[2]]', true), $f->d('[1,[2]]', true, 1));
function g($x, $fl) { return json_decode($x, false, 512, $fl); }
var_dump(g('{"a":"b"}', 0)->a);
"#,
    );
    assert_eq!(
        out,
        r#"array(1) {
  ["a"]=>
  int(1)
}
array(1) {
  ["a"]=>
  int(1)
}
array(2) {
  [0]=>
  int(1)
  [1]=>
  array(1) {
    [0]=>
    int(2)
  }
}
NULL
string(1) "b"
"#
    );
}

/// A `JsonSerializable` element encoded under a small depth limit keeps the `{"key":` prefix its
/// encoder had already written: the runtime saved `_json_depth_limit` in the register that held
/// the prefix length, so it persisted (and restored) that many bytes instead -- none at all for a
/// zero limit. Expected output verified against php 8.5.
#[test]
fn test_json_serializable_element_keeps_prefix_under_small_depth_limit() {
    let out = compile_and_run(
        r#"<?php
class Item implements JsonSerializable {
    public function __construct(private array $names) {}
    public function jsonSerialize(): mixed {
        $out = [];
        foreach ($this->names as $name) { $out[] = strtoupper($name) . '-' . str_repeat('x', 2); }
        return $out;
    }
}
$data = ['published_items' => new Item(['a', 'b']), 'count' => 3, 'next' => new Item(['c'])];
echo json_encode($data, 0, 3), "\n";
echo json_encode($data, 0, 512), "\n";
echo json_encode($data), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"{"published_items":["A-xx","B-xx"],"count":3,"next":["C-xx"]}
{"published_items":["A-xx","B-xx"],"count":3,"next":["C-xx"]}
{"published_items":["A-xx","B-xx"],"count":3,"next":["C-xx"]}
"#
    );
}
