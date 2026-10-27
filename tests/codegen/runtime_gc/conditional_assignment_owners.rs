//! Purpose:
//! Verifies retirement of owners assigned by overlapping conditional branches.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Runs every pair of branch outcomes from one compiled program.
//! - Checks strings, objects, arrays and widened Mixed slots with strict heap diagnostics.

use crate::support::*;

/// A possibly initialized local releases its old owner before another conditional store.
#[test]
fn test_overlapping_conditionals_retire_previous_local_owners() {
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(r#"<?php
class ConditionalAssignmentOwner {
    public static int $destroyed = 0;
    public string $name = 'owner';
    public function __destruct() { self::$destroyed++; }
}
function conditionalAssignmentOwners(int $flags, mixed $boxed = null): void {
    if (($flags & 1) !== 0) {
        $text = str_repeat('a', 80);
        $object = new ConditionalAssignmentOwner();
        $items = [str_repeat('x', 80)];
        $boxed = [str_repeat('m', 80)];
    }
    if (($flags & 2) !== 0) {
        $text = str_repeat('b', 90);
        $object = new ConditionalAssignmentOwner();
        $items = [str_repeat('y', 90)];
        $boxed = str_repeat('n', 90);
    }
    if ($flags !== 0) {
        echo $text[0], ':', $object->name, ':', count($items), ':', is_array($boxed) ? 'array' : 'string', '|';
    } else { echo 'none|'; }
}
conditionalAssignmentOwners($argc - 1);
echo ConditionalAssignmentOwner::$destroyed;
"#, &[&[], &["x"], &["x", "x"], &["x", "x", "x"]]);
    let expected = ["none|0", "a:owner:1:array|1", "b:owner:1:string|1", "b:owner:1:string|2"];
    assert_eq!(runs.len(), expected.len());
    let diagnostic = selected_function_assembly(&assembly, &[elephc::names::function_symbol("conditionalAssignmentOwners")]);
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        assert!(out.success, "mode={mode} stdout={:?}\n{}\n{diagnostic}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "mode={mode}\n{}\n{diagnostic}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={mode}\n{}\n{diagnostic}", out.stderr);
    }
}

/// Conditional overwrite cleanup remains explicit when EIR optimization is disabled.
#[test]
fn test_overlapping_conditionals_retire_previous_local_owners_no_ir_opt() {
    without_ir_opt(test_overlapping_conditionals_retire_previous_local_owners);
}
