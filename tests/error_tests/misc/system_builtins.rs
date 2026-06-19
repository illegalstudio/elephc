//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc system builtin diagnostics, including undefined constant, define wrong args, and define non string name.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

expect_builtin_arity_error!(
    test_error_exit_wrong_args,
    "<?php exit(1, 2);",
    "exit() takes 0 or 1 arguments"
);

expect_builtin_arity_error!(
    test_error_die_wrong_args,
    "<?php die(1, 2);",
    "exit() takes 0 or 1 arguments"
);

/// Verifies that referencing an undefined constant produces the expected "Undefined constant" error.
#[test]
fn test_error_undefined_constant() {
    expect_error("<?php echo UNDEFINED_CONST;", "Undefined constant");
}

/// Verifies that `define()` with a single argument (missing value) yields a wrong-args diagnostic.
#[test]
fn test_error_define_wrong_args() {
    expect_error("<?php define(\"X\");", "define() takes exactly 2 arguments");
}

/// Verifies that `define()` with a non-string first argument (int name) yields a non-string-name error.
#[test]
fn test_error_define_non_string_name() {
    expect_error(
        "<?php define(42, 100);",
        "define() first argument must be a string literal",
    );
}

/// Verifies that `defined()` requires exactly one argument.
#[test]
fn test_error_defined_wrong_args() {
    expect_error("<?php defined();", "defined() takes exactly 1 argument");
}

/// Verifies that `defined()` requires a string literal in AOT mode.
#[test]
fn test_error_defined_non_literal_name() {
    expect_error(
        "<?php $name = \"PHP_OS\"; defined($name);",
        "defined() first argument must be a string literal in AOT mode",
    );
}

// -- List unpack errors --

/// Verifies that `time()` with any arguments yields a no-args diagnostic.
#[test]
fn test_error_time_wrong_args() {
    expect_error("<?php time(1);", "time() takes no arguments");
}

/// Verifies that `microtime()` with two arguments yields a wrong-args diagnostic.
#[test]
fn test_error_microtime_wrong_args() {
    expect_error(
        "<?php microtime(1, 2);",
        "microtime() takes 0 or 1 arguments",
    );
}

/// Verifies that `sleep()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_sleep_wrong_args() {
    expect_error("<?php sleep();", "sleep() takes exactly 1 argument");
}

/// Verifies that `usleep()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_usleep_wrong_args() {
    expect_error("<?php usleep();", "usleep() takes exactly 1 argument");
}

/// Verifies that `getenv()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_getenv_wrong_args() {
    expect_error("<?php getenv();", "getenv() takes exactly 1 argument");
}

/// Verifies that `putenv()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_putenv_wrong_args() {
    expect_error("<?php putenv();", "putenv() takes exactly 1 argument");
}

/// Verifies that `phpversion()` with any arguments yields a no-args diagnostic.
#[test]
fn test_error_phpversion_wrong_args() {
    expect_error("<?php phpversion(1);", "phpversion() takes no arguments");
}

/// Verifies that `php_uname()` with two arguments yields a wrong-args diagnostic.
#[test]
fn test_error_php_uname_wrong_args() {
    expect_error(
        "<?php php_uname(1, 2);",
        "php_uname() takes 0 or 1 arguments",
    );
}

/// Verifies that `php_uname()` with a non-string mode argument yields a wrong-type diagnostic.
#[test]
fn test_error_php_uname_wrong_type() {
    expect_error("<?php php_uname(1);", "php_uname() argument must be string");
}

/// Verifies that `exec()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_exec_wrong_args() {
    expect_error("<?php exec();", "exec() takes exactly 1 argument");
}

/// Verifies that `shell_exec()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_shell_exec_wrong_args() {
    expect_error(
        "<?php shell_exec();",
        "shell_exec() takes exactly 1 argument",
    );
}

/// Verifies that `system()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_system_wrong_args() {
    expect_error("<?php system();", "system() takes exactly 1 argument");
}

