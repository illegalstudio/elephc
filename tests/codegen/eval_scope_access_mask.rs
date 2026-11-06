//! Purpose:
//! Regression coverage for selective native-scope synchronization around dynamic eval.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_scope_access_mask`.
//!
//! Key details:
//! - Named variables remain visible and write back across normal and throwing fragments.
//! - Scope introspection falls back to the full native inventory.

use crate::support::*;

const SCOPE_ACCESS_SOURCE: &str = r#"<?php
function namedAccess(string $code): void {
    $target = 10;
    $untouched = "keep";
    try {
        eval($code);
    } catch (RuntimeException $error) {
        echo $error->getMessage(), "|";
    }
    echo $target, "|", $untouched, "\n";
}

function introspectiveAccess(string $code): void {
    $target = "unset";
    $hidden = "visible";
    eval($code);
    echo $target, "|", $hidden, "\n";
}

function scalarAccess(string $code, bool $useString): void {
    $target = $useString ? "4" : 4;
    $untouched = "keep";
    eval($code);
    echo gettype($target), ":", $target, "|", $untouched, "\n";
}

function referenceAccess(string $code): void {
    $target = 6;
    $alias =& $target;
    $untouched = "keep";
    eval($code);
    echo $target, "|", $alias, "|", $untouched, "\n";
}

$suffix = $argc > 99 ? " " : "";
$named = '$target = $target + 5; throw new RuntimeException("caught");' . $suffix;
$introspective = '$vars = call_user_func("get_defined_vars"); $target = $vars["hidden"];' . $suffix;
$scalar = '$target = $target + 1;' . $suffix;
namedAccess($named);
introspectiveAccess($introspective);
scalarAccess($scalar, false);
scalarAccess($scalar, true);
referenceAccess($scalar);
"#;

const RETAINED_INDIRECT_SOURCE: &str = r#"<?php
function retainedIndirectAccess(string $setup, string $invoke, string $direct): void {
    $target = 2;
    eval($setup);
    eval($invoke);
    eval($direct);
    echo $target, "\n";
}

retainedIndirectAccess(
    '$callback = function() use (&$target): void { $target += 5; };',
    '$callback();',
    '$target = $target + 1;'
);
"#;

/// Dynamic eval preserves named writes, exception writeback, and full-scope introspection.
#[test]
fn test_dynamic_eval_scope_access_mask_preserves_php_semantics() {
    for optimized in [false, true] {
        let output = if optimized {
            compile_and_run_with_heap_debug(SCOPE_ACCESS_SOURCE)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(SCOPE_ACCESS_SOURCE))
        };
        assert!(output.success, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "caught|15|keep\nvisible|visible\ninteger:5|keep\ninteger:5|keep\n7|7|keep\n"
        );
        assert!(
            output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected a clean eval boundary, got: {}",
            output.stderr
        );
    }
}

/// A retained eval closure keeps later calls on full synchronization for the activation.
#[test]
fn test_dynamic_eval_scope_access_mask_stays_full_after_indirect_access() {
    for optimized in [false, true] {
        let output = if optimized {
            compile_and_run(RETAINED_INDIRECT_SOURCE)
        } else {
            without_ir_opt(|| compile_and_run(RETAINED_INDIRECT_SOURCE))
        };
        assert_eq!(output, "8\n");
    }
}
