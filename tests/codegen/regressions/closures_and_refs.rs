use super::*;

#[test]
fn test_closure_default_param() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_closure_default_param_overridden() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5, 20);
"#,
    );
    assert_eq!(out, "25");
}

#[test]
fn test_for_compound_subtract() {
    let out = compile_and_run(
        r#"<?php
for ($i = 10; $i > 0; $i -= 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "10 7 4 1 ");
}

#[test]
fn test_for_compound_add() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 10; $i += 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "0 3 6 9 ");
}

#[test]
fn test_for_compound_multiply() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 100; $i *= 2) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 32 64 ");
}

#[test]
fn test_for_compound_shift_left() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 20; $i <<= 1) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 ");
}

// --- Bug fix: array push with concat expression ---

#[test]
fn test_closure_use_int() {
    let out = compile_and_run(
        r#"<?php
$factor = 3;
$mul = function($x) use ($factor) { return $x * $factor; };
echo $mul(5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_closure_use_string() {
    let out = compile_and_run(
        r#"<?php
$greeting = "Hello";
$greet = function($name) use ($greeting) { return $greeting . " " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_closure_use_multiple() {
    let out = compile_and_run(
        r#"<?php
$a = 10;
$b = 20;
$sum = function() use ($a, $b) { return $a + $b; };
echo $sum();
"#,
    );
    assert_eq!(out, "30");
}

#[test]
fn test_closure_use_no_params() {
    let out = compile_and_run(
        r#"<?php
$name = "World";
$greet = function() use ($name) {
    echo "Hello " . $name;
};
$greet();
"#,
    );
    assert_eq!(out, "Hello World");
}

// === Memory management regression tests ===
