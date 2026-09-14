//! Purpose:
//! Exercises capture-reference initialization with real PHP destructors and native exception handling.
//!
//! Called from:
//! - The parent runtime-GC capture fixture's test module.
//!
//! Key details:
//! - Test shims create a persistent reference owning a fresh PHP factory result without extra value owners.
//! - A separate explicit cleanup boundary observes deferred ownership; request shutdown is not implemented here.
//! - Caller-lvalue resolution and typed-property validation remain the capture host's responsibility.

use super::*;

#[path = "reference_begin/invoke.rs"]
pub(in crate::codegen::runtime_gc) mod invoke;

/// Observes typed/untyped publication and delayed replacement cleanup even when the old destructor throws.
#[test]
fn test_mbstring_capture_reference_begin_native_destructors() {
    let source = r#"<?php
function capture_test_initialize(int &$mode): int {
    $old = capture_test_old_value($mode);
    return $old->mode;
}
function capture_test_observe(int &$mode): int { return $mode; }
function capture_test_release_root(int &$mode): int { return $mode; }
function capture_test_release_deferred(int &$mode): int { return $mode; }
function capture_test_retarget(array $replacement): int { return count($replacement); }
function capture_test_old_value(int $mode): NativeCaptureInitOldValue { return new NativeCaptureInitOldValue($mode); }
class NativeCaptureInitLateValue {
    public function __construct(public int $mode) {}
    public function __destruct() { echo "late:", $this->mode, "\n"; }
}
class NativeCaptureInitOldValue {
    public function __construct(public int $mode) {}
    public function __destruct() {
        echo "old:", $this->mode, "\n";
        $mode = $this->mode;
        echo "visible:", capture_test_observe($mode), "\n";
        if ($mode % 4 >= 2) {
            $replacement = ["late" => new NativeCaptureInitLateValue($mode)];
            echo "retarget:", capture_test_retarget($replacement), "\n";
        }
        if ($mode >= 4) { throw new RuntimeException("initialization"); }
    }
}
function run_capture(int $mode): void {
    try { $status = capture_test_initialize($mode); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo "after:", capture_test_observe($mode), "\n";
    echo "release root\n";
    $status = capture_test_release_root($mode);
    echo "root status:", $status, "\n";
    echo "release deferred\n";
    $status = capture_test_release_deferred($mode);
    echo "deferred status:", $status, "\n";
    echo "end\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) {
    for ($mode = 0; $mode < 8; $mode++) { run_capture($mode); }
}
echo "done\n";
"#;
    let directory = make_cli_test_dir("mbstring_reference_begin");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let factory = function_symbol(&assembly, "capture_test_old_value");
    let call = if target().arch == Arch::AArch64 { "bl" } else { "call" };
    for name in ["capture_test_initialize", "capture_test_observe", "capture_test_release_root",
        "capture_test_release_deferred", "capture_test_retarget"] {
        let symbol = function_symbol(&assembly, name);
        assert!(assembly.lines().any(|line| line.trim().starts_with(&format!("{call} {symbol}"))),
            "{name} must remain a real call for the native shim");
    }
    let mut patched = replace_function(&assembly, "capture_test_initialize", &begin_shim(&factory));
    patched = replace_function(&patched, "capture_test_observe", &observe_shim());
    patched = replace_function(&patched, "capture_test_retarget", &reference_fill::retarget_shim());
    patched = replace_function(&patched, "capture_test_release_root", &release_shim("_capture_test_writer"));
    patched = replace_function(&patched, "capture_test_release_deferred", &release_shim("_capture_test_discarded"));
    for name in ["_capture_test_writer", "_capture_test_discarded"] {
        patched.push_str(&format!("\n{}\n", hash_slot_assembly(name).2));
    }
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    let _ = std::fs::remove_dir_all(directory);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    let mut expected = String::new();
    for _ in 0..8 {
        for mode in 0..8 {
            let typed = mode % 2 == 1;
            let replacement = mode % 4 >= 2;
            expected.push_str(&format!("old:{mode}\nvisible:{}\n", if typed { 10 } else { 8 }));
            if replacement { expected.push_str("retarget:0\n"); }
            expected.push_str(if mode >= 4 { "caught:initialization\n" } else { "status:0\n" });
            expected.push_str(&format!("after:{}\nrelease root\n", if typed && replacement { 11 } else { 10 }));
            if typed && replacement { expected.push_str(&format!("late:{mode}\n")); }
            expected.push_str("root status:0\nrelease deferred\n");
            if !typed && replacement { expected.push_str(&format!("late:{mode}\n")); }
            expected.push_str("deferred status:0\nend\n");
        }
    }
    expected.push_str("done\n");
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Owns a fresh PHP factory result through one reference, initializes it, and releases the returned writer.
fn begin_shim(factory: &str) -> String {
    let publish = hash_slot_assembly("_capture_test_writer").1;
    let discard = hash_slot_assembly("_capture_test_discarded").1;
    if target().arch == Arch::AArch64 {
        return format!(r#"
    sub sp, sp, #80
    stp x29, x30, [sp, #64]
    ldr x0, [x0]
    str x0, [sp, #48]
    bl {factory}
    str x0, [sp, #24]
    mov x1, x0
    mov x0, #6
    mov x2, #0
    bl __rt_mixed_from_value
    str x0, [sp, #32]
    ldr x0, [sp, #24]
    bl __rt_decref_any
    ldr x1, [sp, #32]
    mov x0, #7
    mov x2, #1
    bl __rt_mixed_from_value
    str x0, [sp, #40]
    {publish}
    ldr x0, [sp, #32]
    bl __rt_decref_any
    mov x0, #0
    ldr x1, [sp, #40]
    ldr x2, [sp, #48]
    and x2, x2, #1
    mov x3, sp
    bl __rt_mbstring_capture_reference_begin
    str x0, [sp, #56]
    ldr x0, [sp, #16]
    {discard}
    ldr x0, [sp, #8]
    bl __rt_decref_any
    ldr x0, [sp, #56]
    ldp x29, x30, [sp, #64]
    add sp, sp, #80
    cmp x0, #2
    b.ne .L_capture_begin_return
    b __rt_throw_current
.L_capture_begin_return:
    ret
"#);
    }
    format!(r#"
    push rbp
    mov rbp, rsp
    sub rsp, 64
    mov rdi, QWORD PTR [rdi]
    mov QWORD PTR [rsp + 48], rdi
    call {factory}
    mov QWORD PTR [rsp + 24], rax
    mov rdi, rax
    mov eax, 6
    xor esi, esi
    call __rt_mixed_from_value
    mov QWORD PTR [rsp + 32], rax
    mov rax, QWORD PTR [rsp + 24]
    call __rt_decref_any
    mov rdi, QWORD PTR [rsp + 32]
    mov eax, 7
    mov esi, 1
    call __rt_mixed_from_value
    mov QWORD PTR [rsp + 40], rax
    mov rdi, rax
    {publish}
    mov rax, QWORD PTR [rsp + 32]
    call __rt_decref_any
    xor edi, edi
    mov rsi, QWORD PTR [rsp + 40]
    mov rdx, QWORD PTR [rsp + 48]
    and edx, 1
    mov rcx, rsp
    call __rt_mbstring_capture_reference_begin
    mov QWORD PTR [rsp + 56], rax
    mov rdi, QWORD PTR [rsp + 16]
    {discard}
    mov rax, QWORD PTR [rsp + 8]
    call __rt_decref_any
    mov rax, QWORD PTR [rsp + 56]
    leave
    cmp eax, 2
    je __rt_throw_current
    ret
"#)
}

/// Returns eight for null, ten plus an array's length, or the current non-array value tag.
fn observe_shim() -> String {
    let load = hash_slot_assembly("_capture_test_writer").0;
    if target().arch == Arch::AArch64 {
        format!(r#"
    {load}
    ldr x9, [x0, #8]
    mov x0, #8
    cbz x9, .L_capture_observe_return
    ldr x0, [x9]
    cmp x0, #5
    b.ne .L_capture_observe_return
    ldr x9, [x9, #8]
    ldr x0, [x9]
    add x0, x0, #10
.L_capture_observe_return:
    ret
"#)
    } else {
        format!(r#"
    {load}
    mov r10, QWORD PTR [rdi + 8]
    mov eax, 8
    test r10, r10
    jz .L_capture_observe_return
    mov rax, QWORD PTR [r10]
    cmp rax, 5
    jne .L_capture_observe_return
    mov r10, QWORD PTR [r10 + 8]
    mov rax, QWORD PTR [r10]
    add rax, 10
.L_capture_observe_return:
    ret
"#)
    }
}

/// Clears and consumes one fixture-owned reference or deferred value at an explicit PHP-visible boundary.
fn release_shim(symbol: &str) -> String {
    let (load, clear, _) = hash_slot_assembly(symbol);
    if target().arch == Arch::AArch64 {
        format!(r#"
    stp x29, x30, [sp, #-16]!
    {load}
    mov x10, x0
    mov x0, #0
    {clear}
    mov x0, x10
    bl __rt_decref_any
    mov x0, #0
    ldp x29, x30, [sp], #16
    ret
"#)
    } else {
        format!(r#"
    sub rsp, 8
    {load}
    mov rax, rdi
    xor edi, edi
    {clear}
    call __rt_decref_any
    xor eax, eax
    add rsp, 8
    ret
"#)
    }
}
