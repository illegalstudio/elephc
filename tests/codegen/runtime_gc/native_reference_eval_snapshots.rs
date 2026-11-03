//! Purpose:
//! Verifies that eval flushes a typed native parameter from its canonical reference wrapper.
//!
//! Called from:
//! - Supported-host native reference regressions in CI.
//!
//! Key details:
//! - The parameter's legacy payload is integer, while its actual managed cell owns a PHP wrapper.
//! - Opaque eval and later native reads observe the actual child, including a change of PHP type.

use crate::support::*;
use super::native_reference_validation_support::*;

/// A typed entry reaches eval as an integer, then native reads observe eval's string replacement.
#[test]
fn test_native_reference_eval_snapshot_uses_current_boxed_payload() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_eval_snapshot"));
    let source = r#"<?php
function nativeCanonicalEvalSnapshot(int &$target, string $code): void {
    eval($code); echo gettype($target), ':', $target, '|';
}
if ($argc < 0) { $target = 1; nativeCanonicalEvalSnapshot($target, ''); unset($target); }
"#;
    let (user, runtime, requirements) = without_ir_opt(||
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true));
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #32", "sub rsp, 32");
    instruction(&mut asm, "mov x0, #0", "xor eax, eax");
    instruction(&mut asm, "mov x1, #41", "mov edi, 41");
    instruction(&mut asm, "mov x2, #0", "xor esi, esi");
    call(&mut asm, "__rt_mixed_from_value");
    store(&mut asm, 16);
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 0);
    load(&mut asm, 16);
    call(&mut asm, "__rt_decref_mixed");
    instruction(&mut asm, "mov x0, #7", "mov edi, 7");
    call(&mut asm, "__rt_reference_cell_new");
    store(&mut asm, 8);
    instruction(&mut asm, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]");
    instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
    // Eval enables the compiler-owned argument snapshot collector for every native frame.
    instruction(&mut asm, "mov x0, #0", "xor edi, edi");
    instruction(&mut asm, "mov x1, #8", "mov esi, 8");
    call(&mut asm, "__rt_array_new");
    store(&mut asm, 24);
    let codes = ["echo gettype($target), ':', $target, '|';", "$target = 'changed';"];
    for (index, code) in codes.iter().enumerate() {
        instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
        address(&mut asm, "x1", "rsi", &format!("Lnative_eval_snapshot_code_{index}"));
        instruction(&mut asm, &format!("mov x2, #{}", code.len()), &format!("mov rdx, {}", code.len()));
        instruction(&mut asm, "ldr x3, [sp, #24]", "mov rcx, QWORD PTR [rsp + 24]");
        call(&mut asm, &elephc::names::function_symbol("nativeCanonicalEvalSnapshot"));
    }
    load(&mut asm, 24);
    call(&mut asm, "__rt_decref_array");
    load(&mut asm, 8);
    call(&mut asm, "__rt_reference_cell_release");
    let mut immediate_checks = String::new();
    expect_zero(&mut immediate_checks, "_rt_reference_validation_pins_top");
    expect_zero(&mut immediate_checks, "_exc_value");
    asm.push_str(&immediate_checks.replace("Lvalidation_pin_fail", "Lnative_eval_snapshot_failed"));
    instruction(&mut asm, "b Lnative_eval_snapshot_checked", "jmp Lnative_eval_snapshot_checked");
    asm.push_str("Lnative_eval_snapshot_failed:\n");
    address(&mut asm, "x9", "r10", "Lnative_eval_snapshot_failure");
    instruction(&mut asm, "mov x10, #1", "mov r11, 1");
    instruction(&mut asm, "str x10, [x9]", "mov QWORD PTR [r10], r11");
    asm.push_str("Lnative_eval_snapshot_checked:\n");
    instruction(&mut asm, "add sp, sp, #32", "add rsp, 32");
    // Eval may initialize process-lifetime caches. Run through ordinary PHP shutdown before
    // checking final ownership, rather than calling back into PHP after its shutdown already ran.
    let user = inject_before_main_cleanup(&user, &asm);
    let mut checks = String::new();
    call(&mut checks, "__rt_heap_debug_report");
    expect_zero(&mut checks, "_rt_reference_validation_pins_top");
    expect_zero(&mut checks, "_exc_value");
    expect_zero(&mut checks, "_gc_live");
    expect_zero(&mut checks, "Lnative_eval_snapshot_failure");
    append_status_output(&mut checks, 0);
    let mut user = inject_main_exit_harness(&user, &checks);
    user.push_str("\n.data\nLnative_eval_snapshot_failure:\n    .quad 0\n");
    for (index, code) in codes.iter().enumerate() {
        user.push_str(&format!("Lnative_eval_snapshot_code_{index}:\n    .byte {}\n",
            code.bytes().map(|b| b.to_string()).collect::<Vec<_>>().join(",")));
    }
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run_capture(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "integer:41|integer:41|string:changed|1", "stderr: {}", output.stderr);
    let summaries = output.stderr.lines().filter(|line| line.contains("HEAP DEBUG: leak summary:"))
        .collect::<Vec<_>>();
    assert!(summaries.len() >= 2 && summaries.iter().all(|line| line.ends_with("clean")),
        "main shutdown and the post-harness check must both leave a clean heap: {}", output.stderr);
}
