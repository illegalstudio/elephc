//! Purpose:
//! Checks independent transfer owners across nullable Fiber dispatch and failed state checks.
//!
//! Called from:
//! - Focused runtime GC integration tests in codegen_tests.
//!
//! Key details:
//! - Both optimizer modes verify output and a clean heap after normal and exceptional calls.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Caller argument cleanup never retires the next yielded cell, including after a failed resume.
#[test]
fn test_fiber_transfer_owners_survive_nullable_dispatch_and_errors() {
    let check = || {
        let output = compile_and_run_with_heap_debug(r#"<?php
class Holder { public ?Fiber $fiber = null; }
$holder = new Holder();
$holder->fiber = new Fiber(function (): void {
    for ($i = 1; $i < 4; $i++) { Fiber::suspend($i); }
});
echo $holder->fiber->start();
echo $holder->fiber->resume();
echo $holder->fiber->resume();
$holder->fiber->resume();
try { $holder->fiber->resume(); } catch (FiberError) { echo ':resume'; }
unset($holder);
try { Fiber::suspend(str_repeat('temporary', 32)); } catch (FiberError) { echo ':suspend'; }
"#);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "123:resume:suspend");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    };
    check();
    without_ir_opt(check);
}
