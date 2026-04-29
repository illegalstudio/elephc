use super::*;

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 0) {
        echo "b";
    } else {
        if ($value === 0) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(1);
run(0);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_null_guard() {
    let out = compile_and_run(
        r#"<?php
function runNotNull() {
    $value = 1;
    if ($value !== null) {
        if ($value === null) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

function runNull() {
    $value = null;
    if ($value !== null) {
        echo "bad";
    } else {
        echo "b";
    }
}

runNotNull();
runNull();
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_empty_string_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "") {
        echo "b";
    } else {
        if ($value === "") {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run("x");
run("");
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_string_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "0") {
        echo "b";
    } else {
        if ($value === "0") {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run("1");
run("0");
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_float_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 1.5) {
        echo "b";
    } else {
        if ($value === 1.5) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(2.5);
run(1.5);
"#,
    );

    assert_eq!(out, "ab");
}