/// Verifies that `passthru()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_passthru_wrong_args() {
    expect_error("<?php passthru();", "passthru() takes exactly 1 argument");
}

// -- Global/Static parse errors --

/// Verifies that `date()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_date_no_args() {
    expect_error("<?php date();", "date() takes 1 or 2 arguments");
}

/// Verifies that `gmdate()` with no arguments yields a wrong-args diagnostic naming `gmdate`.
#[test]
fn test_error_gmdate_no_args() {
    expect_error("<?php gmdate();", "gmdate() takes 1 or 2 arguments");
}

/// Verifies that `gmdate()` with three arguments yields a wrong-args diagnostic.
#[test]
fn test_error_gmdate_too_many_args() {
    expect_error(
        "<?php gmdate(\"Y\", 0, 1);",
        "gmdate() takes 1 or 2 arguments",
    );
}

/// Verifies `mktime()` arity: PHP 8.0+ accepts 0–6 arguments (omitted ones default to the
/// corresponding current-time component via the procedural-alias desugar), so seven arguments is
/// out of range and yields the fixed-arity diagnostic.
#[test]
fn test_error_mktime_wrong_args() {
    expect_error(
        "<?php mktime(1, 2, 3, 4, 5, 6, 7);",
        "mktime() takes exactly 6 arguments",
    );
}

/// Verifies that `strtotime()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_strtotime_no_args() {
    expect_error("<?php strtotime();", "strtotime() takes 1 or 2 arguments");
}

/// Verifies that `strtotime()` with three arguments yields a wrong-args diagnostic
/// (the optional `baseTimestamp` is the only second argument).
#[test]
fn test_error_strtotime_too_many_args() {
    expect_error(
        "<?php strtotime(\"now\", 0, 1);",
        "strtotime() takes 1 or 2 arguments",
    );
}

/// Verifies that `checkdate()` with two arguments yields a wrong-args diagnostic
/// (it requires exactly month, day, and year).
#[test]
fn test_error_checkdate_wrong_args() {
    expect_error(
        "<?php checkdate(1, 2);",
        "checkdate() takes exactly 3 arguments",
    );
}

// -- date/time alias arity diagnostics --
// Procedural date/time aliases are desugared by the name resolver only at their supported
// arities. A wrong-arity call must report a precise arity error (matching `function_exists()`,
// which recognizes these names) rather than the misleading "Undefined function". Each test below
// covers a distinct message shape produced by the checker's alias-arity diagnostic.

/// `idate()` accepts 1 or 2 arguments; a zero-arg call reports the "N or M" message shape.
#[test]
fn test_error_idate_too_few_args() {
    expect_error("<?php idate();", "idate() takes 1 or 2 arguments");
}

/// A date alias called with too MANY arguments is diagnosed by arity, not as undefined.
#[test]
fn test_error_idate_too_many_args() {
    expect_error("<?php idate(\"Y\", 0, 1);", "idate() takes 1 or 2 arguments");
}

/// `gregoriantojd()` requires exactly 3 arguments (the "exactly N" message shape).
#[test]
fn test_error_gregoriantojd_wrong_args() {
    expect_error(
        "<?php gregoriantojd(1, 2);",
        "gregoriantojd() takes exactly 3 arguments",
    );
}

/// `jdtogregorian()` requires exactly 1 argument (singular wording in the message).
#[test]
fn test_error_jdtogregorian_wrong_args() {
    expect_error(
        "<?php jdtogregorian();",
        "jdtogregorian() takes exactly 1 argument",
    );
}

/// `easter_date()` accepts 0 to 2 arguments (the "N to M" message shape).
#[test]
fn test_error_easter_date_too_many_args() {
    expect_error(
        "<?php easter_date(1, 2, 3);",
        "easter_date() takes 0 to 2 arguments",
    );
}

/// `date_sunrise()` accepts 1 to 6 arguments; a zero-arg call reports the wide "N to M" range.
#[test]
fn test_error_date_sunrise_too_few_args() {
    expect_error(
        "<?php date_sunrise();",
        "date_sunrise() takes 1 to 6 arguments",
    );
}

