//! Purpose:
//! Runs internal capture stores against compiled PHP destructors and the real native exception runtime.
//!
//! Called from:
//! - Focused runtime-GC codegen tests before public capture-reference adapters are registered.
//!
//! Key details:
//! - Typed PHP helper bodies are replaced by native test shims using a borrowed hash argument.
//! - Replaced helpers retain their generated exceptional-cleanup callbacks and owner steps.
//! - The shim creates descriptors, calls the real store, and propagates a returned pending throwable.
//! - Compilation, object destruction, deep release, exception handling, and heap checks remain real.

use crate::support::*;
use elephc::codegen_support::platform::{Arch, Platform};

#[path = "mbstring_capture_hash/snapshot.rs"]
mod snapshot;

#[path = "mbstring_capture_hash/reference_fill.rs"]
mod reference_fill;

#[path = "mbstring_capture_hash/reference_begin.rs"]
pub(in crate::codegen::runtime_gc) mod reference_begin;

#[path = "mbstring_capture_hash/query_remove.rs"]
mod query_remove;

#[path = "mbstring_capture_hash/query_enter.rs"]
mod query_enter;

#[path = "mbstring_capture_hash/query_register.rs"]
mod query_register;

#[path = "mbstring_capture_hash/query_invoke.rs"]
mod query_invoke;

#[path = "mbstring_capture_hash/ini_materialize.rs"]
mod ini_materialize;

/// Completes object and callable-capture replacement before destructor throws, with no retained owners.
#[test]
fn test_mbstring_capture_hash_native_destructor_boundary() {
    for count in [1, 8] {
        let source = format!(r#"<?php
function capture_test_store(array $matches): int {{ return count($matches); }}
function show_capture(array $matches): void {{ echo $matches["key"], "\n"; }}
class NativeCaptureOldValue {{
    public function __construct(public bool $fail) {{}}
    public function __destruct() {{ echo "destroy\n"; if ($this->fail) {{ throw new RuntimeException("capture cleanup"); }} }}
}}
function run_capture(bool $fail): void {{
    $matches = ["key" => new NativeCaptureOldValue($fail)];
    try {{ $status = capture_test_store($matches); echo "status:", $status, "\n"; }}
    catch (Throwable $error) {{ echo "caught:", $error->getMessage(), "\n"; }}
    show_capture($matches);
}}
function run_capture_closure(bool $fail): void {{
    $old = new NativeCaptureOldValue($fail);
    $matches = ["key" => function() use ($old): void {{}}];
    unset($old);
    try {{ $status = capture_test_store($matches); echo "status:", $status, "\n"; }}
    catch (Throwable $error) {{ echo "caught:", $error->getMessage(), "\n"; }}
    show_capture($matches);
}}
mb_strlen("");
for ($i = 0; $i < {count}; $i++) {{
    run_capture(false); run_capture(true);
    run_capture_closure(false); run_capture_closure(true);
}}
echo "done\n";
"#);
        let directory = make_cli_test_dir("mbstring_capture_native_boundary");
        let (assembly, runtime, libraries) = compile_source_to_asm_with_options(&source, &directory, 8_388_608, true, true);
        let patched = install_store_shim(&assembly);
        let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
            &libraries, &default_link_paths(), &[]);
        let _ = std::fs::remove_dir_all(directory);
        assert!(output.success, "{}\n{}", output.stdout, output.stderr);
        let expected = "destroy\nstatus:0\ncaptured\ndestroy\ncaught:capture cleanup\ncaptured\n".repeat(count * 2);
        assert_eq!(output.stdout, format!("{expected}done\n"));
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    }
}

/// Runs conversion, set, unset, and nested construction during ordinary and throwing PHP destructors.
#[test]
fn test_mbstring_capture_hash_native_reentrant_destructor_boundary() {
    assert_reentry_boundary(false);
    assert_reentry_boundary(true);
}

