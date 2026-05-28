//! Purpose:
//! Provides JSON encode depth-limit tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Container encoders must enter and exit depth state consistently across siblings.

use super::*;

// __rt_json_depth_enter / __rt_json_depth_exit: each container encoder
// increments _json_active_depth at entry, compares with the user-supplied
// $depth limit, and routes through __rt_json_throw_error when exceeded.

/// Verifies PHP's default depth (512) allows four levels of nested arrays.
#[test]
fn test_json_encode_default_depth_allows_deep_nesting() {
    // PHP's default $depth is 512 — four levels fit comfortably.
    let out = compile_and_run("<?php echo json_encode([[[[1]]]]);");
    assert_eq!(out, "[[[[1]]]]");
}

/// Verifies depth=1 accepts a flat array with one container level.
#[test]
fn test_json_encode_depth_one_allows_flat_array() {
    let out = compile_and_run("<?php echo json_encode([1, 2, 3], 0, 1);");
    assert_eq!(out, "[1,2,3]");
}

/// Verifies depth=2 accepts exactly one level of nesting ([[1]]); two levels fails.
#[test]
fn test_json_encode_depth_two_allows_one_nested_level() {
    let out = compile_and_run("<?php echo json_encode([[1]], 0, 2);");
    assert_eq!(out, "[[1]]");
}

/// Verifies two nested levels ([[[1]]]) with depth=2 sets JSON_ERROR_DEPTH (1).
#[test]
fn test_json_encode_depth_two_rejects_two_nested_levels_in_error_state() {
    let out = compile_and_run(
        "<?php json_encode([[[1]]], 0, 2); echo json_last_error();",
    );
    assert_eq!(out, "1");
}

/// Verifies depth overflow sets the error message to "Maximum stack depth exceeded".
#[test]
fn test_json_encode_depth_two_rejects_two_nested_levels_message() {
    let out = compile_and_run(
        "<?php json_encode([[[1]]], 0, 2); echo json_last_error_msg();",
    );
    assert_eq!(out, "Maximum stack depth exceeded");
}

/// Verifies depth overflow with JSON_THROW_ON_ERROR raises JsonException with
/// "Maximum stack depth exceeded" message.
#[test]
fn test_json_encode_depth_throw_on_error_raises_jsonexception() {
    let out = compile_and_run(
        r#"<?php
try {
    json_encode([[[1]]], JSON_THROW_ON_ERROR, 2);
    echo "no throw";
} catch (JsonException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "Maximum stack depth exceeded");
}

/// Verifies depth=0 rejects a top-level array (which is one container level).
#[test]
fn test_json_encode_depth_zero_rejects_top_level_array() {
    let out = compile_and_run(
        "<?php json_encode([1], 0, 0); echo json_last_error();",
    );
    assert_eq!(out, "1");
}

/// Verifies depth=0 does not affect scalar encoding (scalars never enter a container encoder).
#[test]
fn test_json_encode_depth_zero_does_not_affect_scalars() {
    // Scalars never enter a container encoder, so the depth check is a
    // no-op for them even with depth=0.
    let out = compile_and_run("<?php echo json_encode(42, 0, 0);");
    assert_eq!(out, "42");
}

/// Verifies sibling json_encode calls do not bleed depth state; each call resets depth to 0.
#[test]
fn test_json_encode_depth_resets_between_calls() {
    // Sibling calls share _json_active_depth via the encoder; entering a
    // fresh json_encode resets the depth state to 0.
    let out = compile_and_run(
        r#"<?php
json_encode([[1]], 0, 1);
echo json_encode([[1, 2]], 0, 2);
"#,
    );
    assert_eq!(out, "[[1,2]]");
}

/// Verifies associative arrays count toward depth; nesting exceeds limit with depth=2.
#[test]
fn test_json_encode_depth_via_assoc_array() {
    // Associative arrays count toward depth too.
    let out = compile_and_run(
        r#"<?php
try {
    json_encode(["a" => ["b" => ["c" => 1]]], JSON_THROW_ON_ERROR, 2);
    echo "no throw";
} catch (JsonException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "Maximum stack depth exceeded");
}

/// Verifies object encoding counts toward depth; with depth=1, encoding an object whose
/// property is itself an object hits the limit.
#[test]
fn test_json_encode_depth_via_object() {
    // Object encoding counts toward depth too. With depth=1, encoding an
    // object whose property is itself an object should hit the limit.
    let out = compile_and_run(
        r#"<?php
class Inner { public int $x = 1; }
class Outer { public Inner $inner; public function __construct() { $this->inner = new Inner(); } }
try {
    json_encode(new Outer(), JSON_THROW_ON_ERROR, 1);
    echo "no throw";
} catch (JsonException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "Maximum stack depth exceeded");
}
