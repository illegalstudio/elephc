//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination guards, including dead code elimination rebuilds empty elseif tail as needed guard, dead code elimination prunes nested if region from demorgan equivalent guard, and dead code elimination prunes nested if region from loose comparison guard.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

#[test]
fn test_dead_code_elimination_rebuilds_empty_elseif_tail_as_needed_guard() {
    let out = compile_and_run(
        r#"<?php
function poke() {
    echo "a";
    return false;
}

function tap() {
    echo "b";
    return true;
}

if (poke()) {
    echo "x";
} elseif (tap()) {
    strlen("abc");
} else {
    echo "z";
}

echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_demorgan_equivalent_guard() {
    let out = compile_and_run(
        r#"<?php
function run($a, $b) {
    if (!($a && $b)) {
        if (!$a || !$b) {
            echo "a";
        } else {
            echo "bad";
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
fn test_dead_code_elimination_prunes_nested_if_region_from_loose_comparison_guard() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_loose_comparison_guard");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value == 0) {
        if ($value != 0) {
            echo "dead-loose";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(0);
run(2);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "ab");
    assert!(!user_asm.contains("dead-loose"));
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_relational_guard() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_relational_guard");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value > 10) {
        if ($value <= 10) {
            echo "dead-rel";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(11);
run(10);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "ab");
    assert!(!user_asm.contains("dead-rel"));
}

#[test]
fn test_dead_code_elimination_prunes_nested_elseif_from_composite_guard_refinement() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_composite_guard_refinement");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($a, $b, $c) {
    if (($a && $b) || $c) {
        if (!$c) {
            if ($a && $b) {
                echo "a";
            } elseif (true) {
                echo "dead";
            }
        } else {
            echo "c";
        }
    } else {
        echo "x";
    }
}

run(true, true, false);
run(false, false, true);
run(false, false, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "acx");
    assert!(!user_asm.contains("dead"));
}

#[test]
fn test_dead_code_elimination_prunes_nested_subexpr_from_composite_guard_refinement() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_nested_subexpr_guard_refinement");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($a, $b, $c, $d) {
    if ((($a && $b) || $c) && $d) {
        if ($d) {
            if (!$c) {
                if ($a && $b) {
                    echo "a";
                } elseif (true) {
                    echo "dead-ab";
                }
            } else {
                echo "c";
            }
        } else {
            echo "dead-d";
        }
    } else {
        echo "x";
    }
}

run(true, true, false, true);
run(false, false, true, true);
run(false, false, false, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "acx");
    assert!(!user_asm.contains("dead-ab"));
    assert!(!user_asm.contains("dead-d"));
}

#[test]
fn test_dead_code_elimination_drops_unreachable_elseif_suffix_from_cumulative_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_elseif_guard_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function tap($label, $ret) {
    echo $label;
    return $ret;
}

$flag = $argc > 1;
if ($flag) {
    echo "A";
} elseif (!$flag) {
    echo "B";
} elseif (tap("dead-elseif", true)) {
    echo "C";
} else {
    echo "dead-else";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "B");
    assert!(!user_asm.contains("dead-elseif"));
    assert!(!user_asm.contains("dead-else"));
}

#[test]
fn test_dead_code_elimination_drops_unreachable_elseif_suffix_from_negated_composite_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_negated_elseif_guard_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function tap($label, $ret) {
    echo $label;
    return $ret;
}

function run($a, $b) {
    if ($a || $b) {
        echo "A";
    } elseif (!($a || $b)) {
        echo "B";
    } elseif (tap("dead-elseif", true)) {
        echo "C";
    } else {
        echo "dead-else";
    }
}

run(true, false);
run(false, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "AB");
    assert!(!user_asm.contains("dead-elseif"));
    assert!(!user_asm.contains("dead-else"));
}

#[test]
fn test_dead_code_elimination_drops_unreachable_elseif_suffix_from_demorgan_equivalent_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_demorgan_elseif_guard_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($a, $b) {
    if (!($a && $b)) {
        echo "A";
    } elseif (!$a || !$b) {
        echo "dead-equivalent";
    } elseif (true) {
        echo "C";
    } else {
        echo "dead-else";
    }
}

run(true, false);
run(true, true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "AC");
    assert!(!user_asm.contains("dead-equivalent"));
    assert!(!user_asm.contains("dead-else"));
}

#[test]
fn test_dead_code_elimination_preserves_elseif_order_after_empty_head() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", false)) {
} elseif (step("b", true)) {
    echo "!";
}
"#,
    );

    assert_eq!(out, "ab!");
}

#[test]
fn test_dead_code_elimination_skips_elseif_when_empty_head_matches() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", true)) {
} elseif (step("b", true)) {
    echo "!";
}
echo "?";
"#,
    );

    assert_eq!(out, "a?");
}

#[test]
fn test_dead_code_elimination_preserves_regular_elseif_order_after_normalization() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", false)) {
    echo "A";
} elseif (step("b", true)) {
    echo "B";
} else {
    echo "C";
}
"#,
    );

    assert_eq!(out, "abB");
}
