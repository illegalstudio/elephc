use crate::support::*;

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
