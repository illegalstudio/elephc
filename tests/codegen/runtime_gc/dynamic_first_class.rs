//! Purpose:
//! Checks ownership of dynamic first-class callable conversions for issue 828.
//!
//! Called from:
//! - The runtime GC codegen tests.
//!
//! Key details:
//! - A retained descriptor keeps its receiver alive and retires every temporary owner.

use crate::support::compile_and_run_with_heap_debug;

/// Repeated conversions and discarded results release their descriptors and receivers.
#[test]
fn test_issue_828_dynamic_first_class_callable_heap() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Invokable828 {
    public function __invoke(): int { return 9; }
}
function wrap828(callable $method): Closure { return $method(...); }
$total = 0;
for ($i = 0; $i < 3; $i++) {
    $object = new Invokable828();
    $saved = $object(...);
    $discard = wrap828($saved);
    $saved(...);
    unset($object);
    $total += $discard();
    unset($saved);
    unset($discard);
}
echo $total;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "27", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
