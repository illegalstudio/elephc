//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc string and type builtin diagnostics.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

// Tests strlen() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_strlen_wrong_args,
    "<?php strlen();",
    "strlen() takes exactly 1 argument"
);

// Tests intval() arity error when called with no arguments. The optional second parameter
// (`$base`) is now accepted, so the derived phrasing is a range rather than an exact count.
expect_builtin_arity_error!(
    test_error_intval_wrong_args,
    "<?php intval();",
    "intval() takes 1 or 2 arguments"
);

/// Verifies the `is_integer()` alias keeps the one-argument predicate contract.
#[test]
fn test_error_is_integer_wrong_args() {
    expect_error("<?php is_integer();", "is_integer() takes exactly 1 argument");
}

/// Verifies the `is_long()` alias keeps the one-argument predicate contract.
#[test]
fn test_error_is_long_wrong_args() {
    expect_error("<?php is_long();", "is_long() takes exactly 1 argument");
}

/// Verifies the `is_double()` alias keeps the one-argument predicate contract.
#[test]
fn test_error_is_double_wrong_args() {
    expect_error("<?php is_double();", "is_double() takes exactly 1 argument");
}

/// Verifies `strval()` requires exactly one value to convert.
#[test]
fn test_error_strval_wrong_args() {
    expect_error("<?php strval();", "strval() takes exactly 1 argument");
}

// Tests strrpos() arity error when called with only one argument (needs haystack + needle).
// The optional third parameter (`$offset`) is now accepted, so the derived phrasing is a
// range rather than an exact count.
expect_builtin_arity_error!(
    test_error_strrpos_wrong_args,
    "<?php strrpos(\"abc\");",
    "strrpos() takes 2 or 3 arguments"
);

// Tests strstr() arity error when called with only one argument (needs haystack + needle).
// The optional third parameter (`$before_needle`) is now accepted, so the derived phrasing is
// the two-value-range one (`substr()` reads the same way). Reference PHP 8.5.6 raises
// "strstr() expects at least 2 arguments, 1 given" here and "expects at most 3 arguments" for a
// fourth, i.e. the same 2..=3 window.
expect_builtin_arity_error!(
    test_error_strstr_wrong_args,
    "<?php strstr(\"abc\");",
    "strstr() takes 2 or 3 arguments"
);

// Tests strstr() arity error when called with a fourth argument (max is haystack + needle +
// before_needle).
expect_builtin_arity_error!(
    test_error_strstr_too_many_args,
    "<?php strstr(\"abc\", \"b\", true, 1);",
    "strstr() takes 2 or 3 arguments"
);

// Tests strtolower() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_strtolower_wrong_args,
    "<?php strtolower();",
    "strtolower() takes exactly 1 argument"
);

// Tests strtoupper() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_strtoupper_wrong_args,
    "<?php strtoupper();",
    "strtoupper() takes exactly 1 argument"
);

// Tests ucfirst() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_ucfirst_wrong_args,
    "<?php ucfirst();",
    "ucfirst() takes exactly 1 argument"
);

// Tests lcfirst() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_lcfirst_wrong_args,
    "<?php lcfirst();",
    "lcfirst() takes exactly 1 argument"
);

// Tests trim() arity error when called with three arguments (accepts 1 or 2).
expect_builtin_arity_error!(
    test_error_trim_wrong_args,
    "<?php trim(\"x\", \"y\", \"z\");",
    "trim() takes 1 or 2 arguments"
);

// Tests ltrim() arity error when called with three arguments (accepts 1 or 2).
expect_builtin_arity_error!(
    test_error_ltrim_wrong_args,
    "<?php ltrim(\"x\", \"y\", \"z\");",
    "ltrim() takes 1 or 2 arguments"
);

// Tests rtrim() arity error when called with three arguments (accepts 1 or 2).
expect_builtin_arity_error!(
    test_error_rtrim_wrong_args,
    "<?php rtrim(\"x\", \"y\", \"z\");",
    "rtrim() takes 1 or 2 arguments"
);

// Tests str_repeat() arity error when called with only one argument (needs string + count).
expect_builtin_arity_error!(
    test_error_str_repeat_wrong_args,
    "<?php str_repeat(\"x\");",
    "str_repeat() takes exactly 2 arguments"
);

