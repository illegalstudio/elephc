//! Purpose:
//! Verifies callable property stores retain only the published descriptor owner.
//!
//! Called from:
//! - The native runtime GC codegen suite.
//!
//! Key details:
//! - Heap/tagged fixtures check replacement, borrowed parameters and escaping callable copies.

use crate::support::{compile_and_run_tagged, compile_and_run_with_heap_debug};

/// Asserts destructor ordering, balanced descriptor captures and equivalent tagged behavior.
fn assert_callable_property_output(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Replacing a closure property and destroying its owner must preserve independently copied closures.
#[test]
fn test_core_callable_property_temporary_replacement_retires_captured_owners() {
    assert_callable_property_output(r#"<?php
class CallablePropertyPayload {
    public int $number;
    public function __construct(int $number) { $this->number = $number; }
    public function __destruct() { echo "drop", $this->number, "|"; }
}
class CallablePropertyOwner {
    public $callback;
    public function install(int $number): void {
        $payload = new CallablePropertyPayload($number);
        $this->callback = static fn(): int => $payload->number;
    }
    public function borrow(callable $callback): void { $this->callback = $callback; }
}
for ($i = 0; $i < 3; $i++) {
    $owner = new CallablePropertyOwner();
    $owner->install(1);
    $first = $owner->callback;
    $owner->install(2);
    echo $first(), "|";
    unset($first);
    $second = $owner->callback;
    $owner->borrow($second);
    unset($owner);
    echo $second(), "|";
    unset($second);
}
"#, &"1|drop1|2|drop2|".repeat(3));
}

/// First-class method descriptors stored in properties own their bound receiver after local unset.
#[test]
fn test_core_callable_property_first_class_temporary_keeps_receiver_until_last_copy() {
    assert_callable_property_output(r#"<?php
class CallablePropertyMethodTarget {
    public function read(): int { return 42; }
    public function __destruct() { echo "drop|"; }
}
class CallablePropertyMethodOwner { public $callback; }
for ($i = 0; $i < 3; $i++) {
    $owner = new CallablePropertyMethodOwner();
    $target = new CallablePropertyMethodTarget();
    $owner->callback = $target->read(...);
    unset($target);
    $copy = $owner->callback;
    unset($owner);
    echo $copy(), "|";
    unset($copy);
}
"#, &"42|drop|".repeat(3));
}
