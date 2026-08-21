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
    test_error_parse_url_wrong_args,
    "<?php parse_url();",
    "parse_url() takes 1 or 2 arguments"
);

/// Verifies that `parse_url()` rejects a statically non-integer component selector.
#[test]
fn test_error_parse_url_component_type() {
    expect_error(
        "<?php parse_url('https://example.com', 'host');",
        "parse_url() component must be int",
    );
}

expect_builtin_arity_error!(
    test_error_base64_decode_wrong_args,
    "<?php base64_decode();",
    "base64_decode() takes 1 or 2 arguments"
);

expect_builtin_arity_error!(
    test_error_base64_decode_too_many_args,
    "<?php base64_decode(\"SGk=\", true, 1);",
    "base64_decode() takes 1 or 2 arguments"
);

expect_builtin_arity_error!(
    test_error_mb_ereg_match_wrong_args,
    "<?php mb_ereg_match('ab');",
    "mb_ereg_match() takes 2 or 3 arguments"
);

expect_builtin_arity_error!(
    test_error_mb_strlen_wrong_args,
    "<?php mb_strlen();",
    "mb_strlen() takes 1 or 2 arguments"
);

/// Verifies the OpenSSL IV-length helper rejects a missing cipher name.
#[test]
fn test_error_openssl_cipher_iv_length_wrong_args() {
    expect_error(
        "<?php openssl_cipher_iv_length();",
        "openssl_cipher_iv_length() takes exactly 1 argument",
    );
}

/// Verifies OpenSSL decryption rejects calls missing the required passphrase.
#[test]
fn test_error_openssl_decrypt_wrong_args() {
    expect_error(
        "<?php openssl_decrypt('ciphertext', 'aes-128-cbc');",
        "openssl_decrypt() takes 3 to 7 arguments",
    );
}

/// Verifies the optional OpenSSL cipher-list flag cannot be supplied twice.
#[test]
fn test_error_openssl_get_cipher_methods_wrong_args() {
    expect_error(
        "<?php openssl_get_cipher_methods(false, true);",
        "openssl_get_cipher_methods() takes at most 1 argument",
    );
}

/// Verifies that `mb_strlen()` rejects a statically non-string value argument.
#[test]
fn test_error_mb_strlen_string_type() {
    expect_error(
        "<?php mb_strlen([1, 2]);",
        "mb_strlen() string argument must be string",
    );
}

/// Verifies that `mb_strlen()` accepts only string or null encoding arguments.
#[test]
fn test_error_mb_strlen_encoding_type() {
    expect_error(
        "<?php mb_strlen('abc', 123);",
        "mb_strlen() encoding argument must be string or null",
    );
}

/// Verifies that `mb_ereg_match()` rejects a non-string pattern.
#[test]
fn test_error_mb_ereg_match_pattern_type() {
    expect_error(
        "<?php mb_ereg_match(123, 'abc');",
        "mb_ereg_match() pattern argument must be string",
    );
}

/// Verifies that `mb_ereg_match()` rejects non-string, non-null options.
#[test]
fn test_error_mb_ereg_match_options_type() {
    expect_error(
        "<?php mb_ereg_match('ab', 'abc', 1);",
        "mb_ereg_match() options argument must be string or null",
    );
}

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
///
/// The optional third parameter (`$offset`) is now accepted, so the derived phrasing is a
/// range rather than an exact count.
#[test]
fn test_error_strpos_wrong_args() {
    expect_error(
        "<?php strpos(\"hi\");",
        "strpos() takes 2 or 3 arguments",
    );
}

/// Verifies that `stripos()` with only one argument produces the correct arity error.
///
/// The optional third parameter (`$offset`) is accepted, so the derived phrasing is a range
/// rather than an exact count, exactly like `strpos()`.
#[test]
fn test_error_stripos_wrong_args() {
    expect_error(
        "<?php stripos(\"hi\");",
        "stripos() takes 2 or 3 arguments",
    );
}

/// Verifies that `strripos()` with four arguments produces the correct arity error.
#[test]
fn test_error_strripos_too_many_args() {
    expect_error(
        "<?php strripos(\"hi\", \"h\", 0, 1);",
        "strripos() takes 2 or 3 arguments",
    );
}

