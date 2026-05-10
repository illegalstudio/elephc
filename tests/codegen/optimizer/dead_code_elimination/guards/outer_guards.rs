//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination guards, including dead code elimination prunes nested if region from outer guard, dead code elimination invalidates outer guard after local write, and dead code elimination prunes nested if region from outer strict boolean guard.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_guard() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        if (!$flag) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(true);
run(false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_invalidates_outer_guard_after_local_write() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        $flag = false;
        if ($flag) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_strict_bool_guard() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag === true) {
        if ($flag === false) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(true);
run(false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_invalidates_outer_strict_bool_guard_after_local_write() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag === true) {
        $flag = false;
        if ($flag === true) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_and_guard() {
    let out = compile_and_run(
        r#"<?php
function run($a, $b) {
    if ($a && $b) {
        if (!$a || !$b) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(true, true);
run(true, false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_negated_and_guard() {
    let out = compile_and_run(
        r#"<?php
function run($a, $b) {
    if (!($a && $b)) {
        if ($a && $b) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(true, false);
run(true, true);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_or_false_branch() {
    let out = compile_and_run(
        r#"<?php
function run($a, $b) {
    if (!$a || $b) {
        echo "b";
    } else {
        if ($a && !$b) {
            echo "a";
        } else {
            echo "bad";
        }
    }
}

run(true, false);
run(false, false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_null_guard() {
    let out = compile_and_run(
        r#"<?php
function runNull() {
    $value = null;
    if ($value === null) {
        if ($value !== null) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

function runInt() {
    $value = 1;
    if ($value === null) {
        echo "bad";
    } else {
        echo "b";
    }
}

runNull();
runInt();
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 0) {
        if ($value) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(0);
run(1);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_empty_string_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "") {
        if ($value) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run("");
run("x");
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_string_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "0") {
        if ($value) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run("0");
run("1");
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_zero_float_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 0.0) {
        if ($value) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(0.0);
run(1.5);
"#,
    );

    assert_eq!(out, "ab");
}
