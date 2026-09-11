//! Purpose:
//! Verifies request-owned capture values survive GC and drain safely through destructor reentry.
//!
//! Called from:
//! - The focused native capture runtime-GC tests.
//!
//! Key details:
//! - The real capture invocation fixture separately covers automatic adoption of displaced values.
//! - Only adoption and explicit drain entry bodies are replaced with calls to production helpers.
//! - This tests ownership and resumable cleanup, not PHP object-store shutdown ordering.

use super::*;

/// Keeps adopted roots through collection and consumes reentrant additions despite pending exceptions.
#[test]
fn test_mbstring_capture_deferred_roots_and_reentrant_cleanup() {
    let source = r#"<?php
function capture_test_keep(mixed $value): int { return is_object($value) ? 1 : 0; }
function capture_test_drain(int &$marker): int {
    if ($marker < 0) { throw new RuntimeException("fixture drain"); }
    return $marker;
}
function capture_test_collect(int &$marker): int { return $marker; }
class DeferredCaptureOwner {
    public function __construct(public int $id, public bool $reenter, public bool $fail) {}
    public function __destruct() {
        echo "drop:", $this->id, "\n";
        if ($this->reenter) {
            echo "nested keep:", capture_test_keep(new DeferredCaptureOwner($this->id + 1, false, false)), "\n";
            $marker = 0;
            echo "nested drain:", capture_test_drain($marker), "\n";
            echo "nested collection:", capture_test_collect($marker), "\n";
        }
        if ($this->fail) { throw new RuntimeException("deferred cleanup"); }
    }
}
mb_strlen("");
for ($i = 0; $i < 4; $i++) {
    echo "keep:", capture_test_keep(new DeferredCaptureOwner(10, true, true)), "\n";
    echo "keep:", capture_test_keep(new DeferredCaptureOwner(20, false, false)), "\n";
    echo "before collection\n";
    $marker = 0;
    echo "collection:", capture_test_collect($marker), "\n";
    echo "after collection\n";
    $marker = 0;
    try { echo "drain:", capture_test_drain($marker), "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo "empty drain:", capture_test_drain($marker), "\n";
}
echo "done\n";
"#;
    let directory = make_cli_test_dir("mbstring_capture_deferred");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let patched = replace_function(&assembly, "capture_test_keep", &adopt_shim());
    let patched = replace_function(&patched, "capture_test_drain", &drain_shim());
    let patched = replace_function(&patched, "capture_test_collect", &collect_shim());
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}\n{}\n{}", directory.display(), output.stdout, output.stderr);
    let expected = "keep:1\nkeep:1\nbefore collection\ncollection:0\nafter collection\ndrain:drop:10\nnested keep:1\nnested drain:0\nnested collection:0\ndrop:20\ndrop:11\ncaught:deferred cleanup\nempty drain:0\n";
    assert_eq!(output.stdout, format!("{}done\n", expected.repeat(4)), "{}", directory.display());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{}", directory.display(), output.stderr);
    let _ = std::fs::remove_dir_all(directory);
}

/// Retains the fixture's borrowed Mixed argument as the sole extra owner transferred to request storage.
fn adopt_shim() -> String {
    if target().arch == Arch::AArch64 {
        return r#"
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    str x0, [sp, #16]
    bl __rt_incref
    ldr x0, [sp, #16]
    bl __rt_mbstring_defer_capture
    mov x0, #1
    ldp x29, x30, [sp], #32
    ret
"#.into();
    }
    r#"
    push rbp
    mov rbp, rsp
    sub rsp, 16
    mov QWORD PTR [rsp], rdi
    mov rax, rdi
    call __rt_incref
    mov rax, QWORD PTR [rsp]
    call __rt_mbstring_defer_capture
    mov eax, 1
    leave
    ret
"#.into()
}

/// Runs the same complete mbstring cleanup used at main exit and before web arena reset.
fn drain_shim() -> String {
    if target().arch == Arch::AArch64 {
        return r#"
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    bl __rt_mbstring_release_catalog
    mov x0, #0
    ldp x29, x30, [sp], #16
    ret
"#.into();
    }
    r#"
    push rbp
    mov rbp, rsp
    call __rt_mbstring_release_catalog
    xor eax, eax
    pop rbp
    ret
"#.into()
}

/// Invokes the actual collector without requiring a PHP-visible gc_collect_cycles builtin.
pub(in crate::codegen::runtime_gc) fn collect_shim() -> String {
    drain_shim().replace("__rt_mbstring_release_catalog", "__rt_gc_collect_cycles")
}
