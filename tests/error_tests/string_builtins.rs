//! Purpose:
//! Integration or regression tests for diagnostic coverage of string builtins, including substr wrong args, strpos wrong args, and strpos false return rejects integer return type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

expect_builtin_arity_error!(
    test_error_substr_replace_wrong_args,
    "<?php substr_replace(\"abc\", \"x\");",
    "substr_replace() takes 3 or 4 arguments"
);

expect_builtin_arity_error!(
    test_error_rawurlencode_wrong_args,
    "<?php rawurlencode();",
    "rawurlencode() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_base64_decode_wrong_args,
    "<?php base64_decode();",
    "base64_decode() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_ctype_digit_wrong_args,
    "<?php ctype_digit();",
    "ctype_digit() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_ctype_alnum_wrong_args,
    "<?php ctype_alnum();",
    "ctype_alnum() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_ctype_space_wrong_args,
    "<?php ctype_space();",
    "ctype_space() takes exactly 1 argument"
);

#[test]
fn test_error_substr_wrong_args() {
    expect_error("<?php substr(\"hi\");", "substr() takes 2 or 3 arguments");
}

#[test]
fn test_error_strpos_wrong_args() {
    expect_error(
        "<?php strpos(\"hi\");",
        "strpos() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_strpos_false_return_rejects_int_return_type() {
    expect_error(
        r#"<?php
function pos(): int {
    return strpos("abc", "z");
}
"#,
        "Function 'pos' return type expects Int, got Union([Int, Bool])",
    );
}

#[test]
fn test_error_str_replace_wrong_args() {
    expect_error(
        "<?php str_replace(\"a\", \"b\");",
        "str_replace() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_sprintf_no_args() {
    expect_error("<?php sprintf();", "sprintf() requires at least 1 argument");
}

#[test]
fn test_error_printf_no_args() {
    expect_error("<?php printf();", "printf() requires at least 1 argument");
}

#[test]
fn test_error_ord_wrong_args() {
    expect_error("<?php ord();", "ord() takes exactly 1 argument");
}

#[test]
fn test_error_explode_wrong_args() {
    expect_error(
        "<?php explode(\",\");",
        "explode() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_str_pad_wrong_args() {
    expect_error("<?php str_pad(\"x\");", "str_pad() takes 2 to 4 arguments");
}

#[test]
fn test_error_md5_wrong_args() {
    expect_error("<?php md5();", "md5() takes exactly 1 argument");
}

#[test]
fn test_error_sha1_wrong_args() {
    expect_error("<?php sha1();", "sha1() takes exactly 1 argument");
}

#[test]
fn test_error_htmlspecialchars_wrong_args() {
    expect_error(
        "<?php htmlspecialchars();",
        "htmlspecialchars() takes exactly 1 argument",
    );
}

#[test]
fn test_error_urlencode_wrong_args() {
    expect_error("<?php urlencode();", "urlencode() takes exactly 1 argument");
}

#[test]
fn test_error_base64_encode_wrong_args() {
    expect_error(
        "<?php base64_encode();",
        "base64_encode() takes exactly 1 argument",
    );
}

#[test]
fn test_error_ctype_alpha_wrong_args() {
    expect_error(
        "<?php ctype_alpha();",
        "ctype_alpha() takes exactly 1 argument",
    );
}

#[test]
fn test_error_hash_wrong_args() {
    expect_error(r#"<?php hash("md5");"#, "hash() takes exactly 2 arguments");
}

#[test]
fn test_error_sscanf_wrong_args() {
    expect_error(
        r#"<?php sscanf("hi");"#,
        "sscanf() takes at least 2 arguments",
    );
}

// --- v0.5: I/O function errors ---

#[test]
fn test_error_ptr_set_requires_word_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_set($p, \"hello\");",
        "ptr_set() value must be int, bool, null, or pointer",
    );
}
