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
        echo ((string)$text)[0], ':', $object->name, ':', count($items), ':', is_array($boxed) ? 'array' : 'string', '|';
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

/// A `??` result narrowed to a class at a property store hands its boxed owner back, so a
/// parent/child pair that points at each other through such stores is fully released.
#[test]
fn test_coalesce_object_property_store_retires_the_boxed_owner() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CoalesceNode {
    public $parent = null;
    public $kids = null;
    public $doc = null;
    public function add(CoalesceNode $child): void {
        $this->kids = [$child];
        $child->parent = $this;
        $child->doc = $this->doc ?? $this;
    }
}
$root = new CoalesceNode();
$leaf = new CoalesceNode();
$root->add($leaf);
unset($root, $leaf);
gc_collect_cycles();
echo "done";
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
