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

/// Verifies that `grapheme_strrev()` with no arguments produces the correct arity error.
#[test]
fn test_error_grapheme_strrev_wrong_args() {
    expect_error(
        "<?php grapheme_strrev();",
        "grapheme_strrev() takes exactly 1 argument",
    );
}

/// Verifies that `grapheme_strrev()` rejects statically non-string arguments.
#[test]
fn test_error_grapheme_strrev_non_string_argument() {
    expect_error(
        "<?php grapheme_strrev(123);",
        "grapheme_strrev() argument must be string",
    );
}

expect_builtin_arity_error!(
    test_error_crc32_wrong_args,
    "<?php crc32();",
    "crc32() takes exactly 1 argument"
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

expect_builtin_arity_error!(
    test_error_chop_wrong_args,
    "<?php chop();",
    "chop() takes 1 or 2 arguments"
);

/// Verifies that `substr()` with only one string argument produces the correct arity error.
#[test]
fn test_error_substr_wrong_args() {
    expect_error("<?php substr(\"hi\");", "substr() takes 2 or 3 arguments");
}

/// Verifies that `strpos()` with only one argument produces the correct arity error.
#[test]
fn test_error_strpos_wrong_args() {
    expect_error(
        "<?php strpos(\"hi\");",
        "strpos() takes exactly 2 arguments",
    );
}

/// Verifies that a function returning `int` that returns `strpos()` directly is rejected,
/// because `strpos()` returns `Int|Bool` (false on miss), not `int`. This is a type
/// incompatibility regression test.
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

/// Verifies that `str_replace()` with only two arguments produces the correct arity error.
#[test]
fn test_error_str_replace_wrong_args() {
    expect_error(
        "<?php str_replace(\"a\", \"b\");",
        "str_replace() takes exactly 3 arguments",
    );
}

/// Verifies that `sprintf()` with no arguments produces the correct arity error.
#[test]
fn test_error_sprintf_no_args() {
    expect_error("<?php sprintf();", "sprintf() requires at least 1 argument");
}

/// Verifies that `printf()` with no arguments produces the correct arity error.
#[test]
fn test_error_printf_no_args() {
    expect_error("<?php printf();", "printf() requires at least 1 argument");
}

/// Verifies that `ord()` with no arguments produces the correct arity error.
#[test]
fn test_error_ord_wrong_args() {
    expect_error("<?php ord();", "ord() takes exactly 1 argument");
}

/// Verifies that `explode()` with only one argument produces the correct arity error.
#[test]
fn test_error_explode_wrong_args() {
    expect_error(
        "<?php explode(\",\");",
        "explode() takes exactly 2 arguments",
    );
}

/// Verifies that `str_pad()` with only one argument produces the correct arity error.
#[test]
fn test_error_str_pad_wrong_args() {
    expect_error("<?php str_pad(\"x\");", "str_pad() takes 2 to 4 arguments");
}

/// Verifies that `md5()` with no arguments produces the correct arity error.
/// md5() accepts an optional `$binary` flag, so the message reports 1 or 2 args.
#[test]
fn test_error_md5_wrong_args() {
    expect_error("<?php md5();", "md5() takes 1 or 2 arguments");
}

/// Verifies that `sha1()` with no arguments produces the correct arity error.
/// sha1() accepts an optional `$binary` flag, so the message reports 1 or 2 args.
#[test]
fn test_error_sha1_wrong_args() {
    expect_error("<?php sha1();", "sha1() takes 1 or 2 arguments");
}

/// Verifies that `htmlspecialchars()` with no arguments produces the correct arity error.
#[test]
fn test_error_htmlspecialchars_wrong_args() {
    expect_error(
        "<?php htmlspecialchars();",
        "htmlspecialchars() takes exactly 1 argument",
    );
}

/// Verifies that `urlencode()` with no arguments produces the correct arity error.
#[test]
fn test_error_urlencode_wrong_args() {
    expect_error("<?php urlencode();", "urlencode() takes exactly 1 argument");
}

/// Verifies that `base64_encode()` with no arguments produces the correct arity error.
#[test]
fn test_error_base64_encode_wrong_args() {
    expect_error(
        "<?php base64_encode();",
        "base64_encode() takes exactly 1 argument",
    );
}

/// Verifies that `ctype_alpha()` with no arguments produces the correct arity error.
#[test]
fn test_error_ctype_alpha_wrong_args() {
    expect_error(
        "<?php ctype_alpha();",
        "ctype_alpha() takes exactly 1 argument",
    );
}

/// Verifies that `hash()` with only one argument produces the correct arity error.
/// `hash()` now accepts an optional third `$binary` argument, so the message
/// reports the 2-or-3 arity instead of the legacy fixed-2 wording.
#[test]
fn test_error_hash_wrong_args() {
    expect_error(r#"<?php hash("md5");"#, "hash() takes 2 or 3 arguments");
}

/// Verifies the remaining hash-family builtins reject invalid argument counts.
#[test]
fn test_error_hash_family_wrong_args() {
    for (source, message) in [
        (
            r#"<?php hash_hmac("sha256", "data");"#,
            "hash_hmac() takes 3 or 4 arguments",
        ),
        (
            r#"<?php hash_equals("known");"#,
            "hash_equals() takes exactly 2 arguments",
        ),
        (
            "<?php hash_algos(1);",
            "hash_algos() takes no arguments",
        ),
        (
            "<?php hash_init();",
            "hash_init() flags/HASH_HMAC streaming mode is not supported; use hash_hmac() for HMAC",
        ),
        (
            "<?php hash_update();",
            "hash_update() takes exactly 2 arguments",
        ),
        (
            "<?php hash_final();",
            "hash_final() takes 1 or 2 arguments",
        ),
        (
            "<?php hash_copy();",
            "hash_copy() takes exactly 1 argument",
        ),
    ] {
        expect_error(source, message);
    }
}

/// Verifies that `sscanf()` with only one argument produces the correct arity error.
#[test]
fn test_error_sscanf_wrong_args() {
    expect_error(
        r#"<?php sscanf("hi");"#,
        "sscanf() takes at least 2 arguments",
    );
}

// --- v0.5: I/O function errors ---

/// Verifies that `ptr_set()` rejects a string value, since ptr_set only accepts
/// int, bool, null, or pointer. This is an I/O function error regression test.
#[test]
fn test_error_ptr_set_requires_word_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_set($p, \"hello\");",
        "ptr_set() value must be int, bool, null, or pointer",
    );
}

/// Verifies the invalid-call diagnostic for error long2ip wrong args.
#[test]
fn test_error_long2ip_wrong_args() {
    expect_error("<?php long2ip();", "long2ip() takes exactly 1 argument");
}

/// Verifies the invalid-call diagnostic for error ip2long wrong args.
#[test]
fn test_error_ip2long_wrong_args() {
    expect_error("<?php ip2long();", "ip2long() takes exactly 1 argument");
}

/// Verifies the invalid-call diagnostic for error inet ntop wrong args.
#[test]
fn test_error_inet_ntop_wrong_args() {
    expect_error("<?php inet_ntop();", "inet_ntop() takes exactly 1 argument");
}

/// Verifies the invalid-call diagnostic for error inet pton wrong args.
#[test]
fn test_error_inet_pton_wrong_args() {
    expect_error("<?php inet_pton();", "inet_pton() takes exactly 1 argument");
}

/// Verifies the invalid-call diagnostic for error gzcompress wrong args.
#[test]
fn test_error_gzcompress_wrong_args() {
    expect_error("<?php gzcompress();", "gzcompress() expects 1 or 2 arguments");
}

/// Verifies the invalid-call diagnostic for error gzuncompress wrong args.
#[test]
fn test_error_gzuncompress_wrong_args() {
    expect_error("<?php gzuncompress();", "gzuncompress() expects 1 or 2 arguments");
}

/// Verifies the invalid-call diagnostic for error gzdeflate wrong args.
#[test]
fn test_error_gzdeflate_wrong_args() {
    expect_error("<?php gzdeflate();", "gzdeflate() expects 1 or 2 arguments");
}

/// Verifies the invalid-call diagnostic for error gzinflate wrong args.
#[test]
fn test_error_gzinflate_wrong_args() {
    expect_error("<?php gzinflate();", "gzinflate() expects 1 or 2 arguments");
}

/// Verifies the invalid-call diagnostic for error vsprintf wrong args.
#[test]
fn test_error_vsprintf_wrong_args() {
    expect_error(
        "<?php vsprintf(\"%d\");",
        "vsprintf() takes exactly 2 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error vprintf wrong args.
#[test]
fn test_error_vprintf_wrong_args() {
    expect_error(
        "<?php vprintf(\"%d\", [1], 3);",
        "vprintf() takes exactly 2 arguments",
    );
}
