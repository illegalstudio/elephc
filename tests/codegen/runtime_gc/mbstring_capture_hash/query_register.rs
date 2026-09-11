//! Purpose:
//! Executes shared query instruction plans through the native V5 registration adapter.
//!
//! Called from:
//! - Runtime-GC codegen tests using compiled PHP callers and the actual mbstring Rust bridge.
//!
//! Key details:
//! - Test shims supply normalized plans before the public mb_parse_str lvalue adapter is enabled.
//! - Binary writes, nested COW, append history, root removal, and PHP throws use real storage.
//! - Registration metadata crosses the seventh C argument, which resides on the x86_64 stack.

use super::*;

#[path = "query_register/reference.rs"]
mod reference;

/// Releases ternary-produced input values through ordinary PHP unset before any native writer runs.
#[test]
fn test_mbstring_query_register_input_ternary_ownership() {
    let probe = include_str!("query_register/input_ternary.php");
    let expected = "destroy:object\nremoved:0\ndestroy:closure\nremoved:0\ndone\n";
    for indexed in [false, true] {
        let source = if indexed { probe.replace("[\"key\" =>", "[").replace("$root[\"key\"]", "$root[0]") }
            else { probe.to_owned() };
        let directory = make_cli_test_dir("query_input_ternary_ownership");
        let (assembly, runtime, libraries) = compile_source_to_asm_with_options(
            &source, &directory, 8_388_608, true, true);
        let output = assemble_and_run_capture(&assembly, &runtime_obj_for_asm(&runtime), &directory,
            &libraries, &default_link_paths(), &[]);
        assert!(output.success, "{}: {}\n{}", directory.display(), output.stdout, output.stderr);
        assert_eq!(output.stdout, expected, "{}", directory.display());
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}: {}", directory.display(), output.stderr);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

/// Preserves shared children and signed append history, and distinguishes exhaustion from root removal.
#[test]
fn test_mbstring_query_register_native_nested_plans() {
    let source = r#"<?php
function query_test_store(array $root): int { return count($root); }
function query_test_limit(array $root): int { return count($root); }
function run_query(): void {
    $child = [-5 => "old", "keep" => "yes"];
    $root = ["key" => $child];
    echo query_test_store($root), ":", json_encode($root), ":", json_encode($child), "\n";
    $root = ["key" => [9223372036854775807 => "full"], "keep" => "yes"];
    echo query_test_limit($root), ":", json_encode($root), "\n";
    $root = ["key" => ["old" => "value"], "keep" => "yes"];
    echo query_test_limit($root), ":", json_encode($root), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_query(); }
echo "done\n";
"#;
    let expected = concat!(
        "0:{\"key\":{\"-5\":\"old\",\"keep\":\"yes\",\"-4\":\"v\\u0000x\"}}:{\"-5\":\"old\",\"keep\":\"yes\"}\n",
        "0:{\"key\":{\"9223372036854775807\":\"full\"},\"keep\":\"yes\"}\n",
        "10:{\"keep\":\"yes\"}\n",
    ).repeat(8) + "done\n";
    run(source, &expected, true);
}

/// Completes the field after a replaced PHP object or callable destructor throws, with balanced pins.
#[test]
fn test_mbstring_query_register_native_pending_destructors() {
    let source = r#"<?php
function query_test_store(array $root): int { return count($root); }
class NativeQueryRegisterOld {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "destroy\n";
        if ($this->fail) { throw new RuntimeException("query registration"); }
    }
}
function query_closure(bool $fail): callable {
    $old = new NativeQueryRegisterOld($fail);
    return function() use ($old): void {};
}
function run_query(bool $fail, bool $closure): void {
    $root = ["key" => $closure ? query_closure($fail) : new NativeQueryRegisterOld($fail)];
    try { $status = query_test_store($root); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo json_encode($root), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) {
    run_query(false, false); run_query(true, false);
    run_query(false, true); run_query(true, true);
}
echo "done\n";
"#;
    let expected = concat!(
        "destroy\nstatus:0\n{\"key\":[\"v\\u0000x\"]}\n",
        "destroy\ncaught:query registration\n{\"key\":[\"v\\u0000x\"]}\n",
    ).repeat(16) + "done\n";
    run(source, &expected, false);
}

/// Links normalized-plan shims to the real bridge and checks PHP output plus complete heap cleanup.
fn run(source: &str, expected: &str, limit: bool) {
    let directory = make_cli_test_dir("mbstring_query_native_register");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(
        source, &directory, 8_388_608, true, true);
    let mut patched = replace_function(&assembly, "query_test_store", &registration_shim("store", 2));
    if limit { patched = replace_function(&patched, "query_test_limit", &registration_shim("limit", 3)); }
    patched.push_str(concat!(
        "\n.data\n_query_register_key:\n.byte 107,101,121\n_query_register_value:\n.byte 118,0,120\n.p2align 3\n",
        "_query_register_store:\n.quad 1,0,1,_query_register_key,3\n.quad 2,1,0,0,0\n",
        "_query_register_limit:\n.quad 1,0,1,_query_register_key,3\n.quad 1,1,0,0,0\n.quad 3,0,1,_query_register_key,3\n",
    ));
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}: {}\n{}", directory.display(), output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", directory.display());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}: {}", directory.display(), output.stderr);
    std::fs::remove_dir_all(directory).unwrap();
}

