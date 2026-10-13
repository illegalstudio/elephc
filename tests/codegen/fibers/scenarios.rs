//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of fibers scenarios, including fiber php constructs inside body, fiber canonical php doc example, and fiber closure capture string survives suspend.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies a suspended Fiber's recursion accounting is isolated from the main
/// context. Each traversal stays below the 4 MiB guard, while their combined
/// live-frame charge exceeds it when a context switch leaks the Fiber budget.
///
/// The fixture compiles IN-PROCESS (the `compile_and_run` harness drives the
/// elephc library directly), and a function with 128 live locals pushes the
/// compiler's recursive passes past the 2 MiB default test-thread stack — the
/// same Span-width constraint AGENTS.md documents. CI already raises
/// `RUST_MIN_STACK` to 32 MiB, but a bare local `cargo test` run must not
/// depend on that environment variable, so the body runs on a dedicated
/// 32 MiB stack.
#[test]
fn test_suspended_fiber_recursion_budget_does_not_leak_to_main() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(suspended_fiber_recursion_budget_body)
        .expect("spawn the large-stack test thread")
        .join()
        .expect("the large-stack test thread must not panic");
}

/// Body of `test_suspended_fiber_recursion_budget_does_not_leak_to_main`,
/// separated so it can run on a dedicated large stack.
fn suspended_fiber_recursion_budget_body() {
    const LIVE_LOCALS: usize = 128;
    const DEPTH: usize = 10;

    let mut source = String::from(
        "<?php\nfunction descend_with_large_frame(int $depth, bool $suspend): int {\n",
    );
    for local in 0..LIVE_LOCALS {
        source.push_str(&format!("    $v{local} = $depth + {local};\n"));
    }
    let live_sum = (0..LIVE_LOCALS)
        .map(|local| format!("$v{local}"))
        .collect::<Vec<_>>()
        .join(" + ");
    source.push_str(&format!(
        r#"    if ($depth === 0) {{
        if ($suspend) {{
            Fiber::suspend("held");
        }}
        return {live_sum};
    }}
    $next = descend_with_large_frame($depth - 1, $suspend);
    return $next + {live_sum};
}}
$fiber = new Fiber(function(): void {{
    descend_with_large_frame({DEPTH}, true);
}});
$fiber->start();
unset($fiber);
$result = descend_with_large_frame({DEPTH}, false);
echo $result > 0 ? "main-ok" : "bad";
"#,
    ));

    let out = compile_and_run(&source);
    assert_eq!(out, "main-ok");
}

/// Verifies try/finally, foreach, match, and `new` work inside a fiber body across a suspend boundary.
#[test]
fn test_fiber_php_constructs_inside_body() {
    let out = compile_and_run(
        r#"<?php
class Item { public string $name = ""; }
$f = new Fiber(function(): void {
    try {
        echo "T;";
        Fiber::suspend(0);
        echo "A;";
    } finally {
        echo "F;";
    }
    $items = [10, 20, 30];
    foreach ($items as $v) { echo "v" . $v . ";"; }
    $x = 2;
    $r = match ($x) { 1 => "one", 2 => "two", default => "other" };
    echo "m" . $r . ";";
    $i = new Item();
    $i->name = "widget";
    echo "o" . $i->name;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "T;A;F;v10;v20;v30;mtwo;owidget");
}

/// Verifies the canonical PHP documentation example for fibers: suspend with a string value and resume with a different value.
#[test]
fn test_fiber_canonical_php_doc_example() {
    let out = compile_and_run(
        r#"<?php
$fiber = new Fiber(function(): void {
    $value = Fiber::suspend("fiber");
    echo "Value used to resume fiber: " . $value;
});
$value = $fiber->start();
echo "Value from fiber suspending: " . $value . "|";
$fiber->resume("test");
"#,
    );
    assert_eq!(out, "Value from fiber suspending: fiber|Value used to resume fiber: test");
}

/// Verifies closure-captured string variables survive a suspend/resume cycle.
#[test]
fn test_fiber_closure_capture_string_survives_suspend() {
    let out = compile_and_run(
        r#"<?php
$ctx = "stable";
$f = new Fiber(function() use ($ctx): void {
    Fiber::suspend(0);
    echo "after=" . $ctx;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "after=stable");
}

/// Verifies closure-captured integer variables survive a suspend/resume cycle.
#[test]
fn test_fiber_closure_capture_survives_suspend_resume() {
    let out = compile_and_run(
        r#"<?php
$base = 100;
$f = new Fiber(function() use ($base): void {
    Fiber::suspend(0);
    echo "after-resume base=" . $base;
});
$f->start();
$f->resume(0);
"#,
    );
    assert_eq!(out, "after-resume base=100");
}

/// Verifies string payloads round-trip correctly through suspend and start (no data corruption).
#[test]
fn test_fiber_string_payload_round_trip() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend("hello");
});
echo $f->start();
"#,
    );
    assert_eq!(out, "hello");
}

/// Verifies Fiber state transitions: Start → Suspended → Terminated, checking isStarted/isSuspended/isTerminated at each stage.
#[test]
fn test_fiber_state_transitions() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void { Fiber::suspend(0); });
echo $f->isStarted() ? "S" : "s";
$f->start();
echo $f->isStarted() ? "S" : "s";
echo $f->isSuspended() ? "P" : "p";
echo $f->isTerminated() ? "T" : "t";
$f->resume(0);
echo $f->isTerminated() ? "T" : "t";
"#,
    );
    assert_eq!(out, "sSPtT");
}

/// Verifies FiberError is a subclass of Error (catchable by Error, not Exception).
///
/// The FiberError is RAISED by the engine rather than constructed: reference PHP reserves the
/// class for internal use, so the original `throw new FiberError("nope")` is refused there.
/// Suspending outside a fiber is the shortest way to make the engine produce one.
#[test]
fn test_fiber_error_subclasses_error() {
    let out = compile_and_run(
        r#"<?php
try {
    Fiber::suspend(0);
} catch (Exception $e) {
    echo "exception";
} catch (Error $e) {
    echo "error";
}
"#,
    );
    assert_eq!(out, "error");
}

/// Verifies FiberError is caught by its specific type before Exception.
///
/// Engine-raised for the same reason as the test above: `new FiberError(...)` is reserved.
#[test]
fn test_fiber_error_caught_by_specific_type() {
    let out = compile_and_run(
        r#"<?php
try {
    Fiber::suspend(0);
} catch (FiberError $e) {
    echo "fiber-err";
} catch (Exception $e) {
    echo "exc";
}
"#,
    );
    assert_eq!(out, "fiber-err");
}

/// Verifies `Fiber::throw()` delivers a throwable to a fiber's internal try/catch, and
/// execution resumes after the catch block.
///
/// The delivered throwable is an `Exception` rather than a `FiberError`, which reference PHP
/// reserves for internal use. Which class is delivered is incidental here — the test is about
/// the delivery reaching the suspended fiber's handler — and the expected output is unchanged.
#[test]
fn test_fiber_throw_caught_by_internal_try_catch() {
    let out = compile_and_run(
        r#"<?php
$f = new Fiber(function(): void {
    echo "1";
    try {
        Fiber::suspend(0);
        echo "X-not-reached";
    } catch (Exception $e) {
        echo "2";
    }
    echo "3";
});
echo "A";
$f->start();
echo "B";
$f->throw(new Exception("delivered"));
echo "C";
"#,
    );
    assert_eq!(out, "A1B23C");
}
