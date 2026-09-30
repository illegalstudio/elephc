//! Purpose:
//! Tests for resource scope-cleanup: auto-free of Mixed-boxed resources when
//! their owning variable leaves scope without explicit close/finalize.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each test compiles inline PHP and asserts stdout.
//! - Tests verify that unfinalized HashContext handles and unclosed native
//!   stream fds are cleaned up without crashing or double-freeing.

use crate::support::*;

/// Verifies that a HashContext never finalized is auto-freed at scope exit
/// through `__rt_mixed_free_deep` tag-9 kind-2 → `__rt_hash_ctx_free`.
#[test]
fn test_hash_context_auto_freed_on_scope_exit() {
    let out = compile_and_run(
        r#"<?php
$ctx = hash_init("sha256");
hash_update($ctx, "hello");
// No hash_final() — the context should be auto-freed at scope exit.
echo "done\n";
"#,
    );
    assert_eq!(out, "done\n");
}

/// Verifies that an explicitly finalized context still works and does not
/// crash when the Mixed box is later released (double-free guard).
#[test]
fn test_hash_context_explicit_final_then_scope_exit() {
    let out = compile_and_run(
        r#"<?php
$ctx = hash_init("sha256");
hash_update($ctx, "hello");
echo hash_final($ctx);
echo "\n";
"#,
    );
    // sha256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
    assert_eq!(
        out,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\n"
    );
}

/// Verifies that a HashContext local in a function is auto-freed at return
/// via epilogue cleanup (Mixed box decref → __rt_mixed_free_deep → kind-2).
#[test]
fn test_hash_context_in_function_auto_freed() {
    let out = compile_and_run(
        r#"<?php
function leak_ctx(): void {
    $ctx = hash_init("md5");
    hash_update($ctx, "data");
    // No hash_final — auto-freed when $ctx goes out of scope at return.
}
leak_ctx();
echo "ok\n";
"#,
    );
    assert_eq!(out, "ok\n");
}

/// Verifies that aliasing a HashContext ($b = $a) does not double-free: the
/// Mixed box refcount keeps both aliases alive, only the last release frees.
#[test]
fn test_hash_context_alias_no_double_free() {
    let out = compile_and_run(
        r#"<?php
$a = hash_init("sha256");
$b = $a;  // alias — incref the Mixed box
hash_update($a, "x");
echo "survived\n";
// Both leave scope; the box is decref'd twice but freed once.
"#,
    );
    assert_eq!(out, "survived\n");
}

/// Verifies that an fopen'd native fd that is never fclose'd is auto-closed
/// by the kind-1 destructor in `__rt_mixed_free_deep` at scope exit.
#[test]
fn test_native_stream_auto_closed_on_scope_exit() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://temp", "w+");
fwrite($f, "auto-close test");
// No fclose — the fd should be auto-closed at scope exit.
echo "done\n";
"#,
    );
    assert_eq!(out, "done\n");
}

/// Verifies that an explicitly closed stream does not crash and is skipped by
/// scope cleanup: `fclose()` transitions the registry entry to Closed, so the
/// final Mixed release cannot close the backend descriptor a second time.
#[test]
fn test_stream_explicit_close_then_scope_exit() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://temp", "w+");
fwrite($f, "test");
fclose($f);
echo "ok\n";
// $f leaves scope with a Closed registry handle — scope cleanup skips it.
"#,
    );
    assert_eq!(out, "ok\n");
}