/// Owns a persistent writer, calls the C7 adapter, and retires it before propagating PHP exceptions.
fn registration_shim(plan: &str, count: usize) -> String {
    let done = format!(
        "{}query_register_{plan}_done",
        target().platform.local_label_prefix(),
    );
    if target().arch == Arch::AArch64 {
        let address = |reg: &str, symbol: &str| if target().platform == Platform::Linux {
            format!("adrp {reg}, {symbol}\nadd {reg}, {reg}, :lo12:{symbol}")
        } else { format!("adrp {reg}, {symbol}@PAGE\nadd {reg}, {reg}, {symbol}@PAGEOFF") };
        let steps = address("x2", &format!("_query_register_{plan}"));
        let value = address("x4", "_query_register_value");
        let release = address("x0", "__rt_decref_any");
        return format!(r#"
    sub sp, sp, #64
    stp x29, x30, [sp, #48]
    mov x1, x0
    mov x0, #5
    mov x2, #0
    bl __rt_mixed_from_value
    str x0, [sp]
    bl __rt_reference_new
    str x0, [sp, #8]
    ldr x0, [sp]
    bl __rt_decref_any
    mov x0, #0
    ldr x1, [sp, #8]
    {steps}
    mov x3, #{count}
    {value}
    mov x5, #3
    add x6, sp, #24
    bl __rt_mbstring_query_register
    str x0, [sp, #16]
    str xzr, [sp, #32]
    {release}
    ldr x1, [sp, #8]
    add x2, sp, #32
    bl __rt_cleanup_call
    ldr x9, [sp, #32]
    ldr x0, [sp, #16]
    cmp x9, #0
    mov x10, #2
    csel x0, x10, x0, ne
    ldr x9, [sp, #24]
    ldp x29, x30, [sp, #48]
    add sp, sp, #64
    cmp x0, #2
    b.ne {done}
    b __rt_throw_current
{done}:
    mov x10, #10
    madd x0, x9, x10, x0
    ret
"#);
    }
    format!(r#"
    push rbp
    mov rbp, rsp
    sub rsp, 64
    mov rax, 5
    xor esi, esi
    call __rt_mixed_from_value
    mov QWORD PTR [rsp + 8], rax
    call __rt_reference_new
    mov QWORD PTR [rsp + 16], rax
    mov rax, QWORD PTR [rsp + 8]
    call __rt_decref_any
    xor edi, edi
    mov rsi, QWORD PTR [rsp + 16]
    lea rdx, [rip + _query_register_{plan}]
    mov rcx, {count}
    lea r8, [rip + _query_register_value]
    mov r9, 3
    lea r10, [rsp + 32]
    mov QWORD PTR [rsp], r10
    call __rt_mbstring_query_register
    mov QWORD PTR [rsp + 24], rax
    mov QWORD PTR [rsp + 40], 0
    lea rdi, [rip + __rt_decref_any]
    mov rsi, QWORD PTR [rsp + 16]
    lea rdx, [rsp + 40]
    call __rt_cleanup_call
    mov rax, QWORD PTR [rsp + 24]
    cmp QWORD PTR [rsp + 40], 0
    mov r10, 2
    cmovne rax, r10
    mov r10, QWORD PTR [rsp + 32]
    leave
    cmp eax, 2
    jne {done}
    jmp __rt_throw_current
{done}:
    imul r10, r10, 10
    add rax, r10
    ret
"#)
}