/// Verifies that `quoted_printable_encode()` with no arguments produces the correct arity error.
#[test]
fn test_error_quoted_printable_encode_wrong_args() {
    expect_error(
        "<?php quoted_printable_encode();",
        "quoted_printable_encode() takes exactly 1 argument",
    );
}

/// Verifies that `quoted_printable_encode()` with two arguments produces the correct arity error.
#[test]
fn test_error_quoted_printable_encode_too_many_args() {
    expect_error(
        "<?php quoted_printable_encode(\"a\", \"b\");",
        "quoted_printable_encode() takes exactly 1 argument",
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
        "Function 'pos' return type expects Int, got Union([Int, False])",
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
    expect_error("<?php sprintf();", "sprintf() takes at least 1 argument");
}

/// Verifies that `printf()` with no arguments produces the correct arity error.
#[test]
fn test_error_printf_no_args() {
    expect_error("<?php printf();", "printf() takes at least 1 argument");
}

/// Verifies that `ord()` with no arguments produces the correct arity error.
#[test]
fn test_error_ord_wrong_args() {
    expect_error("<?php ord();", "ord() takes exactly 1 argument");
}

/// Verifies that a pure-data registry builtin (`ord`) infers argument types so that
/// an undefined variable passed as an argument produces the correct diagnostic.
///
/// This is a regression test for Fix B: before the fix, the registry-first dispatch
/// branch skipped `infer_type` for builtins with no check hook, so undefined-variable
/// errors were silently dropped.
#[test]
fn test_error_ord_undefined_variable_arg() {
    expect_error(
        "<?php ord($undeclared);",
        "Undefined variable: $undeclared",
    );
}

/// Verifies that `explode()` with only one argument produces the correct arity error.
///
/// The reported range covers the optional `$limit` third parameter.
#[test]
fn test_error_explode_wrong_args() {
    expect_error(
        "<?php explode(\",\");",
        "explode() takes 2 or 3 arguments",
    );
}

/// Verifies that `explode()` rejects a fourth argument now that `$limit` is accepted.
#[test]
fn test_error_explode_too_many_args() {
    expect_error(
        "<?php explode(\",\", \"a,b\", 1, 2);",
        "explode() takes 2 or 3 arguments",
    );
}

/// Verifies that `explode()` accepts the optional `$limit`, positionally and by name.
#[test]
fn test_explode_limit_argument_type_checks() {
    assert!(
        check_source("<?php $a = explode(\",\", \"a,b\", -1);").is_ok(),
        "explode() must accept a negative positional $limit",
    );
    assert!(
        check_source("<?php $a = explode(\",\", \"a,b\", limit: 2);").is_ok(),
        "explode() must accept $limit as a named argument",
    );
}

/// Verifies that `str_pad()`'s padding-mode constants are predefined like PHP's.
#[test]
fn test_str_pad_mode_constants_are_predefined() {
    assert!(
        check_source(
            "<?php $a = str_pad(\"x\", 4, \"-\", STR_PAD_LEFT) . str_pad(\"x\", 4, \"-\", STR_PAD_RIGHT) . str_pad(\"x\", 4, \"-\", STR_PAD_BOTH);"
        )
        .is_ok(),
        "STR_PAD_LEFT/STR_PAD_RIGHT/STR_PAD_BOTH must resolve as predefined constants",
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
/// htmlspecialchars() accepts optional `$flags` and `$encoding` arguments, so the message
/// reports 1 to 3 args.
#[test]
fn test_error_htmlspecialchars_wrong_args() {
    expect_error(
        "<?php htmlspecialchars();",
        "htmlspecialchars() takes 1 to 3 arguments",
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
        // CHANGED CELLS. `hash_init`/`hash_update`/`hash_final`/`hash_copy` are no
        // longer builtins: they are elephc-PHP wrappers injected by
        // `elephc::hash_prelude` so that `hash_init()` can return a real `HashContext`
        // OBJECT (PHP 8 parity). Arity is therefore diagnosed by the ordinary
        // user-function check, which phrases it differently. What matters is preserved:
        // every one of these is still rejected AT COMPILE TIME with a message naming the
        // function and the expected count.
        //
        // The `hash_init` message lost its bespoke "use hash_hmac() for HMAC" hint,
        // which the old builtin carried via `arity_error`. HMAC STREAMING IS STILL
        // UNSUPPORTED and still rejected — the wrapper deliberately keeps a
        // one-parameter signature so `hash_init($algo, HASH_HMAC, $key)` stays a
        // compile-time error rather than being silently accepted. `docs/php/strings.md`
        // carries the pointer to `hash_hmac()` that the diagnostic no longer does.
        (
            "<?php hash_init();",
            "Function 'hash_init' expects 1 arguments, got 0",
        ),
        (
            r#"<?php hash_init("sha256", 1, "key");"#,
            "Function 'hash_init' expects 1 arguments, got 3",
        ),
        (
            "<?php hash_update();",
            "Function 'hash_update' expects 2 arguments, got 0",
        ),
        (
            "<?php hash_final();",
            "Function 'hash_final' expects 1 to 2 arguments, got 0",
        ),
        (
            "<?php hash_copy();",
            "Function 'hash_copy' expects 1 arguments, got 0",
        ),
    ] {
        expect_error(source, message);
    }
}

/// Verifies `HashContext` CANNOT BE CONSTRUCTED DIRECTLY, matching PHP's private
/// constructor.
///
/// Reference PHP 8.5.6 rejects `new HashContext()` at RUNTIME with
/// `Error: Call to private HashContext::__construct() from global scope`. elephc's
/// prelude gives the class the same private constructor, and the checker rejects it at
/// COMPILE TIME instead — stricter than PHP, never more permissive, and there is no
/// legal program this refuses that PHP would have run. `hash_init()` remains the only
/// way to obtain a context on either side.
#[test]
fn test_error_hash_context_cannot_be_constructed_directly() {
    expect_error(
        "<?php $c = new HashContext();",
        "Cannot access private constructor: HashContext::__construct",
    );
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
    expect_error("<?php gzcompress();", "gzcompress() takes 1 or 2 arguments");
}

/// Verifies the invalid-call diagnostic for error gzuncompress wrong args.
#[test]
fn test_error_gzuncompress_wrong_args() {
    expect_error("<?php gzuncompress();", "gzuncompress() takes 1 or 2 arguments");
}

/// Verifies the invalid-call diagnostic for error gzdeflate wrong args.
#[test]
fn test_error_gzdeflate_wrong_args() {
    expect_error("<?php gzdeflate();", "gzdeflate() takes 1 or 2 arguments");
}

/// Verifies the invalid-call diagnostic for error gzinflate wrong args.
#[test]
fn test_error_gzinflate_wrong_args() {
    expect_error("<?php gzinflate();", "gzinflate() takes 1 or 2 arguments");
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

/// Verifies that `join()` with no arguments produces the correct arity error.
/// `join()` mirrors PHP's `implode()` signature, whose `$array` parameter is optional,
/// so the enforced contract is one or two arguments.
#[test]
fn test_error_join_wrong_args() {
    expect_error("<?php join();", "join() takes 1 or 2 arguments");
}

/// Verifies that `join()` with three arguments produces the correct arity error.
#[test]
fn test_error_join_too_many_args() {
    expect_error("<?php join(\"a\", [\"b\"], \"c\");", "join() takes 1 or 2 arguments");
}

/// Verifies that `substr_count()` with a single argument produces the correct arity error.
#[test]
fn test_error_substr_count_wrong_args() {
    expect_error(
        "<?php substr_count(\"abc\");",
        "substr_count() takes 2 to 4 arguments",
    );
}

/// Verifies that `substr_count()` with five arguments produces the correct arity error.
#[test]
fn test_error_substr_count_too_many_args() {
    expect_error(
        "<?php substr_count(\"abc\", \"b\", 0, 1, 2);",
        "substr_count() takes 2 to 4 arguments",
    );
}

/// Verifies that `strncmp()` with two arguments produces the correct arity error.
#[test]
fn test_error_strncmp_wrong_args() {
    expect_error(
        "<?php strncmp(\"a\", \"b\");",
        "strncmp() takes exactly 3 arguments",
    );
}

/// Verifies that `strncasecmp()` with two arguments produces the correct arity error.
#[test]
fn test_error_strncasecmp_wrong_args() {
    expect_error(
        "<?php strncasecmp(\"a\", \"b\");",
        "strncasecmp() takes exactly 3 arguments",
    );
}

/// Verifies `openssl_encrypt()` rejects a non-variable GCM tag output argument.
#[test]
fn test_error_openssl_encrypt_tag_must_be_variable() {
    expect_error(
        r#"<?php openssl_encrypt("data", "aes-256-gcm", "key", 1, "iv", "tag");"#,
        "openssl_encrypt(): Argument #6 ($tag) could not be passed by reference",
    );
}

expect_builtin_arity_error!(
    test_error_iconv_wrong_args,
    "<?php iconv('UTF-8');",
    "iconv() takes exactly 3 arguments"
);

expect_builtin_arity_error!(
    test_error_iconv_strlen_wrong_args,
    "<?php iconv_strlen();",
    "iconv_strlen() takes 1 or 2 arguments"
);

expect_builtin_arity_error!(
    test_error_iconv_substr_wrong_args,
    "<?php iconv_substr('abc');",
    "iconv_substr() takes 2 to 4 arguments"
);

expect_builtin_arity_error!(
    test_error_iconv_strpos_wrong_args,
    "<?php iconv_strpos('abc');",
    "iconv_strpos() takes 2 to 4 arguments"
);

expect_builtin_arity_error!(
    test_error_iconv_strrpos_wrong_args,
    "<?php iconv_strrpos('abc');",
    "iconv_strrpos() takes 2 or 3 arguments"
);

expect_builtin_arity_error!(
    test_error_iconv_mime_encode_wrong_args,
    "<?php iconv_mime_encode('Subject');",
    "iconv_mime_encode() takes 2 or 3 arguments"
);

expect_builtin_arity_error!(
    test_error_iconv_mime_decode_wrong_args,
    "<?php iconv_mime_decode();",
    "iconv_mime_decode() takes 1 to 3 arguments"
);

expect_builtin_arity_error!(
    test_error_iconv_mime_decode_headers_wrong_args,
    "<?php iconv_mime_decode_headers();",
    "iconv_mime_decode_headers() takes 1 to 3 arguments"
);

expect_builtin_arity_error!(
    test_error_iconv_get_encoding_wrong_args,
    "<?php iconv_get_encoding('all', 'extra');",
    "iconv_get_encoding() takes at most 1 argument"
);

expect_builtin_arity_error!(
    test_error_iconv_set_encoding_wrong_args,
    "<?php iconv_set_encoding('internal_encoding');",
    "iconv_set_encoding() takes exactly 2 arguments"
);

/// Verifies `iconv()` rejects a statically non-string charset argument.
#[test]
fn test_error_iconv_charset_type() {
    expect_error(
        "<?php iconv([1, 2], 'UTF-8', 'x');",
        "iconv() from_encoding argument must be string",
    );
}

/// Verifies `iconv_strlen()` rejects a statically non-string subject.
#[test]
fn test_error_iconv_strlen_string_type() {
    expect_error(
        "<?php iconv_strlen([1, 2]);",
        "iconv_strlen() string argument must be string",
    );
}

/// Verifies the nullable `$encoding` parameter still rejects a container argument.
#[test]
fn test_error_iconv_strlen_encoding_type() {
    expect_error(
        "<?php iconv_strlen('abc', [1, 2]);",
        "iconv_strlen() encoding argument must be string or null",
    );
}

/// Verifies `iconv_mime_encode()` rejects a non-array `$options` argument.
#[test]
fn test_error_iconv_mime_encode_options_type() {
    expect_error(
        "<?php iconv_mime_encode('Subject', 'value', 'not-an-array');",
        "iconv_mime_encode() options argument must be array",
    );
}
