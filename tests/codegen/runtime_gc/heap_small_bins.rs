//! Purpose:
//! Bounds small allocation and release work after a large persistent-worker request.
//!
//! Called from:
//! - Focused native allocator regression tests.
//!
//! Key details:
//! - Test-only counters inspect the real runtime loops, without timing assumptions.
//! - Fifty thousand cached blocks must not slow later requests or graph teardown.

use crate::support::*;

/// Selects an instruction for the executable fixture's target dialect.
fn instruction(out: &mut String, arm: &str, x86: &str) {
    out.push_str("    ");
    out.push_str(if target().arch == Arch::AArch64 { arm } else { x86 });
    out.push('\n');
}

/// Materializes a fixture symbol without borrowing another live register.
fn address(out: &mut String, arm: &str, x86: &str, symbol: &str) {
    if target().arch == Arch::AArch64 {
        if target().platform == Platform::Linux {
            out.push_str(&format!("    adrp {arm}, {symbol}\n    add {arm}, {arm}, :lo12:{symbol}\n"));
        } else {
            out.push_str(&format!("    adrp {arm}, {symbol}@PAGE\n    add {arm}, {arm}, {symbol}@PAGEOFF\n"));
        }
    } else {
        out.push_str(&format!("    lea {x86}, [rip + {symbol}]\n"));
    }
}

/// Counts loop visits and stops a quadratic regression before it monopolizes the host.
fn instrument(runtime: &str, label: &str, counter: &str) -> String {
    let label = format!("{label}:\n");
    assert_eq!(runtime.matches(&label).count(), 1);
    let mut count = String::new();
    instruction(&mut count, "stp x16, x17, [sp, #-16]!", "push r11");
    address(&mut count, "x17", "r11", counter);
    instruction(&mut count, "ldr x16, [x17]", "inc QWORD PTR [r11]");
    instruction(&mut count, "add x16, x16, #1", "cmp QWORD PTR [r11], 1000000");
    instruction(&mut count, "str x16, [x17]", "pop r11");
    if target().arch == Arch::AArch64 {
        instruction(&mut count, "mov x17, #1048576", "");
        instruction(&mut count, "cmp x16, x17", "");
        instruction(&mut count, "ldp x16, x17, [sp], #16", "");
    }
    let next = format!("L{counter}_budget_ok");
    instruction(&mut count, &format!("b.ls {next}"), &format!("jbe {next}"));
    instruction(&mut count, "b __rt_heap_allocation_failed", "jmp __rt_heap_allocation_failed");
    count.push_str(&format!("{next}:\n"));
    let mut output = runtime.replacen(&label, &format!("{label}{count}"), 1);
    output.push_str(&format!("\n.comm {counter}, 8\n"));
    output
}