/// Checks raw and preboxed capture slots across conversion, mutation, pending throws, and heap cleanup.
fn assert_reentry_boundary(initially_boxed: bool) {
    let source = r#"<?php
function capture_test_store(array $matches): int { return count($matches); }
function capture_test_rewrite(int &$mode): int { return $mode; }
function capture_test_take_snapshot(int &$mode): NativeCaptureReentrantValue {
    return new NativeCaptureReentrantValue($mode, false);
}
function show_capture(array $matches): void { echo $matches["key"], "\n"; }
class NativeCaptureReentrantValue {
    public function __construct(public int $mode, public bool $fail) {}
    public function __destruct() {
        echo "destroy:", $this->mode, "\n";
        $mode = $this->mode;
        echo "rewrite:", capture_test_rewrite($mode), "\n";
        if ($this->fail) { throw new RuntimeException("reentry cleanup"); }
    }
}
function run_capture(int $mode, bool $fail): void {
    $matches = ["key" => new NativeCaptureReentrantValue($mode, $fail)];
    try { $status = capture_test_store($matches); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    show_capture($matches);
    if ($mode == 8) {
        $copy = capture_test_take_snapshot($mode);
        echo "snapshot:", $copy->mode, "\n";
        unset($copy);
    }
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) {
    for ($mode = 0; $mode < 9; $mode++) { run_capture($mode, false); run_capture($mode, true); }
}
echo "done\n";
"#;
    let directory = make_cli_test_dir("mbstring_capture_native_reentry");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let call = if target().arch == Arch::AArch64 { "bl" } else { "call" };
    for helper in ["capture_test_rewrite", "capture_test_take_snapshot"] {
        let symbol = function_symbol(&assembly, helper);
        assert!(assembly.lines().any(|line| line.trim().starts_with(&format!("{call} {symbol}"))),
            "{helper} must remain a real call for the native shim");
    }
    let store = function_symbol(&assembly, "capture_test_store");
    let (load_selected, publish_selected, common) = hash_slot_assembly("_capture_test_selected");
    let (load_snapshot, publish_snapshot, snapshot_common) = hash_slot_assembly("_capture_test_snapshot_hash");
    let patched = install_store_shim(&assembly);
    let prefix = format!("{store}:\n");
    let prepare = if initially_boxed { prepare_mixed_shim() } else { "" };
    let patched = patched.replacen(&prefix, &format!("{prefix}{prepare}{publish_selected}\n"), 1);
    let rewrite = format!("{}{}", reentry_shim(&load_selected, &store),
        snapshot::copy_shim(&load_selected, &publish_snapshot));
    let patched = replace_function(&patched, "capture_test_rewrite", &rewrite);
    let take = snapshot::take_shim(&load_snapshot, &publish_snapshot);
    let patched = format!("{}{common}\n{snapshot_common}\n",
        replace_function(&patched, "capture_test_take_snapshot", &take));
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    let _ = std::fs::remove_dir_all(directory);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    let mut expected = String::new();
    for _ in 0..8 {
        for mode in 0..9 {
            expected.push_str(&format!("destroy:{mode}\nrewrite:0\nstatus:0\ncaptured\n"));
            if mode == 8 { expected.push_str("snapshot:8\n"); }
            expected.push_str(&format!("destroy:{mode}\nrewrite:0\ncaught:reentry cleanup\ncaptured\n"));
            if mode == 8 { expected.push_str("snapshot:8\n"); }
        }
    }
    expected.push_str("done\n");
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Boxes the test input before capture starts, using the same native argument and return conventions.
fn prepare_mixed_shim() -> &'static str {
    if target().arch == Arch::AArch64 {
        "    stp x29, x30, [sp, #-16]!\n    bl __rt_hash_to_mixed\n    ldp x29, x30, [sp], #16\n"
    } else {
        "    sub rsp, 8\n    call __rt_hash_to_mixed\n    add rsp, 8\n    mov rdi, rax\n"
    }
}

/// Addresses test-only hash storage with the target's data relocations and alignment.
fn hash_slot_assembly(symbol: &str) -> (String, String, String) {
    if target().arch == Arch::X86_64 {
        return (format!("mov rdi, QWORD PTR [rip + {symbol}]"),
            format!("mov QWORD PTR [rip + {symbol}], rdi"),
            format!(".comm {symbol},8,8"));
    }
    let (address, alignment) = if target().platform == Platform::Linux {
        (format!("adrp x9, {symbol}\nadd x9, x9, :lo12:{symbol}"), 8)
    } else {
        (format!("adrp x9, {symbol}@PAGE\nadd x9, x9, {symbol}@PAGEOFF"), 3)
    };
    (format!("{address}\nldr x0, [x9]"), format!("{address}\nstr x0, [x9]"),
        format!(".comm {symbol},8,{alignment}"))
}

/// Converts borrowed entries or invokes real hash mutations while PHP retains the destructor frame.
fn reentry_shim(load_selected: &str, store: &str) -> String {
    if target().arch == Arch::AArch64 {
        return format!(r#"
    ldr x0, [x0]
    cmp x0, #7
    b.ge .L_capture_test_copy
    cmp x0, #3
    b.lt .L_capture_test_dispatch
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x0, [sp]
    {load_selected}
    bl __rt_hash_to_mixed
    ldr x0, [sp]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    cmp x0, #3
    b.ne .L_capture_test_convert_mutation
    mov x0, #0
    ret
.L_capture_test_convert_mutation:
    sub x0, x0, #4
.L_capture_test_dispatch:
    cmp x0, #2
    b.eq .L_capture_test_nested
    sub sp, sp, #64
    stp x29, x30, [sp, #48]
    str x0, [sp, #8]
    {load_selected}
    str x0, [sp]
    mov x9, #0x656b
    movk x9, #0x79, lsl #16
    str x9, [sp, #16]
    ldr x9, [sp, #8]
    cbnz x9, .L_capture_test_unset
    mov x9, #0x6562
    movk x9, #0x6f66, lsl #16
    movk x9, #0x6572, lsl #32
    str x9, [sp, #24]
    add x1, sp, #24
    mov x2, #6
    bl __rt_str_persist
    mov x3, x1
    mov x4, x2
    mov x5, #1
    ldr x0, [sp]
    add x1, sp, #16
    mov x2, #3
    bl __rt_hash_set
    b .L_capture_test_rewrite_done
.L_capture_test_unset:
    ldr x0, [sp]
    add x1, sp, #16
    mov x2, #3
    bl __rt_hash_unset
.L_capture_test_rewrite_done:
    ldp x29, x30, [sp, #48]
    add sp, sp, #64
    mov x0, #0
    ret
.L_capture_test_nested:
    {load_selected}
    b {store}
"#);
    }
    format!(r#"
    mov rdi, QWORD PTR [rdi]
    cmp rdi, 7
    jge .L_capture_test_copy
    cmp rdi, 3
    jl .L_capture_test_dispatch
    sub rsp, 24
    mov QWORD PTR [rsp], rdi
    {load_selected}
    call __rt_hash_to_mixed
    mov rdi, QWORD PTR [rsp]
    add rsp, 24
    cmp rdi, 3
    jne .L_capture_test_convert_mutation
    xor eax, eax
    ret
.L_capture_test_convert_mutation:
    sub rdi, 4
.L_capture_test_dispatch:
    cmp rdi, 2
    je .L_capture_test_nested
    push rbp
    mov rbp, rsp
    sub rsp, 32
    mov QWORD PTR [rsp + 8], rdi
    {load_selected}
    mov QWORD PTR [rsp], rdi
    mov QWORD PTR [rsp + 16], 0x79656b
    cmp QWORD PTR [rsp + 8], 0
    jne .L_capture_test_unset
    mov r10, 0x65726f666562
    mov QWORD PTR [rsp + 24], r10
    lea rax, [rsp + 24]
    mov edx, 6
    call __rt_str_persist
    mov rcx, rax
    mov r8, rdx
    mov r9d, 1
    mov rdi, QWORD PTR [rsp]
    lea rsi, [rsp + 16]
    mov edx, 3
    call __rt_hash_set
    jmp .L_capture_test_rewrite_done
.L_capture_test_unset:
    mov rdi, QWORD PTR [rsp]
    lea rsi, [rsp + 16]
    mov edx, 3
    call __rt_hash_unset
.L_capture_test_rewrite_done:
    add rsp, 32
    pop rbp
    xor eax, eax
    ret
.L_capture_test_nested:
    {load_selected}
    jmp {store}
"#)
}

/// Reads the emitted symbol from function metadata instead of duplicating PHP name mangling.
fn function_symbol(assembly: &str, name: &str) -> String {
    let marker = format!("@fn name={name} symbol=");
    let start = assembly.find(&marker)
        .unwrap_or_else(|| panic!("typed capture test function {name} was not emitted")) + marker.len();
    assembly[start..].lines().next().unwrap().trim().to_owned()
}

/// Replaces exactly one marked helper body while preserving its generated cleanup callback.
///
/// Executable PHP functions publish the callback symbol from assembly directives outside their
/// marked body. Keeping the callback suffix, including its owner-step labels, leaves those symbols
/// defined after the native fixture shim replaces the PHP entry point.
pub(in crate::codegen::runtime_gc) fn replace_function(assembly: &str, name: &str, body: &str) -> String {
    let marker = format!("@fn name={name} symbol=");
    let marker_start = assembly.find(&marker).expect("typed capture test function was not emitted");
    let start = assembly[..marker_start].rfind('\n').map_or(0, |offset| offset + 1);
    let symbol = function_symbol(assembly, name);
    let end_marker = format!("@endfn name={name}");
    let end_start = assembly[marker_start..].find(&end_marker).unwrap() + marker_start;
    let end = assembly[end_start..].find('\n').map_or(assembly.len(), |offset| end_start + offset + 1);
    let cleanup_marker = "# exceptional PHP frame cleanup callback";
    let cleanup_start = assembly[marker_start..end_start]
        .find(cleanup_marker)
        .map(|offset| marker_start + offset)
        .and_then(|offset| assembly[..offset].rfind('\n').map(|line| line + 1));
    let cleanup = cleanup_start.map_or("", |offset| &assembly[offset..end_start]);
    format!(
        "{}.text\n.globl {symbol}\n{symbol}:\n{body}{cleanup}{}",
        &assembly[..start],
        &assembly[end..]
    )
}

/// Replaces only the marked test function, leaving actual PHP classes and ownership lowering intact.
fn install_store_shim(assembly: &str) -> String {
    // The frame contains two borrowed descriptors and inline "key"/"captured" bytes.
    // Both variants borrow the PHP hash, then propagate status two after frame teardown.
    let done = format!(
        "{}capture_store_done",
        target().platform.local_label_prefix()
    );
    let body = if target().arch == Arch::AArch64 {
        format!(r#"
    sub sp, sp, #80
    stp x29, x30, [sp, #64]
    mov x1, x0
    mov x9, #1
    str x9, [sp]
    str x9, [sp, #24]
    mov x9, #3
    str x9, [sp, #16]
    mov x9, #8
    str x9, [sp, #40]
    mov x9, #0x656b
    movk x9, #0x79, lsl #16
    str x9, [sp, #48]
    mov x9, #0x6163
    movk x9, #0x7470, lsl #16
    movk x9, #0x7275, lsl #32
    movk x9, #0x6465, lsl #48
    str x9, [sp, #56]
    add x9, sp, #48
    str x9, [sp, #8]
    add x9, sp, #56
    str x9, [sp, #32]
    mov x0, #0
    mov x2, sp
    add x3, sp, #24
    bl __rt_mbstring_capture_hash_store
    ldp x29, x30, [sp, #64]
    add sp, sp, #80
    cmp x0, #2
    b.ne {done}
    b __rt_throw_current
{done}:
    ret
"#)
    } else {
        format!(r#"
    push rbp
    mov rbp, rsp
    sub rsp, 64
    mov rsi, rdi
    mov QWORD PTR [rsp], 1
    mov QWORD PTR [rsp + 16], 3
    mov QWORD PTR [rsp + 24], 1
    mov QWORD PTR [rsp + 40], 8
    mov QWORD PTR [rsp + 48], 0x79656b
    mov r10, 0x6465727574706163
    mov QWORD PTR [rsp + 56], r10
    lea r10, [rsp + 48]
    mov QWORD PTR [rsp + 8], r10
    lea r10, [rsp + 56]
    mov QWORD PTR [rsp + 32], r10
    xor edi, edi
    mov rdx, rsp
    lea rcx, [rsp + 24]
    call __rt_mbstring_capture_hash_store
    add rsp, 64
    pop rbp
    cmp eax, 2
    jne {done}
    jmp __rt_throw_current
{done}:
    ret
"#)
    };
    replace_function(assembly, "capture_test_store", &body)
}
