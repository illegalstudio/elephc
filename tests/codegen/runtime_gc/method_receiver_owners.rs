//! Purpose:
//! Checks method receiver leases across argument evaluation and callee exceptions.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Temporary receivers retire before catches without consuming a caller's boxed local.
//! - Nullable dispatch shares the ordinary method receiver lifetime.

use crate::support::*;

/// Throwing arguments and callees retire receiver temporaries and owned Mixed projections.
#[test]
fn test_method_receiver_owners_survive_caught_exceptions() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class MethodReceiverOwner {
    public static int $destroyed = 0;
    public function __destruct() { self::$destroyed++; }
    public function fail(int $ignored = 0): void { throw new RuntimeException('callee'); }
}
function methodReceiverArgument(): int { throw new RuntimeException('argument'); }
function invokeBoxedReceiver(mixed $receiver): void {
    if ($receiver instanceof MethodReceiverOwner) {
        try { $receiver->fail(); } catch (RuntimeException $error) { echo 'B'; }
    }
}
function invokeNullableReceiver(?MethodReceiverOwner $receiver): void {
    try { $receiver?->fail(); } catch (RuntimeException $error) { echo 'N'; }
}
for ($index = 0; $index < 3; $index++) {
    try { (new MethodReceiverOwner())->fail(); }
    catch (RuntimeException $error) { echo 'T'; }
    try { (new MethodReceiverOwner())->fail(methodReceiverArgument()); }
    catch (RuntimeException $error) { echo 'A'; }
    $kept = new MethodReceiverOwner();
    invokeBoxedReceiver($kept);
    invokeNullableReceiver($kept);
    echo ':', MethodReceiverOwner::$destroyed, ':';
    unset($kept);
    echo MethodReceiverOwner::$destroyed, '|';
}
unset($error);
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "TABN:2:3|TABN:5:6|TABN:8:9|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Receiver exception guards remain balanced without EIR optimization.
#[test]
fn test_method_receiver_owners_survive_caught_exceptions_no_ir_opt() {
    without_ir_opt(test_method_receiver_owners_survive_caught_exceptions);
}
