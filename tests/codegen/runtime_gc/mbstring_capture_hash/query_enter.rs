//! Purpose:
//! Exercises native query child selection with real PHP array owners and destructor callbacks.
//!
//! Called from:
//! - Focused runtime-GC tests through the parent mbstring native-storage fixture.
//!
//! Key details:
//! - A test entry selects a child, writes one field, and releases its cursor before rethrowing.
//! - Shared child arrays and ordinary Mixed boxes must preserve their external copies.
//! - Object and callable cleanup uses compiled PHP code and the actual exception runtime.

use super::*;

/// Completes child creation and its following field write even after an old PHP destructor throws.
#[test]
fn test_mbstring_query_enter_native_destructor_boundary() {
    for payload in ["new NativeQueryEnterOld($rewrite, $fail)", "query_test_closure($rewrite, $fail)"] {
        for boxed in [false, true] {
            let source = r#"<?php
function capture_test_store(array $matches): int { return count($matches); }
function query_test_enter(array $matches): int { return count($matches); }
function query_test_observe(int &$mode): int { return $mode; }
function query_test_rewrite(int &$mode): int { return capture_test_store(["key" => $mode]); }
class NativeQueryEnterOld {
    public function __construct(public bool $rewrite, public bool $fail) {}
    public function __destruct() {
        $mode = 0;
        echo "destroy:", query_test_observe($mode), "\n";
        if ($this->rewrite) { echo "rewrite:", query_test_rewrite($mode), "\n"; }
        if ($this->fail) { throw new RuntimeException("query enter"); }
    }
}
function query_test_closure(bool $rewrite, bool $fail): callable {
    $old = new NativeQueryEnterOld($rewrite, $fail);
    return function() use ($old): void {};
}
function run_query(bool $rewrite, bool $fail): void {
    $matches = ["before" => 1, "key" => QUERY_PAYLOAD, "after" => 2];
    try { $status = query_test_enter($matches); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo json_encode($matches), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) {
    run_query(false, false); run_query(false, true);
    run_query(true, false); run_query(true, true);
}
echo "done\n";
"#.replace("QUERY_PAYLOAD", payload);
            let mut expected = String::new();
            for _ in 0..8 {
                for rewrite in [false, true] {
                    for fail in [false, true] {
                        expected.push_str("destroy:3\n");
                        if rewrite { expected.push_str("rewrite:0\n"); }
                        expected.push_str(if fail { "caught:query enter\n" } else { "status:0\n" });
                        expected.push_str("{\"before\":1,\"key\":{\"key\":\"captured\"},\"after\":2}\n");
                    }
                }
            }
            expected.push_str("done\n");
            run(&source, boxed, true, &expected);
        }
    }
}

/// Separates child values shared directly or through root COW and preserves promoted dense copies.
#[test]
fn test_mbstring_query_enter_native_nested_cow() {
    let source = r#"<?php
function query_test_enter(array $matches): int { return count($matches); }
function run_query(): void {
    $child = ["key" => "before", "keep" => "v"];
    $root = ["key" => $child];
    echo "status:", query_test_enter($root), "\n";
    echo json_encode($root), ":", json_encode($child), "\n";
    $root = ["before" => 1, "key" => ["key" => "before", "keep" => "v"], "after" => 2];
    $snapshot = $root;
    $snapshot["extra"] = true;
    echo "status:", query_test_enter($root), "\n";
    echo json_encode($root), ":", json_encode($snapshot), "\n";
    $dense = ["first", "last"];
    $root = ["key" => $dense];
    echo "status:", query_test_enter($root), "\n";
    echo json_encode($root), ":", json_encode($dense), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_query(); }
echo "done\n";
"#;
    let expected = concat!(
        "status:0\n{\"key\":{\"key\":\"captured\",\"keep\":\"v\"}}:{\"key\":\"before\",\"keep\":\"v\"}\n",
        "status:0\n{\"before\":1,\"key\":{\"key\":\"captured\",\"keep\":\"v\"},\"after\":2}:{\"before\":1,\"key\":{\"key\":\"before\",\"keep\":\"v\"},\"after\":2,\"extra\":true}\n",
        "status:0\n{\"key\":{\"0\":\"first\",\"1\":\"last\",\"key\":\"captured\"}}:[\"first\",\"last\"]\n",
    ).repeat(8) + "done\n";
    run(source, false, false, &expected);
}

/// Replaces only native test entries while preserving compiled PHP callers, objects, and cleanup.
fn run(source: &str, boxed: bool, callbacks: bool, expected: &str) {
    let directory = make_cli_test_dir("mbstring_query_native_enter");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(
        source, &directory, 8_388_608, true, true);
    let (load, publish, common) = hash_slot_assembly("_query_test_selected");
    let mut patched = assembly;
    if callbacks {
        let store = function_symbol(&patched, "capture_test_store");
        patched = install_store_shim(&patched);
        patched = replace_function(&patched, "query_test_observe", &observe_shim(&load));
        let jump = if target().arch == Arch::AArch64 { "b" } else { "jmp" };
        patched = replace_function(&patched, "query_test_rewrite", &format!("{load}\n{jump} {store}\n"));
    }
    let prepare = if boxed { prepare_mixed_shim() } else { "" };
    patched = replace_function(&patched, "query_test_enter", &format!("{prepare}{publish}\n{}", enter_shim()));
    patched.push_str(&format!("\n{common}\n"));
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}: {}\n{}", directory.display(), output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", directory.display());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}: {}", directory.display(), output.stderr);
    std::fs::remove_dir_all(directory).unwrap();
}

