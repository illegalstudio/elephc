//! Purpose:
//! Exercises query root removal with real PHP destructors, nested cleanup, and exception chains.
//!
//! Called from:
//! - The runtime-GC mbstring capture fixture through focused codegen tests.
//!
//! Key details:
//! - Only test entry bodies are replaced; allocation, destruction, and unwinding remain native.
//! - The observer reads the selected hash while removal holds a lifetime pin.
//! - Destructor insertions must survive removal and later exceptions without leaking owners.
//! - Expected traces match PHP unset with the same destructor actions and shared root reference.

use super::*;

/// Preserves completed deletion and reinserted values through nested, callable, and throwing cleanup.
#[test]
fn test_mbstring_query_remove_native_destructor_boundary() {
    for (payload, destructors) in [
        ("new NativeQueryOldValue(1, $rewrite, $fail)", 1),
        ("query_test_closure($rewrite, $fail)", 1),
        ("[new NativeQueryOldValue(1, $rewrite, $fail), new NativeQueryOldValue(2, $rewrite, $fail)]", 2),
    ] {
        for boxed in [false, true] {
            let source = source(payload);
            let directory = make_cli_test_dir("mbstring_query_native_remove");
            let (assembly, runtime, libraries) = compile_source_to_asm_with_options(
                &source, &directory, 8_388_608, true, true);
            let store = function_symbol(&assembly, "capture_test_store");
            let (load, publish, common) = hash_slot_assembly("_query_test_selected");
            let patched = install_store_shim(&assembly);
            let preparation = if boxed { prepare_mixed_shim() } else { "" };
            let patched = replace_function(&patched, "query_test_remove", &format!(
                "{preparation}{publish}\n{}", remove_shim()));
            let patched = replace_function(&patched, "query_test_observe", &observe_shim(&load));
            let jump = if target().arch == Arch::AArch64 { "b" } else { "jmp" };
            let patched = replace_function(&patched, "query_test_rewrite", &format!("{load}\n{jump} {store}\n"));
            let patched = format!("{patched}\n{common}\n");
            let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
                &libraries, &default_link_paths(), &[]);
            assert!(output.success, "{}: {}\n{}", directory.display(), output.stdout, output.stderr);
            assert_eq!(output.stdout, expected(destructors), "{}", directory.display());
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}: {}", directory.display(), output.stderr);
            std::fs::remove_dir_all(directory).unwrap();
        }
    }
}

/// Keeps all PHP destructor and exception behavior in compiled source while isolating the internal entry.
fn source(payload: &str) -> String {
    r#"<?php
function capture_test_store(array $matches): int { return count($matches); }
function query_test_remove(array $matches): int { return count($matches); }
function query_test_observe(int &$mode): int { return $mode; }
function query_test_rewrite(int &$mode): int { return capture_test_store(["key" => $mode]); }
class NativeQueryOldValue {
    public function __construct(public int $id, public bool $rewrite, public bool $fail) {}
    public function __destruct() {
        $mode = $this->id;
        echo "destroy:", $this->id, ":", query_test_observe($mode), "\n";
        if ($this->rewrite) { echo "rewrite:", query_test_rewrite($mode), "\n"; }
        if ($this->fail) { throw new RuntimeException("query cleanup " . $this->id); }
    }
}
function query_test_closure(bool $rewrite, bool $fail): callable {
    $old = new NativeQueryOldValue(1, $rewrite, $fail);
    return function() use ($old): void {};
}
function run_query(bool $rewrite, bool $fail): void {
    $matches = ["before" => 1, "key" => QUERY_PAYLOAD, "after" => 2];
    try { $status = query_test_remove($matches); echo "status:", $status, "\n"; }
    catch (Throwable $error) {
        echo "caught:", $error->getMessage(), "\n";
        $previous = $error->getPrevious();
        if ($previous !== null) { echo "previous:", $previous->getMessage(), "\n"; }
    }
    echo json_encode($matches), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) {
    run_query(false, false); run_query(false, true);
    run_query(true, false); run_query(true, true);
}
echo "done\n";
"#.replace("QUERY_PAYLOAD", payload)
}

/// Records observable deletion before cleanup and the insertion order of destructor replacements.
fn expected(destructors: usize) -> String {
    let mut expected = String::new();
    for _ in 0..8 {
        for rewrite in [false, true] {
            for fail in [false, true] {
                for id in 1..=destructors {
                    expected.push_str(&format!("destroy:{id}:{}\n", if rewrite && id > 1 { 3 } else { 2 }));
                    if rewrite { expected.push_str("rewrite:0\n"); }
                }
                if fail {
                    expected.push_str(&format!("caught:query cleanup {destructors}\n"));
                    if destructors == 2 { expected.push_str("previous:query cleanup 1\n"); }
                } else { expected.push_str("status:0\n"); }
                expected.push_str(if rewrite { "{\"before\":1,\"after\":2,\"key\":\"captured\"}\n" }
                    else { "{\"before\":1,\"after\":2}\n" });
            }
        }
    }
    expected.push_str("done\n");
    expected
}

/// Calls actual protected root removal and rethrows its pending exception after restoring the test frame.
fn remove_shim() -> String {
    let done = format!("{}query_remove_done", target().platform.local_label_prefix());
    if target().arch == Arch::AArch64 {
        return format!(r#"
    sub sp, sp, #48
    stp x29, x30, [sp, #32]
    mov x1, x0
    mov x9, #1
    str x9, [sp]
    mov x9, #3
    str x9, [sp, #16]
    mov x9, #0x656b
    movk x9, #0x79, lsl #16
    str x9, [sp, #24]
    add x9, sp, #24
    str x9, [sp, #8]
    mov x0, #0
    mov x2, sp
    bl __rt_mbstring_query_hash_remove
    ldp x29, x30, [sp, #32]
    add sp, sp, #48
    cmp x0, #2
    b.ne {done}
    b __rt_throw_current
{done}:
    ret
"#);
    }
    format!(r#"
    push rbp
    mov rbp, rsp
    sub rsp, 32
    mov rsi, rdi
    mov QWORD PTR [rsp], 1
    mov QWORD PTR [rsp + 16], 3
    mov QWORD PTR [rsp + 24], 0x79656b
    lea r10, [rsp + 24]
    mov QWORD PTR [rsp + 8], r10
    xor edi, edi
    mov rdx, rsp
    call __rt_mbstring_query_hash_remove
    leave
    cmp eax, 2
    jne {done}
    jmp __rt_throw_current
{done}:
    ret
"#)
}

/// Reports the destructor-visible live count and rejects an absent or extra construction pin.
fn observe_shim(load: &str) -> String {
    let body = if target().arch == Arch::AArch64 {
        r#"
    ldr x9, [x0, #48]
    cmp x9, #1
    b.ne .L_query_test_bad_pin
    ldr x0, [x0]
    ret
.L_query_test_bad_pin:
    mov x0, #-1
    ret
"#
    } else {
        r#"
    cmp QWORD PTR [rdi + 48], 1
    jne .L_query_test_bad_pin
    mov rax, QWORD PTR [rdi]
    ret
.L_query_test_bad_pin:
    mov rax, -1
    ret
"#
    };
    format!("{load}\n{body}")
}
