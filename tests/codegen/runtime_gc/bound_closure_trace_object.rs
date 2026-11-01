//! Purpose:
//! Regression tests for the ownership of a bound closure's `$this` receiver when a backtrace
//! materializes the suspended frame.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - `debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT)` records the frame's `$this` in the trace's
//!   frame hash. That `$this` is the closure descriptor's Mixed capture, borrowed by the frame;
//!   the trace must own a clone, not the capture itself. Before the fix, releasing the trace
//!   decref'd the descriptor's capture, so the next invocation of the still-live closure read
//!   freed memory and heap debug reported free-list corruption. The loop invokes the closure
//!   after the first trace has been released, which is the observable regression.

use crate::support::*;

/// Verifies a bound closure survives release of a trace that recorded its `$this`.
#[test]
fn test_bound_closure_trace_object_keeps_this_capture_alive() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class B {
    public string $tag = 'ok';
    public function make(): \Closure {
        return (function (): string {
            $frames = debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT, 3);
            $seen = isset($frames[0]['object']) ? 'obj' : 'none';
            return 'ok:' . $this->tag . ':' . $seen;
        })->bindTo($this);
    }
}
$b = new B();
$c = $b->make();
for ($i = 0; $i < 50; $i++) { $out = $c(); }
echo $out, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok:ok:obj\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