/// Verifies that finalizing an already-finalized HashContext raises PHP's
/// `TypeError` instead of silently digesting again, and that the rejection is
/// memory-safe: the guard reads the finalized flag BEFORE touching the context,
/// the original handle is still owned by scope cleanup, and the caught throw
/// leaves the program running so that cleanup actually happens.
///
/// This used to pin the opposite behaviour — both finals returning the same
/// digest — as a documented divergence. PHP throws, so the divergence is gone
/// and the expectation moved with it.
#[test]
fn test_hash_context_double_final_rejected_and_memory_safe() {
    let out = compile_and_run(
        r#"<?php
$ctx = hash_init("sha256");
hash_update($ctx, "hello");
$a = hash_final($ctx);
try {
    hash_final($ctx);
    echo "no-throw\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo strlen($a), "\n";
"#,
    );
    assert_eq!(
        out,
        "hash_final(): Argument #1 ($context) must be a valid, non-finalized HashContext\n64\n"
    );
}

/// Verifies that updating a HashContext after `hash_final()` raises PHP's
/// `TypeError` rather than quietly accumulating more data into the still-live
/// context — the silently-wrong-digest shape this guard exists to kill.
///
/// The `hash("sha256", "ab")` comparison this test used to make is kept as the
/// positive control, computed the legitimate way, so the suite still proves the
/// digest path itself was not broken by the guard.
#[test]
fn test_hash_context_update_after_final_rejected_and_memory_safe() {
    let out = compile_and_run(
        r#"<?php
$ctx = hash_init("sha256");
hash_update($ctx, "a");
hash_final($ctx);
try {
    hash_update($ctx, "b");
    echo "no-throw\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
$fresh = hash_init("sha256");
hash_update($fresh, "ab");
echo (hash_final($fresh) === hash("sha256", "ab")) ? "ok\n" : "bad\n";
"#,
    );
    assert_eq!(
        out,
        "hash_update(): Argument #1 ($context) must be a valid, non-finalized HashContext\nok\n"
    );
}

/// Verifies that a `popen()` pipe never `pclose`d is auto-released at scope exit
/// through `__rt_mixed_free_deep` tag-9 kind-3 → `__rt_pclose` (which closes the
/// FILE* and reaps the child) without crashing.
#[test]
fn test_popen_auto_closed_on_scope_exit() {
    let out = compile_and_run(
        r#"<?php
$p = popen("printf abc", "r");
echo fread($p, 16);
// No pclose() — the pipe is auto-closed and the child reaped at scope exit.
echo "|done\n";
"#,
    );
    assert_eq!(out, "abc|done\n");
}

/// Verifies that an explicitly `pclose`d pipe is skipped by scope cleanup (the
/// Closed registry state remains observable) so the child is not reaped or closed twice.
#[test]
fn test_popen_explicit_pclose_then_scope_exit() {
    let out = compile_and_run(
        r#"<?php
$p = popen("printf xyz", "r");
echo fread($p, 16);
echo "|";
echo pclose($p);
echo "\n";
// $p leaves scope with a Closed registry handle — cleanup does not pclose again.
"#,
    );
    assert_eq!(out, "xyz|0\n");
}

/// Verifies that an `opendir()` stream never `closedir`d is auto-released at
/// scope exit through tag-9 kind-4 → `__rt_closedir` without crashing.
#[test]
fn test_opendir_auto_closed_on_scope_exit() {
    let out = compile_and_run(
        r#"<?php
mkdir("d");
file_put_contents("d/a.txt", "x");
$h = opendir("d");
readdir($h);
// No closedir() — the directory stream is auto-closed at scope exit.
echo "done\n";
"#,
    );
    assert_eq!(out, "done\n");
}

/// Verifies that an explicitly `closedir`d stream is skipped by scope cleanup
/// (Closed registry state) so `closedir` does not run twice.
#[test]
fn test_opendir_explicit_closedir_then_scope_exit() {
    let out = compile_and_run(
        r#"<?php
mkdir("d");
$h = opendir("d");
closedir($h);
echo "ok\n";
// $h leaves scope with a Closed registry handle — cleanup does not closedir again.
"#,
    );
    assert_eq!(out, "ok\n");
}

/// Verifies that closing a stream and opening another (which may reuse the same
/// fd number) before scope exit is safe: the closed stream's registry
/// generation cannot resolve to or close the reused descriptor.
#[test]
fn test_stream_fd_reuse_after_close_is_safe() {
    let out = compile_and_run(
        r#"<?php
$a = fopen("php://temp", "w+");
fclose($a);            // $a's registry entry becomes Closed
$b = fopen("php://temp", "w+"); // may reuse $a's old fd number
fwrite($b, "reused");
rewind($b);
echo fread($b, 16);
echo "\n";
// Both leave scope: stale $a is skipped, while $b is closed exactly once.
"#,
    );
    assert_eq!(out, "reused\n");
}