// Tests strrev() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_strrev_wrong_args,
    "<?php strrev();",
    "strrev() takes exactly 1 argument"
);

// Tests chr() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_chr_wrong_args,
    "<?php chr();",
    "chr() takes exactly 1 argument"
);

// Tests strcmp() arity error when called with only one argument (needs two strings).
expect_builtin_arity_error!(
    test_error_strcmp_wrong_args,
    "<?php strcmp(\"a\");",
    "strcmp() takes exactly 2 arguments"
);

// Tests strcasecmp() arity error when called with only one argument (needs two strings).
expect_builtin_arity_error!(
    test_error_strcasecmp_wrong_args,
    "<?php strcasecmp(\"a\");",
    "strcasecmp() takes exactly 2 arguments"
);

// Tests str_contains() arity error when called with only one argument (needs haystack + needle).
expect_builtin_arity_error!(
    test_error_str_contains_wrong_args,
    "<?php str_contains(\"a\");",
    "str_contains() takes exactly 2 arguments"
);

// Tests str_starts_with() arity error when called with only one argument (needs haystack + needle).
expect_builtin_arity_error!(
    test_error_str_starts_with_wrong_args,
    "<?php str_starts_with(\"a\");",
    "str_starts_with() takes exactly 2 arguments"
);

// Tests str_ends_with() arity error when called with only one argument (needs haystack + needle).
expect_builtin_arity_error!(
    test_error_str_ends_with_wrong_args,
    "<?php str_ends_with(\"a\");",
    "str_ends_with() takes exactly 2 arguments"
);

// Tests implode() arity error for a genuinely invalid call. The one-argument form
// `implode($array)` is valid PHP and now compiles, so the rejected shapes are zero
// arguments and three, which PHP reports as ArgumentCountError.
expect_builtin_arity_error!(
    test_error_implode_wrong_args,
    "<?php implode();",
    "implode() takes 1 or 2 arguments"
);

// Tests implode() arity error when called with more arguments than PHP accepts.
expect_builtin_arity_error!(
    test_error_implode_too_many_args,
    "<?php implode(\",\", [1], 3);",
    "implode() takes 1 or 2 arguments"
);

// Tests ucwords() arity error when called with no arguments. The optional second parameter
// (`$separators`) is now accepted, so the derived phrasing is a range rather than an exact count.
expect_builtin_arity_error!(
    test_error_ucwords_wrong_args,
    "<?php ucwords();",
    "ucwords() takes 1 or 2 arguments"
);

// Tests str_ireplace() arity error when called with only two arguments (needs search, replace, subject).
expect_builtin_arity_error!(
    test_error_str_ireplace_wrong_args,
    "<?php str_ireplace(\"a\", \"b\");",
    "str_ireplace() takes 3 or 4 arguments"
);

// Tests str_split() arity error when called with too many arguments (accepts 1 or 2).
expect_builtin_arity_error!(
    test_error_str_split_wrong_args,
    "<?php str_split(\"abc\", 1, 2);",
    "str_split() takes 1 or 2 arguments"
);

// Tests addslashes() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_addslashes_wrong_args,
    "<?php addslashes();",
    "addslashes() takes exactly 1 argument"
);

// Tests stripslashes() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_stripslashes_wrong_args,
    "<?php stripslashes();",
    "stripslashes() takes exactly 1 argument"
);

// Tests nl2br() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_nl2br_wrong_args,
    "<?php nl2br();",
    "nl2br() takes exactly 1 argument"
);

// Tests wordwrap() arity error when called with too many arguments (accepts 1 to 4).
expect_builtin_arity_error!(
    test_error_wordwrap_wrong_args,
    "<?php wordwrap(\"a\", 1, \"-\", true, 5);",
    "wordwrap() takes 1 to 4 arguments"
);

// Tests bin2hex() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_bin2hex_wrong_args,
    "<?php bin2hex();",
    "bin2hex() takes exactly 1 argument"
);

// Tests hex2bin() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_hex2bin_wrong_args,
    "<?php hex2bin();",
    "hex2bin() takes exactly 1 argument"
);

