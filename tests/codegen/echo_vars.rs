use crate::support::*;

// --- Phase 1: Echo strings ---

#[test]
fn test_echo_hello_world() {
    let out = compile_and_run("<?php echo \"Hello, World!\\n\";");
    assert_eq!(out, "Hello, World!\n");
}

#[test]
fn test_echo_empty_string() {
    let out = compile_and_run("<?php echo \"\";");
    assert_eq!(out, "");
}

#[test]
fn test_echo_multiple_strings() {
    let out = compile_and_run("<?php echo \"foo\"; echo \"bar\"; echo \"\\n\";");
    assert_eq!(out, "foobar\n");
}

#[test]
fn test_echo_escape_sequences() {
    let out = compile_and_run("<?php echo \"a\\tb\\nc\";");
    assert_eq!(out, "a\tb\nc");
}

// --- Phase 2: Variables and integers ---

#[test]
fn test_echo_integer() {
    let out = compile_and_run("<?php echo 42;");
    assert_eq!(out, "42");
}

#[test]
fn test_echo_zero() {
    let out = compile_and_run("<?php echo 0;");
    assert_eq!(out, "0");
}

#[test]
fn test_echo_negative() {
    let out = compile_and_run("<?php echo -7;");
    assert_eq!(out, "-7");
}

#[test]
fn test_echo_large_number() {
    let out = compile_and_run("<?php echo 1000000;");
    assert_eq!(out, "1000000");
}

#[test]
fn test_variable_int() {
    let out = compile_and_run("<?php $x = 42; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_variable_string() {
    let out = compile_and_run("<?php $s = \"hello\"; echo $s;");
    assert_eq!(out, "hello");
}

#[test]
fn test_variable_reassign_same_type() {
    let out = compile_and_run("<?php $x = 1; $x = 2; echo $x;");
    assert_eq!(out, "2");
}

#[test]
fn test_multiple_variables() {
    let out =
        compile_and_run("<?php $a = 10; $b = 20; echo $a; echo \" \"; echo $b; echo \"\\n\";");
    assert_eq!(out, "10 20\n");
}

#[test]
fn test_variable_negative_int() {
    let out = compile_and_run("<?php $x = -100; echo $x;");
    assert_eq!(out, "-100");
}

#[test]
fn test_echo_int_zero_variable() {
    let out = compile_and_run("<?php $z = 0; echo $z;");
    assert_eq!(out, "0");
}