/// Exercises 50000 small frees, then 1000 larger allocations, with a constant probe budget.
#[test]
fn test_heap_small_bins_persistent_churn_has_bounded_probes() {
    let dir = make_cli_test_dir("elephc_small_bin_probes");
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        /// Removes only this harness's uniquely allocated directory on success or failure.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let _cleanup = Cleanup(dir.clone());
    let (user, runtime, requirements) =
        compile_source_to_asm_with_options("<?php", &dir, 8 * 1024 * 1024, false, false);
    let runtime = instrument(&runtime, "__rt_heap_alloc_small_bin_scan", "_small_bin_alloc_probes");
    let runtime = instrument(&runtime, "__rt_heap_free_cache_small_scan", "_small_bin_free_probes");
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #64", "sub rsp, 64");
    address(&mut asm, "x9", "r8", "_web_heap_guard_enabled");
    instruction(&mut asm, "mov x10, #1", "mov QWORD PTR [r8], 1");
    if target().arch == Arch::AArch64 { instruction(&mut asm, "str x10, [x9]", ""); }
    // Store payload bases in an independent live allocation so every cached block stays non-tail.
    instruction(&mut asm, "movz x0, #0x1a80", "mov eax, 400000");
    if target().arch == Arch::AArch64 { instruction(&mut asm, "movk x0, #6, lsl #16", ""); }
    instruction(&mut asm, "bl __rt_heap_alloc", "call __rt_heap_alloc");
    instruction(&mut asm, "str x0, [sp]", "mov QWORD PTR [rsp], rax");
    for (name, count, allocate, size) in [
        ("warm", 50000, true, 40), ("free", 50000, false, 40),
        ("later", 1000, true, 56), ("shutdown", 1000, false, 56),
    ] {
        instruction(&mut asm, "str xzr, [sp, #8]", "mov QWORD PTR [rsp + 8], 0");
        asm.push_str(&format!("Lsmall_bin_{name}:\n"));
        if allocate {
            instruction(&mut asm, &format!("mov x0, #{size}"), &format!("mov eax, {size}"));
            instruction(&mut asm, "bl __rt_heap_alloc", "call __rt_heap_alloc");
            instruction(&mut asm, "ldr x9, [sp]", "mov r8, QWORD PTR [rsp]");
            instruction(&mut asm, "ldr x10, [sp, #8]", "mov r9, QWORD PTR [rsp + 8]");
            instruction(&mut asm, "str x0, [x9, x10, lsl #3]", "mov QWORD PTR [r8 + r9 * 8], rax");
        } else {
            instruction(&mut asm, "ldr x9, [sp]", "mov r8, QWORD PTR [rsp]");
            instruction(&mut asm, "ldr x10, [sp, #8]", "mov r9, QWORD PTR [rsp + 8]");
            instruction(&mut asm, "ldr x0, [x9, x10, lsl #3]", "mov rax, QWORD PTR [r8 + r9 * 8]");
            instruction(&mut asm, "bl __rt_heap_free", "call __rt_heap_free");
        }
        instruction(&mut asm, "ldr x9, [sp, #8]", "mov r8, QWORD PTR [rsp + 8]");
        instruction(&mut asm, "add x9, x9, #1", "add r8, 1");
        instruction(&mut asm, "str x9, [sp, #8]", "mov QWORD PTR [rsp + 8], r8");
        instruction(&mut asm, &format!("mov x10, #{count}"), &format!("cmp r8, {count}"));
        if target().arch == Arch::AArch64 { instruction(&mut asm, "cmp x9, x10", ""); }
        instruction(&mut asm, &format!("b.lo Lsmall_bin_{name}"), &format!("jb Lsmall_bin_{name}"));
        if name == "warm" {
            instruction(&mut asm, "mov x0, #8", "mov eax, 8");
            instruction(&mut asm, "bl __rt_heap_alloc", "call __rt_heap_alloc");
            instruction(&mut asm, "str x0, [sp, #16]", "mov QWORD PTR [rsp + 16], rax");
        }
    }
    address(&mut asm, "x9", "r8", "_small_bin_free_probes");
    instruction(&mut asm, "ldr x0, [x9]", "mov rax, QWORD PTR [r8]");
    // No ordinary web free may traverse the duplicate-detection chain.
    instruction(&mut asm, "bl __rt_itoa", "call __rt_itoa");
    if target().arch == Arch::AArch64 {
        instruction(&mut asm, "mov x0, #1", "");
        if target().platform == Platform::Linux {
            instruction(&mut asm, "mov x8, #64", "");
            instruction(&mut asm, "svc #0", "");
        } else {
            instruction(&mut asm, "mov x16, #4", "");
            instruction(&mut asm, "svc #0x80", "");
        }
    } else {
        asm.push_str("    mov rsi, rax\n    mov edi, 1\n    mov eax, 1\n    syscall\n");
    }
    instruction(&mut asm, "add sp, sp, #64", "add rsp, 64");
    let user = inject_main_exit_harness(&user, &asm);
    let runtime_obj = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime_obj, &dir, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "0");
}