// Tests htmlentities() arity error when called with no arguments. htmlentities()
// accepts optional `$flags` and `$encoding` arguments, so the message reports 1 to 3 args.
expect_builtin_arity_error!(
    test_error_htmlentities_wrong_args,
    "<?php htmlentities();",
    "htmlentities() takes 1 to 3 arguments"
);

// Tests html_entity_decode() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_html_entity_decode_wrong_args,
    "<?php html_entity_decode();",
    "html_entity_decode() takes exactly 1 argument"
);

// Tests urldecode() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_urldecode_wrong_args,
    "<?php urldecode();",
    "urldecode() takes exactly 1 argument"
);

// Tests rawurldecode() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_rawurldecode_wrong_args,
    "<?php rawurldecode();",
    "rawurldecode() takes exactly 1 argument"
);

// Tests is_bool() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_is_bool_wrong_args,
    "<?php is_bool();",
    "is_bool() takes exactly 1 argument"
);

// Tests boolval() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_boolval_wrong_args,
    "<?php boolval();",
    "boolval() takes exactly 1 argument"
);

// Tests is_string() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_is_string_wrong_args,
    "<?php is_string();",
    "is_string() takes exactly 1 argument"
);

// Tests is_numeric() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_is_numeric_wrong_args,
    "<?php is_numeric();",
    "is_numeric() takes exactly 1 argument"
);

// Tests is_iterable() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_is_iterable_wrong_args,
    "<?php is_iterable();",
    "is_iterable() takes exactly 1 argument"
);

// Tests is_callable() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_is_callable_wrong_args,
    "<?php is_callable();",
    "is_callable() takes exactly 1 argument"
);

// Tests quotemeta() arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_quotemeta_wrong_args,
    "<?php quotemeta();",
    "quotemeta() takes exactly 1 argument"
);

// Tests chunk_split() arity error when called with too many arguments (accepts 1 to 3).
expect_builtin_arity_error!(
    test_error_chunk_split_wrong_args,
    "<?php chunk_split(\"a\", 1, \"-\", 5);",
    "chunk_split() takes 1 to 3 arguments"
);

// Tests str_word_count() arity error when called with too many arguments (accepts 1 to 3).
expect_builtin_arity_error!(
    test_error_str_word_count_wrong_args,
    "<?php str_word_count(\"a\", 0, \"x\", 5);",
    "str_word_count() takes 1 to 3 arguments"
);

// Tests count_chars() arity error when called with too many arguments (accepts 1 to 2).
expect_builtin_arity_error!(
    test_error_count_chars_wrong_args,
    "<?php count_chars(\"a\", 0, 1);",
    "count_chars() takes 1 or 2 arguments"
);

// Tests strtr() arity error when called with too many arguments (accepts 2 to 3).
expect_builtin_arity_error!(
    test_error_strtr_wrong_args,
    "<?php strtr(\"a\", \"b\", \"c\", \"d\");",
    "strtr() takes 2 or 3 arguments"
);

// Tests strtr() rejecting a string $from in the two-argument replacement-pair form, using
// php-src's own TypeError wording.
expect_builtin_arity_error!(
    test_error_strtr_two_arg_string_from,
    "<?php strtr(\"abc\", \"a\");",
    "strtr(): Argument #2 ($from) must be of type array, string given"
);

// Tests strtr() rejecting an array $from in the three-argument pairwise form, using php-src's
// own TypeError wording.
expect_builtin_arity_error!(
    test_error_strtr_three_arg_array_from,
    "<?php strtr(\"abc\", [\"a\" => \"b\"], \"x\");",
    "strtr(): Argument #2 ($from) must be of type string, array given"
);

// Tests str_word_count() rejecting a $format that elephc cannot resolve at compile time.
expect_builtin_arity_error!(
    test_error_str_word_count_non_literal_format,
    "<?php $f = strlen(\"ab\"); str_word_count(\"a b\", $f);",
    "str_word_count() format argument must be an integer literal in AOT mode"
);

// Tests count_chars() rejecting a $mode that elephc cannot resolve at compile time.
expect_builtin_arity_error!(
    test_error_count_chars_non_literal_mode,
    "<?php $m = strlen(\"ab\"); count_chars(\"ab\", $m);",
    "count_chars() mode argument must be an integer literal in AOT mode"
);
