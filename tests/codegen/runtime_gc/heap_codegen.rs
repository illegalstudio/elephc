//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of runtime GC heap codegen, including new object codegen sets heap kind, and explicit GC safe points for `unset()`.
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
    if !codegen_fixture_uses_ir_backend() {
        assert!(user_asm.contains("new Foo()"));
    }
    match target().arch {
        Arch::AArch64 => assert!(user_asm.contains("str x9, [x0, #-8]"), "{user_asm}"),
        Arch::X86_64 => assert!(user_asm.contains("mov QWORD PTR [rax - 8], r10"), "{user_asm}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Returns the generated assembly body for one runtime helper label.
fn asm_function<'a>(asm: &'a str, label: &str) -> &'a str {
    let marker = format!("{label}:");
    let start = asm
        .find(&marker)
        .unwrap_or_else(|| panic!("missing assembly label {label}"));
    let rest = &asm[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    &rest[..end]
}

/// Pins the x86_64 argument register of `__rt_incref` at EVERY call site in the runtime.
///
/// `__rt_incref` does not follow SysV on x86_64: its body opens with `test rax, rax`, so it reads
/// the pointer to retain from `rax`, never from `rdi`. A call site that loads the pointer into
/// `rdi` instead assembles and links perfectly, retains whatever happened to be left in `rax`
/// (usually the result hash the helper is building), and leaves the real payload unretained — the
/// element is then freed with the source and read back as reused heap memory. That is silent on
/// aarch64, where the same emitters use `x0` for both, which is why this scans the GENERATED
/// linux-x86_64 runtime rather than any single emitter.
///
/// The gate is the instruction immediately before each `call __rt_incref`: it may not write `rdi`.
#[test]
fn every_x86_64_incref_call_site_passes_its_pointer_in_rax() {
    let target = Target::parse("linux-x86_64").expect("linux-x86_64 is a supported target");
    let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target);

    let mut previous_instruction = "";
    let mut offenders: Vec<String> = Vec::new();
    for line in runtime_asm.lines() {
        let instruction = line.trim();
        if instruction.is_empty() || instruction.starts_with('#') || instruction.ends_with(':') {
            continue;
        }
        if instruction == "call __rt_incref"
            && (previous_instruction.starts_with("mov rdi,")
                || previous_instruction.starts_with("lea rdi,"))
        {
            offenders.push(previous_instruction.to_string());
        }
        previous_instruction = instruction;
    }

    assert!(
        offenders.is_empty(),
        "{} x86_64 call site(s) hand __rt_incref its pointer in rdi, which it never reads: {:?}",
        offenders.len(),
        offenders
    );
}

/// Verifies `unset()` emits an explicit GC safe point outside hash decref.
///
/// Compiles a PHP snippet that creates a flat string-keyed array `["a" => 1, "b" => 2]` and then
/// `unset`s it. The user assembly should call the cycle collector after the PHP-visible
/// root has been removed, while the runtime hash decref helper should only decrement RC
/// and free at zero.
///
/// This is a regression guard against reintroducing collection from generic release paths.
#[test]
fn test_unset_codegen_uses_explicit_gc_safe_point() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, _) = compile_source_to_asm_with_options(
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
    let decref_hash = asm_function(&runtime_asm, "__rt_decref_hash");
    match target().arch {
        Arch::AArch64 => {
            assert!(
                user_asm.contains("bl __rt_gc_collect_cycles"),
                "unset missing explicit GC safe point: {user_asm}"
            );
            assert!(
                !decref_hash.contains("bl __rt_gc_collect_cycles"),
                "hash decref must not collect from a generic release path: {decref_hash}"
            );
            assert!(
                !decref_hash.contains("bl __rt_hash_may_have_cyclic_values"),
                "hash decref must not pre-scan for cycle collection: {decref_hash}"
            );
        }
        Arch::X86_64 => {
            assert!(
                user_asm.contains("call __rt_gc_collect_cycles"),
                "unset missing explicit GC safe point: {user_asm}"
            );
            assert!(
                !decref_hash.contains("call __rt_gc_collect_cycles"),
                "hash decref must not collect from a generic release path: {decref_hash}"
            );
            assert!(
                !decref_hash.contains("call __rt_hash_may_have_cyclic_values"),
                "hash decref must not pre-scan for cycle collection: {decref_hash}"
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}