/// `timezone_version_get()` takes no arguments (the "exactly 0" message shape).
#[test]
fn test_error_timezone_version_get_wrong_args() {
    expect_error(
        "<?php timezone_version_get(1);",
        "timezone_version_get() takes exactly 0 arguments",
    );
}

// -- JSON error tests --

/// Verifies that `json_encode()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_json_encode_no_args() {
    expect_error(
        "<?php json_encode();",
        "json_encode() takes 1 to 3 arguments",
    );
}

/// Verifies that `json_decode()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_json_decode_no_args() {
    expect_error(
        "<?php json_decode();",
        "json_decode() takes 1 to 4 arguments",
    );
}

/// Verifies that `json_validate()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_json_validate_no_args() {
    expect_error(
        "<?php json_validate();",
        "json_validate() takes 1 to 3 arguments",
    );
}

/// Verifies that `json_last_error()` with arguments yields a no-args diagnostic.
#[test]
fn test_error_json_last_error_with_args() {
    expect_error(
        "<?php json_last_error(1);",
        "json_last_error() takes no arguments",
    );
}

/// Verifies that `json_last_error_msg()` with arguments yields a no-args diagnostic.
#[test]
fn test_error_json_last_error_msg_with_args() {
    expect_error(
        "<?php json_last_error_msg(1);",
        "json_last_error_msg() takes no arguments",
    );
}

// -- Regex error tests --

/// Verifies that `preg_match()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_preg_match_no_args() {
    expect_error(
        "<?php preg_match();",
        "preg_match() takes 2 or 3 arguments",
    );
}

/// Verifies that `preg_match()` with only the pattern argument yields a wrong-args diagnostic.
#[test]
fn test_error_preg_match_one_arg() {
    expect_error(
        r#"<?php preg_match("/test/");"#,
        "preg_match() takes 2 or 3 arguments",
    );
}

/// Verifies that `preg_match()` rejects non-variable output arguments for `$matches`.
#[test]
fn test_error_preg_match_matches_must_be_variable() {
    expect_error(
        r#"<?php preg_match("/test/", "test", []);"#,
        "preg_match() parameter $matches must be passed a variable",
    );
}

/// Verifies that `preg_match()` rejects arguments beyond the supported `$matches` parameter.
#[test]
fn test_error_preg_match_four_args() {
    expect_error(
        r#"<?php preg_match("/test/", "test", $matches, 0);"#,
        "preg_match() takes 2 or 3 arguments",
    );
}

/// Verifies that `preg_match_all()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_preg_match_all_no_args() {
    expect_error(
        "<?php preg_match_all();",
        "preg_match_all() takes exactly 2 arguments",
    );
}

/// Verifies that `preg_replace()` with only two arguments yields a wrong-args diagnostic.
#[test]
fn test_error_preg_replace_wrong_args() {
    expect_error(
        r#"<?php preg_replace("/a/", "b");"#,
        "preg_replace() takes exactly 3 arguments",
    );
}

/// Verifies that `preg_replace_callback()` with only two arguments yields a wrong-args diagnostic.
#[test]
fn test_error_preg_replace_callback_wrong_args() {
    expect_error(
        r#"<?php preg_replace_callback("/a/", function($matches) { return $matches[0]; });"#,
        "preg_replace_callback() takes exactly 3 arguments",
    );
}

/// Verifies that `preg_split()` with no arguments yields a wrong-args diagnostic.
#[test]
fn test_error_preg_split_no_args() {
    expect_error(
        "<?php preg_split();",
        "preg_split() takes between 2 and 4 arguments",
    );
}

// -- Hex literal errors --

/// Verifies that concatenating an undefined constant with a string path inside `require` produces a
/// diagnostic that references the undefined constant name.
#[test]
fn test_include_path_with_undefined_const_errors() {
    let err = resolver_error("<?php require UNDEFINED . '/x.php';");
    assert!(
        err.message.contains("UNDEFINED"),
        "message should reference the undefined constant: {}",
        err.message
    );
}
