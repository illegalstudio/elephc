//! Purpose:
//! Ownership regressions for native isset probes reached through nullable receivers.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Rooted nullable property results are retired once at the probe merge.
//! - Direct null and missing-container receivers exercise the quiet null arm under heap debug.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Nullable native isset probes retire rooted hash receivers exactly once on every branch.
#[test]
fn test_nullable_native_isset_retires_rooted_receiver_once() {
    let check = || {
        let output = compile_and_run_with_heap_debug(
            r#"<?php
class NullableNativeIssetBox {
    public array<string, int> $items = ["present" => 1];
}

function probeNullableNativeIsset(?NullableNativeIssetBox $box, string $key): bool {
    return isset($box->items[$key]);
}

function probeMissingNativeIsset(
    array<NullableNativeIssetBox> $boxes,
    int $index,
    string $key
): bool {
    return isset($boxes[$index]->items[$key]);
}

$box = new NullableNativeIssetBox();
echo probeNullableNativeIsset(null, "present") ? "bad" : "null";
echo ":", probeNullableNativeIsset($box, "present") ? "present" : "bad";
echo ":", probeNullableNativeIsset($box, "missing") ? "bad" : "missing";
echo "|";
echo probeMissingNativeIsset([], 0, "present") ? "bad" : "absent";
echo ":", probeMissingNativeIsset([$box], 0, "present") ? "present" : "bad";
"#,
        );
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "null:present:missing|absent:present", "{}", output.stderr);
        assert!(
            output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "{}",
            output.stderr
        );
    };
    check();
    without_ir_opt(check);
}
