use super::*;

// __rt_json_depth_enter / __rt_json_depth_exit: each container encoder
// increments _json_active_depth at entry, compares with the user-supplied
// $depth limit, and routes through __rt_json_throw_error when exceeded.

#[test]
fn test_json_encode_default_depth_allows_deep_nesting() {
    // PHP's default $depth is 512 — four levels fit comfortably.
    let out = compile_and_run("<?php echo json_encode([[[[1]]]]);");
    assert_eq!(out, "[[[[1]]]]");
}

#[test]
fn test_json_encode_depth_one_allows_flat_array() {
    let out = compile_and_run("<?php echo json_encode([1, 2, 3], 0, 1);");
    assert_eq!(out, "[1,2,3]");
}

#[test]
fn test_json_encode_depth_two_allows_one_nested_level() {
    let out = compile_and_run("<?php echo json_encode([[1]], 0, 2);");
    assert_eq!(out, "[[1]]");
}

#[test]
fn test_json_encode_depth_two_rejects_two_nested_levels_in_error_state() {
    let out = compile_and_run(
        "<?php json_encode([[[1]]], 0, 2); echo json_last_error();",
    );
    assert_eq!(out, "1");
}

#[test]
fn test_json_encode_depth_two_rejects_two_nested_levels_message() {
    let out = compile_and_run(
        "<?php json_encode([[[1]]], 0, 2); echo json_last_error_msg();",
    );
    assert_eq!(out, "Maximum stack depth exceeded");
}

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

#[test]
fn test_json_encode_depth_zero_rejects_top_level_array() {
    let out = compile_and_run(
        "<?php json_encode([1], 0, 0); echo json_last_error();",
    );
    assert_eq!(out, "1");
}

#[test]
fn test_json_encode_depth_zero_does_not_affect_scalars() {
    // Scalars never enter a container encoder, so the depth check is a
    // no-op for them even with depth=0.
    let out = compile_and_run("<?php echo json_encode(42, 0, 0);");
    assert_eq!(out, "42");
}

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
