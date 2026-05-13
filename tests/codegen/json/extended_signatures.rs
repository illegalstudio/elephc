use super::*;

// Phase 2 only widens the *signatures* — the runtimes don't yet observe the
// optional flag/depth parameters. These tests pin the signature surface so a
// future regression in arg parsing fails loudly.

#[test]
fn test_json_encode_with_flags_argument_compiles() {
    // JSON_PRETTY_PRINT is now observed by the encoder.
    let out = compile_and_run(
        "<?php echo json_encode([1, 2, 3], JSON_PRETTY_PRINT);",
    );
    assert_eq!(out, "[\n    1,\n    2,\n    3\n]");
}

#[test]
fn test_json_encode_with_flags_and_depth_arguments_compiles() {
    let out = compile_and_run(
        "<?php echo json_encode([1, 2], JSON_UNESCAPED_SLASHES, 64);",
    );
    assert_eq!(out, "[1,2]");
}

#[test]
fn test_json_decode_with_associative_argument_compiles() {
    let out = compile_and_run(
        r#"<?php echo json_decode("\"hi\"", true);"#,
    );
    assert_eq!(out, "hi");
}

#[test]
fn test_json_decode_with_all_optional_arguments_compiles() {
    let out = compile_and_run(
        r#"<?php echo json_decode("\"x\"", false, 256, 0);"#,
    );
    assert_eq!(out, "x");
}

#[test]
fn test_json_validate_first_class_callable_compiles() {
    let out = compile_and_run(
        r#"<?php $f = json_validate(...); echo ($f("[1]") ? "ok" : "no");"#,
    );
    assert_eq!(out, "ok");
}
