//! Purpose:
//! End-to-end coverage for iterable-root guards in `foreach` bodies.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core dead_code_elimination::guards`.
//!
//! Key details:
//! - By-reference values may mutate their iterable root invisibly, while
//!   by-value assignments leave the source array unchanged.

use super::*;

/// Verifies by-ref iteration retains both post-write outcomes and by-value iteration still prunes.
#[test]
fn test_dead_code_elimination_invalidates_iterable_guard_for_by_ref_foreach_value() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_foreach_by_ref_root_guard");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function by_ref($a, $b) {
    if ($a == $b) {
        foreach ($a as &$value) {
            $value = 2;
            if ($a == $b) {
                echo "still-equal-by-ref";
            } else {
                echo "changed-by-ref";
            }
        }
    }
}

function by_value($a, $b) {
    if ($a == $b) {
        foreach ($a as $value) {
            $value = 2;
            if ($a == $b) {
                echo "by-value-equal";
            } else {
                echo "dead-by-value-changed";
            }
        }
    }
}

function after_loop($a, $b) {
    foreach ($a as &$value) {
    }
    if ($a == $b) {
        $value = 2;
        if ($a == $b) {
            echo "stale-after-loop";
        } else {
            echo "changed-after-loop";
        }
    }
}

by_ref([1], [1]);
echo "|";
by_ref([2], [2]);
echo "|";
by_value([1], [1]);
echo "|";
after_loop([1], [1]);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(user_asm.contains("changed-by-ref"));
    assert!(user_asm.contains("still-equal-by-ref"));
    assert!(!user_asm.contains("dead-by-value-changed"));
    assert!(user_asm.contains("stale-after-loop"));
    assert!(user_asm.contains("changed-after-loop"));
}
