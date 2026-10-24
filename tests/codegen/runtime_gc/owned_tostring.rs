//! Purpose:
//! Verifies ownership of freshly allocated strings returned by object conversion methods.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - A runtime repeat count prevents folding the method result into a persistent literal.
//! - Typed casts, direct echo, and boxed echo must each retire the method result once.

use crate::support::compile_and_run_with_heap_debug;

/// Owned __toString results are released after explicit casts and both echo receiver shapes.
#[test]
fn test_owned_tostring_cast_and_echo_heap_balance() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class OwnedStringEmitter {
    private int $copies;
    public function __construct(int $copies) { $this->copies = $copies; }
    public function __toString(): string { return str_repeat('s', $this->copies); }
}
function echoOwnedStringMixed(mixed $value): void { echo $value; }
$object = new OwnedStringEmitter($argc);
echo (string)$object;
echo $object;
echoOwnedStringMixed($object);
$stored = (string)$object;
echo $stored;
unset($stored, $object);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ssss");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
