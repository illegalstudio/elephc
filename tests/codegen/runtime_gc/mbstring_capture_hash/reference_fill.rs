//! Purpose:
//! Executes shared capture-graph filling through a retained native reference and PHP destructors.
//!
//! Called from:
//! - The parent runtime-GC capture fixture's test module.
//!
//! Key details:
//! - A typed helper's native shim owns a persistent reference to the caller's existing hash.
//! - Graph validation, ordered insertion, retargeting, destruction, pending throws, and heap accounting are real.
//! - The indexed submodule covers conversion through compiler-owned reference cells.

use super::*;
use elephc_builtin_contract::mbstring_abi::array::{ArrayGraph, Key, Value};

#[path = "reference_fill/indexed.rs"]
mod indexed;

/// Completes all capture entries after a PHP destructor throws, through the actual Rust graph ABI.
#[test]
fn test_mbstring_capture_reference_fill() {
    let source = r#"<?php
function capture_test_store(array $matches): int { return count($matches); }
function show_capture(array $matches): void {
    echo $matches[0], "|", $matches["key"], "|";
    var_export($matches[2]);
    echo "|", bin2hex($matches["bin\0key"]), "\n";
}
class NativeReferenceCaptureValue {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "destroy\n";
        if ($this->fail) { throw new RuntimeException("reference cleanup"); }
    }
}
function run_capture(bool $fail): void {
    $matches = ["key" => new NativeReferenceCaptureValue($fail)];
    try { echo "status:", capture_test_store($matches), "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    show_capture($matches);
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_capture(false); run_capture(true); }
echo "done\n";
"#;
    let graph = ArrayGraph::new(0, vec![vec![
        (Key::Int(0), Value::String(b"first".to_vec())),
        (Key::String(b"key".to_vec()), Value::String(b"captured".to_vec())),
        (Key::Int(2), Value::Bool(false)),
        (Key::String(b"bin\0key".to_vec()), Value::String(b"a\0\xffz".to_vec())),
    ]]).unwrap().encode();
    let expected = "status:destroy\n0\nfirst|captured|false|6100ff7a\nstatus:destroy\ncaught:reference cleanup\nfirst|captured|false|6100ff7a\n".repeat(8);
    assert_fill(source, &graph, &format!("{expected}done\n"), false);
}

/// Rejects an invalid later capture before the first valid entry can destroy or replace a PHP value.
#[test]
fn test_mbstring_capture_reference_fill_validates_before_mutation() {
    let source = r#"<?php
function capture_test_store(array $matches): int { return count($matches); }
class NativeInvalidCaptureValue {
    public int $value = 42;
    public function __destruct() { echo "destroy\n"; }
}
function show_original(array $matches): void { echo $matches["key"]->value, "\n"; }
function run_capture(): void {
    $matches = ["key" => new NativeInvalidCaptureValue()];
    echo "status:", capture_test_store($matches), "\n";
    show_original($matches);
    echo "before cleanup\n";
}
mb_strlen("");
run_capture();
echo "done\n";
"#;
    let graph = ArrayGraph::new(0, vec![vec![
        (Key::String(b"key".to_vec()), Value::String(b"must not replace".to_vec())),
        (Key::Int(1), Value::Bool(true)),
    ]]).unwrap().encode();
    assert_fill(source, &graph, "status:1\n42\nbefore cleanup\ndestroy\ndone\n", false);
}

/// Keeps the selected write on the original hash and sends later captures to a destructor's new hash.
#[test]
fn test_mbstring_capture_reference_fill_follows_destructor_retargeting() {
    let source = r#"<?php
function capture_test_store(array $matches): int { return count($matches); }
function capture_test_retarget(array $replacement): int { return count($replacement); }
function capture_test_take_result(array $fallback): array { return $fallback; }
function show_original(array $matches): void {
    echo "old:", count($matches), "|", $matches[0], "|", $matches["key"], "\n";
}
function show_result(array $matches): void {
    echo "new:", count($matches), "|", $matches["keep"], "|";
    var_export($matches[2]);
    echo "\n";
}
class NativeRetargetCaptureValue {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "destroy\n";
        $replacement = ["keep" => "new"];
        echo "retarget:", capture_test_retarget($replacement), "\n";
        if ($this->fail) { throw new RuntimeException("retarget cleanup"); }
    }
}
function run_capture(bool $fail): void {
    $matches = ["key" => new NativeRetargetCaptureValue($fail)];
    try { $status = capture_test_store($matches); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    show_original($matches);
    $result = capture_test_take_result($matches);
    show_result($result);
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_capture(false); run_capture(true); }
echo "done\n";
"#;
    let graph = ArrayGraph::new(0, vec![vec![
        (Key::Int(0), Value::String(b"first".to_vec())),
        (Key::String(b"key".to_vec()), Value::String(b"captured".to_vec())),
        (Key::Int(2), Value::Bool(false)),
    ]]).unwrap().encode();
    let expected = "destroy\nretarget:0\nstatus:0\nold:2|first|captured\nnew:2|new|false\ndestroy\nretarget:0\ncaught:retarget cleanup\nold:2|first|captured\nnew:2|new|false\n".repeat(8);
    assert_fill(source, &graph, &format!("{expected}done\n"), true);
}

