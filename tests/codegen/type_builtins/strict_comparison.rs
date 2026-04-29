use super::*;

#[test]
fn test_strict_eq_int_same() {
    let out = compile_and_run("<?php echo 1 === 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_different() {
    let out = compile_and_run("<?php echo 1 === 2;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_same() {
    let out = compile_and_run("<?php echo 1 !== 1;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_different() {
    let out = compile_and_run("<?php echo 1 !== 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_vs_bool() {
    // 1 === true should be false (different types)
    let out = compile_and_run("<?php echo 1 === true;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_vs_bool() {
    // 1 !== true should be true (different types)
    let out = compile_and_run("<?php echo 1 !== true;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_vs_string() {
    // 1 === "1" should be false (different types)
    let out = compile_and_run("<?php echo 1 === \"1\";");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_string_same() {
    let out = compile_and_run("<?php echo \"hello\" === \"hello\";");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_string_different() {
    let out = compile_and_run("<?php echo \"hello\" === \"world\";");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_string() {
    let out = compile_and_run("<?php echo \"abc\" !== \"def\";");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_true() {
    let out = compile_and_run("<?php echo true === true;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_false() {
    let out = compile_and_run("<?php echo false === false;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_mixed() {
    let out = compile_and_run("<?php echo true === false;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_null() {
    let out = compile_and_run("<?php echo null === null;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_null_vs_int() {
    // null === 0 should be false
    let out = compile_and_run("<?php echo null === 0;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_null_vs_false() {
    // null === false should be false (different types)
    let out = compile_and_run("<?php echo null === false;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_float_same() {
    let out = compile_and_run("<?php echo 3.14 === 3.14;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_float_different() {
    let out = compile_and_run("<?php echo 3.14 === 2.71;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_float_vs_int() {
    // 1.0 === 1 should be false (different types)
    let out = compile_and_run("<?php echo 1.0 === 1;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_in_if() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x === 5) {
    echo "yes";
} else {
    echo "no";
}
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_strict_neq_in_if() {
    let out = compile_and_run(
        r#"<?php
$x = "hello";
if ($x !== "world") {
    echo "different";
} else {
    echo "same";
}
"#,
    );
    assert_eq!(out, "different");
}

#[test]
fn test_strict_eq_string_variables() {
    let out = compile_and_run(
        r#"<?php
$a = "test";
$b = "test";
echo $a === $b;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_neq_string_variables() {
    let out = compile_and_run(
        r#"<?php
$a = "foo";
$b = "bar";
echo $a !== $b;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_side_effects_preserved() {
    // Both operands must be evaluated even when types differ
    let out = compile_and_run(
        r#"<?php
function effect() { echo "X"; return 1; }
$r = 1.0 === effect();
echo $r;
"#,
    );
    assert_eq!(out, "X");
}

#[test]
fn test_strict_eq_assign_result() {
    let out = compile_and_run(
        r#"<?php
$x = 1 === 1;
echo $x;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_neq_assign_result() {
    let out = compile_and_run(
        r#"<?php
$x = 1 !== 2;
echo $x;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_compare_mixed_uses_payload_type_and_value() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "int_a" => 42,
    "int_b" => 42,
    "int_c" => 7,
    "str_a" => "42",
    "str_b" => "42",
    "bool_t" => true,
];
echo $map["int_a"] === $map["int_b"] ? "1" : "0";
echo $map["int_a"] === $map["int_c"] ? "1" : "0";
echo $map["int_a"] === $map["str_a"] ? "1" : "0";
echo $map["str_a"] === $map["str_b"] ? "1" : "0";
echo $map["int_a"] !== $map["str_a"] ? "1" : "0";
echo $map["bool_t"] === true ? "1" : "0";
"#,
    );
    assert_eq!(out, "100111");
}

// --- Include / Require ---
