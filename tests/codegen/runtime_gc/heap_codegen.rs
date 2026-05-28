//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of runtime GC heap codegen, including new object codegen sets heap kind, and decref hash codegen skips GC for scalar only hashes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies that `new` object codegen sets the heap-kind flag in the object header.
///
/// Compiles a simple class with a public `$x = 1` property and a `new Foo()` expression.
/// Asserts the generated user assembly contains the constructor call and that the
/// heap-kind field is written on both AArch64 (`str x9, [x0, #-8]`) and x86_64
/// (`mov QWORD PTR [rax - 8], r10`).
///
/// Uses an isolated temp directory for output; directory is cleaned up after the test.
#[test]
fn test_new_object_codegen_sets_heap_kind() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, _) = compile_source_to_asm_with_options(
        "<?php class Foo { public $x = 1; } $o = new Foo();",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(user_asm.contains("new Foo()"));
    match target().arch {
        Arch::AArch64 => assert!(user_asm.contains("str x9, [x0, #-8]"), "{user_asm}"),
        Arch::X86_64 => assert!(user_asm.contains("mov QWORD PTR [rax - 8], r10"), "{user_asm}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that hash decrement codegen skips the GC cyclic-value check for scalar-only hashes.
///
/// Compiles a PHP snippet that creates a flat string-keyed array `["a" => 1, "b" => 2]` and then
/// `unset`s it. Asserts the runtime assembly contains `__rt_hash_may_have_cyclic_values`
/// and the conditional skip branch (`cbz` on AArch64 / `jz` on x86_64) that avoids a full
/// GC traversal when the hash holds only scalar values.
///
/// This is a regression guard: scalar-only hashes must not trigger the slower GC path.
#[test]
fn test_decref_hash_codegen_skips_gc_for_scalar_only_hashes() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (_user_asm, _runtime_asm, _) = compile_source_to_asm_with_options(
        r#"<?php
$map = ["a" => 1, "b" => 2];
unset($map);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target());
    assert!(
        runtime_asm.contains("__rt_hash_may_have_cyclic_values"),
        "runtime missing cyclic-value check"
    );
    match target().arch {
        Arch::AArch64 => {
            assert!(
                runtime_asm.contains("bl __rt_hash_may_have_cyclic_values"),
                "runtime missing cyclic-value call"
            );
            assert!(
                runtime_asm.contains("cbz x0, __rt_decref_hash_skip"),
                "runtime missing scalar-only skip branch"
            );
        }
        Arch::X86_64 => {
            assert!(
                runtime_asm.contains("call __rt_hash_may_have_cyclic_values"),
                "runtime missing cyclic-value call"
            );
            assert!(
                runtime_asm.contains("jz __rt_decref_hash_skip"),
                "runtime missing scalar-only skip branch"
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}
