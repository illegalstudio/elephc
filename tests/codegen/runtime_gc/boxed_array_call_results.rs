//! Purpose:
//! Verifies caller argument ownership when a declared PHP array uses boxed return storage.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Array containers cannot alias object arguments, but array children must retain those objects.

use crate::support::*;

/// Unrelated object arguments die after a call, while objects returned inside an array remain alive.
#[test]
fn test_core_php_array_return_releases_object_argument_and_retains_children() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class BoxedArrayResultOwner {
    public function fresh(): array { return ["fresh"]; }
    public function held(): array { return [$this]; }
    public function __destruct() { echo "drop|"; }
}
function freshArrayResult(BoxedArrayResultOwner $owner): array { return $owner->fresh(); }
function heldArrayResult(BoxedArrayResultOwner $owner): array { return $owner->held(); }
echo freshArrayResult(new BoxedArrayResultOwner())[0], "|";
$held = heldArrayResult(new BoxedArrayResultOwner());
echo count($held), "|";
unset($held);
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop|fresh|1|drop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
