//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc string and type builtin diagnostics.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

expect_builtin_arity_error!(
    test_error_strlen_wrong_args,
    "<?php strlen();",
    "strlen() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_intval_wrong_args,
    "<?php intval();",
    "intval() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_strrpos_wrong_args,
    "<?php strrpos(\"abc\");",
    "strrpos() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_strstr_wrong_args,
    "<?php strstr(\"abc\");",
    "strstr() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_strtolower_wrong_args,
    "<?php strtolower();",
    "strtolower() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_strtoupper_wrong_args,
    "<?php strtoupper();",
    "strtoupper() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_ucfirst_wrong_args,
    "<?php ucfirst();",
    "ucfirst() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_lcfirst_wrong_args,
    "<?php lcfirst();",
    "lcfirst() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_trim_wrong_args,
    "<?php trim(\"x\", \"y\", \"z\");",
    "trim() takes 1 or 2 arguments"
);

expect_builtin_arity_error!(
    test_error_ltrim_wrong_args,
    "<?php ltrim(\"x\", \"y\", \"z\");",
    "ltrim() takes 1 or 2 arguments"
);

expect_builtin_arity_error!(
    test_error_rtrim_wrong_args,
    "<?php rtrim(\"x\", \"y\", \"z\");",
    "rtrim() takes 1 or 2 arguments"
);

expect_builtin_arity_error!(
    test_error_str_repeat_wrong_args,
    "<?php str_repeat(\"x\");",
    "str_repeat() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_strrev_wrong_args,
    "<?php strrev();",
    "strrev() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_chr_wrong_args,
    "<?php chr();",
    "chr() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_strcmp_wrong_args,
    "<?php strcmp(\"a\");",
    "strcmp() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_strcasecmp_wrong_args,
    "<?php strcasecmp(\"a\");",
    "strcasecmp() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_str_contains_wrong_args,
    "<?php str_contains(\"a\");",
    "str_contains() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_str_starts_with_wrong_args,
    "<?php str_starts_with(\"a\");",
    "str_starts_with() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_str_ends_with_wrong_args,
    "<?php str_ends_with(\"a\");",
    "str_ends_with() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_implode_wrong_args,
    "<?php implode([\"a\"]);",
    "implode() takes exactly 2 arguments"
);

expect_builtin_arity_error!(
    test_error_ucwords_wrong_args,
    "<?php ucwords();",
    "ucwords() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_str_ireplace_wrong_args,
    "<?php str_ireplace(\"a\", \"b\");",
    "str_ireplace() takes exactly 3 arguments"
);

expect_builtin_arity_error!(
    test_error_str_split_wrong_args,
    "<?php str_split(\"abc\", 1, 2);",
    "str_split() takes 1 or 2 arguments"
);

expect_builtin_arity_error!(
    test_error_addslashes_wrong_args,
    "<?php addslashes();",
    "addslashes() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_stripslashes_wrong_args,
    "<?php stripslashes();",
    "stripslashes() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_nl2br_wrong_args,
    "<?php nl2br();",
    "nl2br() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_wordwrap_wrong_args,
    "<?php wordwrap(\"a\", 1, \"-\", true, 5);",
    "wordwrap() takes 1 to 4 arguments"
);

expect_builtin_arity_error!(
    test_error_bin2hex_wrong_args,
    "<?php bin2hex();",
    "bin2hex() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_hex2bin_wrong_args,
    "<?php hex2bin();",
    "hex2bin() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_htmlentities_wrong_args,
    "<?php htmlentities();",
    "htmlentities() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_html_entity_decode_wrong_args,
    "<?php html_entity_decode();",
    "html_entity_decode() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_urldecode_wrong_args,
    "<?php urldecode();",
    "urldecode() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_rawurldecode_wrong_args,
    "<?php rawurldecode();",
    "rawurldecode() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_is_bool_wrong_args,
    "<?php is_bool();",
    "is_bool() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_boolval_wrong_args,
    "<?php boolval();",
    "boolval() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_is_string_wrong_args,
    "<?php is_string();",
    "is_string() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_is_numeric_wrong_args,
    "<?php is_numeric();",
    "is_numeric() takes exactly 1 argument"
);

expect_builtin_arity_error!(
    test_error_is_iterable_wrong_args,
    "<?php is_iterable();",
    "is_iterable() takes exactly 1 argument"
);
