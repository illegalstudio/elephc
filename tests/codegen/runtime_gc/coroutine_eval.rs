//! Purpose:
//! Checks coroutine cancellation through eval-to-native call boundaries.
//!
//! Called from:
//! - Focused runtime GC and exception compatibility regressions.
//!
//! Key details:
//! - Eval catches must reject internal unwind objects while still running finally.
//! - Real Throwable matching and finally control overrides remain ordinary PHP behavior.

use crate::support::*;

/// Eval cannot catch cancellation raised by a compiled function called from its try body.
#[test]
fn test_gc_coroutine_eval_catch_rejects_native_cancellation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function evalBoundaryPause(): void { Fiber::suspend(1); }
$fiber = new Fiber(function (): void {
    eval('try { evalBoundaryPause(); echo "after|"; } catch (Throwable $error) { echo "caught|"; } finally { echo "finally|"; }');
    echo 'tail|';
});
$fiber->start(); unset($fiber); gc_collect_cycles(); echo 'done';
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "finally|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native methods, real Throwable catches and finally returns keep their separate semantics.
#[test]
fn test_gc_coroutine_eval_cancellation_preserves_real_catches_and_finally_override() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class EvalBoundaryCallbacks {
    public static function pauseStatic(): void { Fiber::suspend(1); }
    public function pauseMethod(): void { Fiber::suspend(1); }
}
$static = new Fiber(static function(): void {
    eval('try { EvalBoundaryCallbacks::pauseStatic(); } catch (Throwable $error) { echo "caught-static|"; } finally { echo "static|"; }');
    echo 'static-tail|';
});
$static->start(); unset($static);
$method = new Fiber(static function(): void {
    eval('$callback = new EvalBoundaryCallbacks(); try { $callback->pauseMethod(); } catch (Throwable $error) { echo "caught-method|"; } finally { echo "method|"; }');
    echo 'method-tail|';
});
$method->start(); unset($method);
$override = new Fiber(static function(): void {
    eval('try { EvalBoundaryCallbacks::pauseStatic(); } finally { echo "override|"; return; }');
    echo 'override-tail|';
});
$override->start(); unset($override);
eval('try { throw new Exception("real"); } catch (Throwable $error) { echo $error->getMessage(), "|"; }');
eval('class EvalBoundaryException extends Exception {} try { throw new EvalBoundaryException("dynamic"); } catch (Throwable $error) { echo $error->getMessage(); }');
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "static|method|override|override-tail|real|dynamic", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
