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

/// Verifies that an explicitly closed stream does not crash when the scope
/// cleanup kind-1 path calls close() on the already-closed fd (EBADF harmless).
#[test]
fn test_stream_explicit_close_then_scope_exit() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://temp", "w+");
fwrite($f, "test");
fclose($f);
echo "ok\n";
// $f leaves scope with a closed fd — close() again is a harmless no-op.
"#,
    );
    assert_eq!(out, "ok\n");
}