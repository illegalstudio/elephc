use crate::support::*;

// --- Single-quoted strings ---

#[test]
fn test_single_quoted_string() {
    let out = compile_and_run("<?php echo 'hello';");
    assert_eq!(out, "hello");
}

#[test]
fn test_single_quoted_no_escape() {
    let out = compile_and_run(r"<?php echo 'no\n escape';");
    assert_eq!(out, "no\\n escape");
}

#[test]
fn test_single_quoted_escaped_quote() {
    let out = compile_and_run("<?php echo 'it\\'s';");
    assert_eq!(out, "it's");
}

// --- null ---

#[test]
fn test_null_echo_nothing() {
    let out = compile_and_run("<?php echo null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_variable_echo_nothing() {
    let out = compile_and_run("<?php $x = null; echo $x;");
    assert_eq!(out, "");
}

#[test]
fn test_is_null_true() {
    let out = compile_and_run("<?php $x = null; echo is_null($x);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_null_false() {
    let out = compile_and_run("<?php $x = 42; echo is_null($x);");
    assert_eq!(out, "");
}

#[test]
fn test_null_plus_int() {
    let out = compile_and_run("<?php $x = null; echo $x + 5;");
    assert_eq!(out, "5");
}

#[test]
fn test_null_concat() {
    let out = compile_and_run("<?php $x = null; echo $x . \"hello\";");
    assert_eq!(out, "hello");
}

#[test]
fn test_null_equals_zero() {
    let out = compile_and_run("<?php $x = null; echo $x == 0;");
    assert_eq!(out, "1");
}

#[test]
fn test_null_plus_assign() {
    let out = compile_and_run("<?php $y = null; $y += 10; echo $y;");
    assert_eq!(out, "10");
}

#[test]
fn test_null_reassign() {
    let out = compile_and_run("<?php $x = null; $x = 42; echo $x;");
    assert_eq!(out, "42");
}

// --- Built-in functions ---

#[test]
fn test_strlen() {
    let out = compile_and_run("<?php echo strlen(\"hello\");");
    assert_eq!(out, "5");
}

#[test]
fn test_strlen_empty() {
    let out = compile_and_run("<?php echo strlen(\"\");");
    assert_eq!(out, "0");
}

#[test]
fn test_intval_string() {
    let out = compile_and_run("<?php echo intval(\"42\");");
    assert_eq!(out, "42");
}

#[test]
fn test_intval_negative() {
    let out = compile_and_run("<?php echo intval(\"-7\");");
    assert_eq!(out, "-7");
}

#[test]
fn test_intval_int_passthrough() {
    let out = compile_and_run("<?php echo intval(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_exit_code() {
    // We can't easily test exit code in compile_and_run, so test that
    // exit stops execution (nothing after exit is printed)
    let out = compile_and_run("<?php echo \"before\"; exit(0); echo \"after\";");
    assert_eq!(out, "before");
}

// --- $argc ---

#[test]
fn test_argc_exists() {
    let out = compile_and_run("<?php echo $argc;");
    // When run as a test, argc is 1 (just the binary name)
    assert_eq!(out, "1");
}

#[test]
fn test_argv_count_exists() {
    let out = compile_and_run("<?php echo count($argv);");
    assert_eq!(out, "1");
}

#[test]
fn test_argv_first_entry_exists() {
    let out = compile_and_run("<?php echo $argv[0];");
    assert!(out.ends_with("/test"), "unexpected argv[0]: {out}");
}