/// Selects a child, writes one field, retires its pin, and then propagates any contained PHP throw.
fn enter_shim() -> String {
    let body = if target().arch == Arch::AArch64 {
        r#"
    sub sp, sp, #96
    stp x29, x30, [sp, #80]
    mov x1, x0
    mov x9, #1
    str x9, [sp]
    str x9, [sp, #48]
    mov x9, #3
    str x9, [sp, #16]
    mov x9, #0x656b
    movk x9, #0x79, lsl #16
    str x9, [sp, #24]
    add x9, sp, #24
    str x9, [sp, #8]
    mov x9, #0x6163
    movk x9, #0x7470, lsl #16
    movk x9, #0x7275, lsl #32
    movk x9, #0x6465, lsl #48
    str x9, [sp, #72]
    add x9, sp, #72
    str x9, [sp, #56]
    mov x9, #8
    str x9, [sp, #64]
    mov x0, #0
    mov x2, sp
    add x3, sp, #32
    bl __rt_mbstring_query_hash_enter
    lsr x9, x0, #1
    str x9, [sp, #40]
    mov x0, #0
    ldr x1, [sp, #32]
    mov x2, sp
    add x3, sp, #48
    bl __rt_mbstring_capture_hash_store
    lsr x9, x0, #1
    ldr x10, [sp, #40]
    orr x9, x9, x10
    str x9, [sp, #40]
    QUERY_UNPIN_ADDRESS
    ldr x1, [sp, #32]
    add x2, sp, #40
    bl __rt_cleanup_call
    ldr x0, [sp, #40]
    ldp x29, x30, [sp, #80]
    add sp, sp, #96
    cmp x0, #0
    b.ne __rt_throw_current
    ret
"#
    } else {
        r#"
    push rbp
    mov rbp, rsp
    sub rsp, 80
    mov rsi, rdi
    mov QWORD PTR [rsp], 1
    mov QWORD PTR [rsp + 16], 3
    mov QWORD PTR [rsp + 24], 0x79656b
    lea r10, [rsp + 24]
    mov QWORD PTR [rsp + 8], r10
    mov QWORD PTR [rsp + 48], 1
    mov QWORD PTR [rsp + 64], 8
    mov r10, 0x6465727574706163
    mov QWORD PTR [rsp + 72], r10
    lea r10, [rsp + 72]
    mov QWORD PTR [rsp + 56], r10
    xor edi, edi
    mov rdx, rsp
    lea rcx, [rsp + 32]
    call __rt_mbstring_query_hash_enter
    shr eax, 1
    mov QWORD PTR [rsp + 40], rax
    xor edi, edi
    mov rsi, QWORD PTR [rsp + 32]
    mov rdx, rsp
    lea rcx, [rsp + 48]
    call __rt_mbstring_capture_hash_store
    shr eax, 1
    or QWORD PTR [rsp + 40], rax
    lea rdi, [rip + __rt_hash_unpin]
    mov rsi, QWORD PTR [rsp + 32]
    lea rdx, [rsp + 40]
    call __rt_cleanup_call
    mov rax, QWORD PTR [rsp + 40]
    leave
    test rax, rax
    jne __rt_throw_current
    ret
"#
    };
    let address = if target().platform == Platform::Linux {
        "adrp x0, __rt_hash_unpin\n    add x0, x0, :lo12:__rt_hash_unpin"
    } else {
        "adrp x0, __rt_hash_unpin@PAGE\n    add x0, x0, __rt_hash_unpin@PAGEOFF"
    };
    body.replace("QUERY_UNPIN_ADDRESS", address)
}

/// Reads the parent during guarded replacement, when child selection and storage each own a pin.
fn observe_shim(load: &str) -> String {
    let body = if target().arch == Arch::AArch64 {
        r#"
    ldr x9, [x0, #48]
    cmp x9, #2
    b.ne .L_query_enter_bad_pin
    ldr x0, [x0]
    ret
.L_query_enter_bad_pin:
    mov x0, #-1
    ret
"#
    } else {
        r#"
    cmp QWORD PTR [rdi + 48], 2
    jne .L_query_enter_bad_pin
    mov rax, QWORD PTR [rdi]
    ret
.L_query_enter_bad_pin:
    mov rax, -1
    ret
"#
    };
    format!("{load}\n{body}")
}
