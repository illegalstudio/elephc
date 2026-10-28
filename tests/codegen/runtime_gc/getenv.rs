//! Purpose:
//! Heap-debug regression coverage for `getenv()` whole-environment results.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime runtime_gc::getenv` through the runtime-GC suite.
//!
//! Key details:
//! - `__rt_getenv_all` allocates a hash plus persisted keys and values, then boxes
//!   the hash as Mixed. Releasing only the Mixed cell leaked the hash and every
//!   entry; each iteration must leave a clean heap.

use crate::support::*;

/// Verifies repeated no-argument `getenv()` results release the environment hash.
#[test]
fn test_getenv_all_owned_results_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$n = 0;
for ($i = 0; $i < 3; $i++) {
    $all = getenv();
    $n = count($all);
    unset($all);
}
echo $n > 0 ? "clean" : "empty";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "clean");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected getenv() results to leave a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies dynamic mode selection releases environment hashes and owned temporary names.
#[test]
fn test_getenv_nullable_owned_results_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function read_env(?string $name): mixed { return getenv($name); }
function read_dynamic_env(mixed $name): mixed { return getenv($name); }
class MissingEnvName {
    public function __toString(): string {
        global $casts;
        $casts++;
        echo "cast:";
        return str_repeat("ELEPHC_NULLABLE_ENV_ABSENT_", 2);
    }
}
$casts = 0;
$n = 0;
for ($i = 0; $i < 3; $i++) {
    $all = read_env(null);
    $n = count($all);
    unset($all);
    $missing = read_env(str_repeat("ELEPHC_NULLABLE_ENV_ABSENT_", 2));
    echo $missing === false ? "missing:" : "bad:";
    unset($missing);
    $converted = read_dynamic_env(new MissingEnvName());
    echo $converted === false ? "missing:" : "bad:";
    unset($converted);
}
echo $n > 0 ? "clean" : "empty";
echo ":", $casts;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "missing:cast:missing:missing:cast:missing:missing:cast:missing:clean:3");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "nullable getenv() leaked storage: {}", out.stderr
    );
}
