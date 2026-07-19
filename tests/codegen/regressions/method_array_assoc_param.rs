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

/// `isset()` dispatches on the runtime tag of each Mixed-valued associative-array entry before it
/// attempts to unbox the entry payload.
///
/// PDO SQLSRV exposed this through repeated `prepare()` option dispatch: the nested option helper
/// passed raw integer `42` to `__rt_mixed_unbox` as though it were a boxed pointer. The null entry
/// covers the concrete tag-8 path, while the final reads prove the non-null entry remains intact.
#[test]
fn test_nested_method_array_param_isset_handles_concrete_mixed_entries() {
    let out = compile_and_run(
        r#"<?php
class Options {
    public function prepare(array $options): string {
        $missing = $this->configure($options, 1000);
        $null = $this->configure($options, 1004);
        $present = $this->configure($options, 1003);
        return $missing . ":" . $null . ":" . $present . ":" . $options[1003];
    }

    private function configure(array $options, int $option): int {
        if (!isset($options[$option])) {
            return -1;
        }
        return $this->asInt($options[$option]);
    }

    private function asInt(mixed $value): int {
        return (int) $value;
    }
}

$options = [10 => 1, 1003 => 42, 1004 => null, 1006 => true, 1007 => true];
echo (new Options())->prepare($options), ":", $options[1003];
$boxed = $options;
foreach ($boxed as &$entry) {
}
unset($entry);
echo "|", (new Options())->prepare($boxed);
"#,
    );
    assert_eq!(out, "-1:-1:42:42:42|-1:-1:42:42");
}
