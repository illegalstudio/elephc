//! Purpose:
//! Regression tests for issue #406: an `array`-typed method parameter (instance or
//! static) must preserve the associative shape known at the call site, exactly like
//! a free-function `array` parameter does.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Before the fix a declared `array` method parameter was treated as an
//!   integer-indexed list: string-key access was rejected with "Array index must be
//!   integer" and `json_encode` emitted a JSON list with garbage values instead of a
//!   JSON object. Free functions already specialized the generic `array` hint from the
//!   call-site argument; methods did not.

use crate::support::*;

/// A static method with a declared `array` parameter, called with an associative
/// literal, accepts string-key access on that parameter (previously rejected at
/// compile time with "Array index must be integer").
#[test]
fn test_static_method_array_param_string_key_access() {
    let out = compile_and_run(
        r#"<?php
class W {
    public static function first(array $d): string {
        return $d['a'];
    }
}
echo W::first(['a' => 'hello']);
"#,
    );
    assert_eq!(out, "hello");
}

/// An instance method with a declared `array` parameter, called with an associative
/// literal, accepts string-key access on that parameter.
#[test]
fn test_instance_method_array_param_string_key_access() {
    let out = compile_and_run(
        r#"<?php
class W {
    public function first(array $d): string {
        return $d['a'];
    }
}
$w = new W();
echo $w->first(['a' => 'hello']);
"#,
    );
    assert_eq!(out, "hello");
}

/// `json_encode` of a static method's `array` parameter emits a JSON object (the
/// associative shape is preserved), not a JSON list with garbage values.
#[test]
fn test_static_method_array_param_json_encode_object() {
    let out = compile_and_run(
        r#"<?php
class W {
    public static function enc(array $d): string {
        return json_encode($d);
    }
}
echo W::enc(['a' => 1, 'b' => 2]);
"#,
    );
    assert_eq!(out, r#"{"a":1,"b":2}"#);
}

/// `json_encode` of an instance method's `array` parameter emits a JSON object.
#[test]
fn test_instance_method_array_param_json_encode_object() {
    let out = compile_and_run(
        r#"<?php
class W {
    public function enc(array $d): string {
        return json_encode($d);
    }
}
$w = new W();
echo $w->enc(['a' => 1, 'b' => 2]);
"#,
    );
    assert_eq!(out, r#"{"a":1,"b":2}"#);
}

/// A method `array` parameter still works as an integer-indexed list when called with
/// a list literal: the call-site specialization must not break the plain list case.
#[test]
fn test_method_array_param_list_still_works() {
    let out = compile_and_run(
        r#"<?php
class W {
    public static function at(array $d): string {
        return $d[0] . $d[1];
    }
}
echo W::at(['x', 'y']);
"#,
    );
    assert_eq!(out, "xy");
}
