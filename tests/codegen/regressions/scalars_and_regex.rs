use super::*;

#[test]
fn test_null_byte_in_string() {
    let out = compile_and_run(r#"<?php echo strlen("ab\0cd");"#);
    assert_eq!(out, "5");
}

// -- Issue #26: empty string should be falsy --

#[test]
fn test_not_empty_string_is_true() {
    let out = compile_and_run(r#"<?php echo !!"";"#);
    assert_eq!(out, "");
}

#[test]
fn test_not_nonempty_string_is_false() {
    let out = compile_and_run(r#"<?php echo !!"hello";"#);
    assert_eq!(out, "1");
}

// -- Issue #27: is_numeric() should work for numeric strings --

#[test]
fn test_is_numeric_string_digits() {
    let out = compile_and_run(r#"<?php if (is_numeric("42")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_float() {
    let out =
        compile_and_run(r#"<?php if (is_numeric("3.14")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_negative() {
    let out = compile_and_run(r#"<?php if (is_numeric("-5")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_not_numeric() {
    let out =
        compile_and_run(r#"<?php if (is_numeric("abc")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "no");
}

// -- Issue #29: function_exists() should recognize builtins --

#[test]
fn test_preg_split_backslash_s() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/\s+/", "hello  world");
echo $parts[1];
"#,
    );
    assert_eq!(out, "world");
}

#[test]
fn test_preg_split_backslash_d() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/\d+/", "abc123def456ghi");
echo count($parts) . "|" . $parts[0] . "|" . $parts[1] . "|" . $parts[2];
"#,
    );
    assert_eq!(out, "3|abc|def|ghi");
}

#[test]
fn test_preg_match_backslash_s() {
    let out = compile_and_run(r#"<?php echo preg_match("/\s/", "hello world");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_backslash_d() {
    let out = compile_and_run(r#"<?php echo preg_match("/\d+/", "abc123");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_backslash_w() {
    let out = compile_and_run(r#"<?php echo preg_match("/^\w+$/", "hello_world");"#);
    assert_eq!(out, "1");
}

// --- Issue #14: hex integer literals ---

#[test]
fn test_hex_literal_0xff() {
    let out = compile_and_run("<?php echo 0xFF;");
    assert_eq!(out, "255");
}

#[test]
fn test_hex_literal_0x1a() {
    let out = compile_and_run("<?php echo 0x1A;");
    assert_eq!(out, "26");
}

#[test]
fn test_hex_literal_0x0() {
    let out = compile_and_run("<?php echo 0x0;");
    assert_eq!(out, "0");
}

#[test]
fn test_hex_literal_uppercase_prefix() {
    let out = compile_and_run("<?php echo 0XFF;");
    assert_eq!(out, "255");
}

#[test]
fn test_hex_literal_arithmetic() {
    let out = compile_and_run("<?php echo 0xFF + 1;");
    assert_eq!(out, "256");
}

// --- Issue #23: modulo by zero ---

#[test]
fn test_modulo_normal() {
    let out = compile_and_run("<?php echo 5 % 1;");
    assert_eq!(out, "0");
}

#[test]
fn test_modulo_by_zero() {
    let out = compile_and_run("<?php echo 5 % 0;");
    assert_eq!(out, "0");
}

#[test]
fn test_modulo_normal_remainder() {
    let out = compile_and_run("<?php echo 7 % 3;");
    assert_eq!(out, "1");
}

// --- Issue #24: negative array index ---

#[test]
fn test_fmod_negative_dividend() {
    let out = compile_and_run("<?php echo fmod(-10, 3);");
    assert_eq!(out, "-1");
}

#[test]
fn test_float_modulo_negative() {
    let out = compile_and_run("<?php echo -10.0 % 3;");
    assert_eq!(out, "-1");
}

// --- Bug fix: string "0" is falsy ---

#[test]
fn test_string_zero_falsy_if() {
    let out = compile_and_run(
        r#"<?php
if ("0") { echo "bad"; } else { echo "good"; }
"#,
    );
    assert_eq!(out, "good");
}

#[test]
fn test_string_zero_falsy_ternary() {
    let out = compile_and_run(r#"<?php echo "0" ? "truthy" : "falsy";"#);
    assert_eq!(out, "falsy");
}

#[test]
fn test_string_zero_falsy_not() {
    let out = compile_and_run(r#"<?php echo !"0" ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_string_nonempty_truthy() {
    let out = compile_and_run(r#"<?php echo "hello" ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_string_empty_falsy() {
    let out = compile_and_run(r#"<?php echo "" ? "yes" : "no";"#);
    assert_eq!(out, "no");
}

// --- Bug fix: compound assignment in for-loop update ---
