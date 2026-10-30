//! Purpose:
//! Regression tests for owned keys passed to throwing boxed ArrayAccess offsetUnset methods.
//!
//! Called from:
//! - The runtime-GC codegen integration suite.
//!
//! Key details:
//! - Native dispatch borrows the PHP key but owns its staging box across the method call.
//! - Fresh strings and retained Mixed keys exercise both caller-side boxing paths.

use crate::support::*;

/// A throwing PHP offsetUnset method must release fresh boxed strings and retained Mixed keys.
#[test]
fn test_throwing_boxed_offset_unset_releases_original_key_owners() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class ThrowingKeyBag implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {
        echo gettype($key), ":", $key, "|";
        throw new RuntimeException("stop");
    }
}
class ThrowingKeyHolder { public mixed $bag; }
function original_boxed_key(int $n): mixed { return "m" . $n; }
$holder = new ThrowingKeyHolder();
$holder->bag = new ThrowingKeyBag();
for ($i = 0; $i < 20; $i++) {
    try { unset($holder->bag[str_repeat("k", 4) . $argc]); }
    catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
    try { unset($holder->bag[original_boxed_key($argc)]); }
    catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
}
unset($holder);
"#);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "string:kkkk1|stop|string:m1|stop|".repeat(20));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