/// Links the real bridge and native callbacks, replacing only explicitly declared fixture helpers.
fn assert_fill(source: &str, graph: &[u8], expected: &str, retarget: bool) {
    let directory = make_cli_test_dir("mbstring_reference_fill");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let mut patched = replace_function(&assembly, "capture_test_store", &fill_shim(graph.len(), retarget));
    if retarget {
        patched = replace_function(&patched, "capture_test_retarget", &retarget_shim());
        patched = replace_function(&patched, "capture_test_take_result", &take_result_shim());
        for symbol in ["_capture_test_writer", "_capture_test_result"] {
            patched.push_str(&format!("\n{}\n", hash_slot_assembly(symbol).2));
        }
    }
    let bytes = graph.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
    let patched = format!("{patched}\n.data\n_capture_test_graph:\n.byte {bytes}\n");
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    let _ = std::fs::remove_dir_all(directory);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Creates and releases the writer reference around a real fill call without mutating compiler visibility.
fn fill_shim(length: usize, retarget: bool) -> String {
    let publish_writer = if retarget {
        let publish = hash_slot_assembly("_capture_test_writer").1;
        if target().arch == Arch::AArch64 { publish } else { format!("mov rdi, rax\n{publish}") }
    } else { String::new() };
    let save_result = if retarget { save_result_shim() } else { String::new() };
    if target().arch == Arch::AArch64 {
        let graph = if target().platform == Platform::Linux {
            "adrp x2, _capture_test_graph\nadd x2, x2, :lo12:_capture_test_graph"
        } else {
            "adrp x2, _capture_test_graph@PAGE\nadd x2, x2, _capture_test_graph@PAGEOFF"
        };
        return format!(r#"
    sub sp, sp, #48
    stp x29, x30, [sp, #32]
    mov x1, x0
    mov x0, #5
    mov x2, #0
    bl __rt_mixed_from_value
    str x0, [sp]
    bl __rt_reference_new
    str x0, [sp, #8]
    {publish_writer}
    ldr x0, [sp]
    bl __rt_decref_any
    mov x0, #0
    ldr x1, [sp, #8]
    {graph}
    mov x3, #{length}
    bl __rt_mbstring_capture_reference_fill
    str x0, [sp, #16]
    {save_result}
    ldr x0, [sp, #8]
    bl __rt_decref_any
    ldr x0, [sp, #16]
    ldp x29, x30, [sp, #32]
    add sp, sp, #48
    cmp x0, #2
    b.ne .L_capture_fill_return
    b __rt_throw_current
.L_capture_fill_return:
    ret
"#);
    }
    format!(r#"
    push rbp
    mov rbp, rsp
    sub rsp, 32
    mov rax, 5
    xor esi, esi
    call __rt_mixed_from_value
    mov QWORD PTR [rsp], rax
    call __rt_reference_new
    mov QWORD PTR [rsp + 8], rax
    {publish_writer}
    mov rax, QWORD PTR [rsp]
    call __rt_decref_any
    xor edi, edi
    mov rsi, QWORD PTR [rsp + 8]
    lea rdx, [rip + _capture_test_graph]
    mov rcx, {length}
    call __rt_mbstring_capture_reference_fill
    mov QWORD PTR [rsp + 16], rax
    {save_result}
    mov rax, QWORD PTR [rsp + 8]
    call __rt_decref_any
    mov rax, QWORD PTR [rsp + 16]
    leave
    cmp eax, 2
    je __rt_throw_current
    ret
"#)
}

/// Retains the final selected hash for PHP observation after the writer's reference is released.
fn save_result_shim() -> String {
    let publish_result = hash_slot_assembly("_capture_test_result").1;
    let clear_writer = hash_slot_assembly("_capture_test_writer").1;
    if target().arch == Arch::AArch64 {
        format!("ldr x0, [sp, #8]\nbl __rt_mixed_unbox\nmov x0, x1\nbl __rt_incref\n{publish_result}\nmov x0, #0\n{clear_writer}")
    } else {
        format!("mov rax, QWORD PTR [rsp + 8]\ncall __rt_mixed_unbox\nmov rax, rdi\ncall __rt_incref\nmov rdi, rax\n{publish_result}\nxor edi, edi\n{clear_writer}")
    }
}

/// Assigns a PHP-created replacement hash through the live persistent reference while a store is active.
pub(super) fn retarget_shim() -> String {
    let load_writer = hash_slot_assembly("_capture_test_writer").0;
    if target().arch == Arch::AArch64 {
        return format!(r#"
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    mov x1, x0
    mov x0, #5
    mov x2, #0
    bl __rt_mixed_from_value
    str x0, [sp]
    {load_writer}
    ldr x1, [sp]
    bl __rt_reference_replace
    bl __rt_decref_any
    ldr x0, [sp]
    bl __rt_decref_any
    mov x0, #0
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    ret
"#);
    }
    format!(r#"
    push rbp
    mov rbp, rsp
    sub rsp, 16
    mov rax, 5
    xor esi, esi
    call __rt_mixed_from_value
    mov QWORD PTR [rsp], rax
    {load_writer}
    mov rax, rdi
    mov rdi, QWORD PTR [rsp]
    call __rt_reference_replace
    call __rt_decref_any
    mov rax, QWORD PTR [rsp]
    call __rt_decref_any
    xor eax, eax
    leave
    ret
"#)
}

/// Transfers the retained result hash to its typed PHP caller and clears the fixture's owner slot.
fn take_result_shim() -> String {
    let (load, clear, _) = hash_slot_assembly("_capture_test_result");
    if target().arch == Arch::AArch64 {
        format!("{load}\nmov x10, x0\nmov x0, #0\n{clear}\nmov x0, x10\nret")
    } else {
        format!("{load}\nmov rax, rdi\nxor edi, edi\n{clear}\nret")
    }
}
