//! Purpose:
//! Exercises object argument owners returned through Mixed boxes at runtime.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Source-evaluation pins and independently retained Mixed payloads must each retire once.

use crate::support::*;

/// Repeated Object-to-Mixed forwarding preserves the returned object and leaves a clean heap.
#[test]
fn test_core_object_arguments_returned_as_mixed_release_evaluation_pins() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class MixedReturnArgumentOwner {
    public string $value = "alive";
    public function __destruct() { echo "drop|"; }
}
function returnArgumentObjectAsMixed(MixedReturnArgumentOwner $owner, int $later): mixed {
    return $owner;
}
function evaluateAfterObjectArgument(): int { return 1; }
for ($i = 0; $i < 3; $i++) {
    $owner = new MixedReturnArgumentOwner();
    $returned = returnArgumentObjectAsMixed($owner, evaluateAfterObjectArgument());
    unset($owner);
    echo $returned->value, "|";
    unset($returned);
}
echo "done";
"#,
    );
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "alive|drop|alive|drop|alive|drop|done", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr,
    );
}
