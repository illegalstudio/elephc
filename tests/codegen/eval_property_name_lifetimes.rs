//! Purpose:
//! Verifies computed intermediate property-name lifetimes in opaque eval writes.
//!
//! Called from:
//! - The executable codegen CI suite.
//!
//! Key details:
//! - Name destruction precedes the outer read and RHS destruction, including throw cleanup.

use crate::support::*;

/// Intermediate name captures expire after selection and before the outer compound read.
#[test]
fn test_eval_intermediate_property_name_expires_before_outer_write_bundle() {
    let source = r#"<?php
class IntermediateNameLifetime {
    public function __construct(private object $root, private bool $fail = false) {}
    public function __toString(): string { echo 'name|'; return 'b'; }
    public function __destruct() {
        echo 'drop-name|';
        if ($this->fail) { throw new RuntimeException('name failed'); }
        $this->root->b->c = 'd';
    }
}
class IntermediateRhsLifetime {
    public function __toString(): string { echo 'rhs-cast|'; return 'x'; }
    public function __destruct() { echo 'drop-rhs|'; }
}
$code = $argc > 0 ? '
$root = new stdClass(); $root->b = new stdClass(); $root->b->c = "p";
$root->{new IntermediateNameLifetime($root)}->c .= new IntermediateRhsLifetime();
echo $root->b->c, "|";
$root->b->c = "p";
try { $root->{new IntermediateNameLifetime($root, true)}->c .= new IntermediateRhsLifetime(); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
echo $root->b->c, "|";
unset($root, $error);
' : '';
eval($code);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "name|drop-name|rhs-cast|drop-rhs|dx|",
        "name|drop-name|drop-rhs|name failed|p|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A coalescing statement only converts its parent name again when the fallback writes.
#[test]
fn test_property_coalesce_keep_branch_does_not_replay_parent_name() {
    let source = r#"<?php
class CoalesceParentName {
    public function __toString(): string { echo 'name|'; return 'child'; }
}
class CoalesceParentNode { public mixed $value = 1; }
function coalesceParentWrite(mixed $root, mixed $name): void { $root->{$name}->value ??= 2; }
$root = new stdClass(); $root->child = new CoalesceParentNode(); $name = new CoalesceParentName();
coalesceParentWrite($root, $name); echo $root->child->value, '|';
unset($root->child->value);
coalesceParentWrite($root, $name); echo $root->child->value, '|';
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "name|1|name|name|2|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
