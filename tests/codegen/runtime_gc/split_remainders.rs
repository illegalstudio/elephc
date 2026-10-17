//! Purpose:
//! Verifies allocator split remainder placement, ownership accounting and cache drain.
//!
//! Called from:
//! - Focused runtime GC codegen tests on every executable CI target.
//!
//! Key details:
//! - The harness calls real runtime helpers and enables their free-list validator.
//! - Boundary payloads exercise all four small bins and the ordered-list fallback.

use crate::support::*;

/// Selects an instruction using the same target as the compiled runtime harness.
fn instruction(emitter: &mut String, arm: &str, x86: &str) {
    let text = if target().arch == Arch::AArch64 { arm } else { x86 };
    emitter.push_str(&format!("    {text}\n"));
}

/// Emits runtime calls in the harness dialect normalized by the compiler test helper.
fn call(emitter: &mut String, label: &str) {
    instruction(emitter, &format!("bl {label}"), &format!("call {label}"));
}

/// Materializes allocator state addresses using the harness's macOS-to-Linux normalization.
fn symbol_address(emitter: &mut String, register: &str, symbol: &str) {
    if target().arch == Arch::AArch64 {
        emitter.push_str(&format!("    adrp {register}, {symbol}@PAGE\n    add {register}, {register}, {symbol}@PAGEOFF\n"));
    } else {
        emitter.push_str(&format!("    lea {register}, [rip + {symbol}]\n"));
    }
}

/// Branches to the single failure output when the preceding comparison differs.
fn expect_equal(emitter: &mut String) {
    instruction(emitter, "b.ne split_test_fail", "jne split_test_fail");
}

/// Checks one runtime counter or bump offset against an exact expected value.
fn expect_symbol(emitter: &mut String, symbol: &str, expected: usize) {
    let reg = if target().arch == Arch::AArch64 { "x9" } else { "r8" };
    symbol_address(emitter, reg, symbol);
    instruction(emitter, "ldr x9, [x9]", "mov r8, QWORD PTR [r8]");
    instruction(emitter, &format!("cmp x9, #{expected}"), &format!("cmp r8, {expected}"));
    expect_equal(emitter);
}

/// Exercises exact size classes and drains split-only free storage without recounting frees.
#[test]
fn test_gc_small_split_remainder_classes_reuse_and_cache_drain() {
    let mut emitter = String::new();
    instruction(&mut emitter, "sub sp, sp, #64", "sub rsp, 64");
    for remainder in [8, 16, 24, 32, 40, 64, 72] {
        for symbol in ["_gc_allocs", "_gc_frees", "_gc_peak"] {
            let reg = if target().arch == Arch::AArch64 { "x9" } else { "r8" };
            symbol_address(&mut emitter, reg, symbol);
            instruction(&mut emitter, "str xzr, [x9]", "mov QWORD PTR [r8], 0");
        }
        let payload = 80 + 16 + remainder;
        instruction(&mut emitter, &format!("mov x0, #{payload}"), &format!("mov eax, {payload}"));
        call(&mut emitter, "__rt_heap_alloc");
        instruction(&mut emitter, "str x0, [sp]", "mov QWORD PTR [rsp], rax");
        instruction(&mut emitter, "mov x0, #8", "mov eax, 8");
        call(&mut emitter, "__rt_heap_alloc");
        instruction(&mut emitter, "str x0, [sp, #8]", "mov QWORD PTR [rsp + 8], rax");
        instruction(&mut emitter, "ldr x0, [sp]", "mov rax, QWORD PTR [rsp]");
        call(&mut emitter, "__rt_heap_free");
        instruction(&mut emitter, "mov x0, #80", "mov eax, 80");
        call(&mut emitter, "__rt_heap_alloc");
        instruction(&mut emitter, "ldr x9, [sp]", "mov r8, QWORD PTR [rsp]");
        instruction(&mut emitter, "cmp x0, x9", "cmp rax, r8");
        expect_equal(&mut emitter);
        expect_symbol(&mut emitter, "_gc_allocs", 3);
        expect_symbol(&mut emitter, "_gc_frees", 1);
        expect_symbol(&mut emitter, "_gc_live", 120);
        expect_symbol(&mut emitter, "_gc_peak", payload + 40);
        expect_symbol(&mut emitter, "_heap_off", payload + 40);
        let bin = if remainder <= 8 { 0 } else if remainder <= 16 { 8 }
            else if remainder <= 32 { 16 } else { 24 };
        let symbol = if remainder <= 64 { "_heap_small_bins" } else { "_heap_free_list" };
        let offset = if remainder <= 64 { bin } else { 0 };
        let reg = if target().arch == Arch::AArch64 { "x9" } else { "r8" };
        symbol_address(&mut emitter, reg, symbol);
        instruction(&mut emitter, &format!("ldr x10, [x9, #{offset}]"),
            &format!("mov r9, QWORD PTR [r8 + {offset}]"));
        instruction(&mut emitter, "ldr x11, [sp]", "mov r10, QWORD PTR [rsp]");
        instruction(&mut emitter, "add x11, x11, #80", "add r10, 80");
        instruction(&mut emitter, "cmp x10, x11", "cmp r9, r10");
        expect_equal(&mut emitter);
        instruction(&mut emitter, "ldr w12, [x10]", "mov ecx, DWORD PTR [r9]");
        instruction(&mut emitter, &format!("cmp w12, #{remainder}"), &format!("cmp ecx, {remainder}"));
        expect_equal(&mut emitter);
        if remainder <= 64 { expect_symbol(&mut emitter, "_heap_free_list", 0); }
        call(&mut emitter, "__rt_heap_debug_validate_free_list");
        instruction(&mut emitter, &format!("mov x0, #{remainder}"), &format!("mov eax, {remainder}"));
        call(&mut emitter, "__rt_heap_alloc");
        instruction(&mut emitter, "str x0, [sp, #16]", "mov QWORD PTR [rsp + 16], rax");
        instruction(&mut emitter, "ldr x9, [sp]", "mov r8, QWORD PTR [rsp]");
        instruction(&mut emitter, "add x9, x9, #96", "add r8, 96");
        instruction(&mut emitter, "cmp x0, x9", "cmp rax, r8");
        expect_equal(&mut emitter);
        expect_symbol(&mut emitter, "_heap_off", payload + 40);
        for offset in [0, 16] {
            instruction(&mut emitter, &format!("ldr x0, [sp, #{offset}]"),
                &format!("mov rax, QWORD PTR [rsp + {offset}]"));
            call(&mut emitter, "__rt_heap_free");
        }
        call(&mut emitter, "__rt_gc_mem_caches");
        let released = if remainder <= 64 { remainder + 16 } else { 0 };
        instruction(&mut emitter, &format!("cmp x0, #{released}"), &format!("cmp rax, {released}"));
        expect_equal(&mut emitter);
        call(&mut emitter, "__rt_gc_mem_caches");
        instruction(&mut emitter, "cmp x0, #0", "cmp rax, 0");
        expect_equal(&mut emitter);
        expect_symbol(&mut emitter, "_gc_allocs", 4);
        expect_symbol(&mut emitter, "_gc_frees", 3);
        expect_symbol(&mut emitter, "_gc_live", 24);
        instruction(&mut emitter, "ldr x0, [sp, #8]", "mov rax, QWORD PTR [rsp + 8]");
        call(&mut emitter, "__rt_heap_free");
        expect_symbol(&mut emitter, "_heap_off", 0);
        expect_symbol(&mut emitter, "_heap_free_list", 0);
        expect_symbol(&mut emitter, "_gc_live", 0);
        expect_symbol(&mut emitter, "_gc_frees", 4);
    }
    instruction(&mut emitter, "mov x0, #1", "mov eax, 1");
    instruction(&mut emitter, "b split_test_print", "jmp split_test_print");
    emitter.push_str("split_test_fail:\n");
    instruction(&mut emitter, "mov x0, #0", "xor eax, eax");
    emitter.push_str("split_test_print:\n");
    call(&mut emitter, "__rt_itoa");
    instruction(&mut emitter, "mov x0, #1", "mov rsi, rax");
    if target().arch == Arch::X86_64 { instruction(&mut emitter, "", "mov edi, 1"); }
    instruction(&mut emitter, "mov x16, #4", "mov eax, 1");
    instruction(&mut emitter, "svc #0x80", "syscall");
    instruction(&mut emitter, "add sp, sp, #64", "add rsp, 64");
    assert_eq!(compile_harness_and_run_with_heap_debug("<?php", 65536, &emitter), "1");
}

/// Draining a never-freed split remainder preserves counters and permits later coalescing.
#[test]
fn test_gc_small_split_remainder_drains_before_live_allocation_free() {
    let mut emitter = String::new();
    instruction(&mut emitter, "sub sp, sp, #64", "sub rsp, 64");
    for remainder in [8, 16, 24, 32, 40, 64, 72] {
        // -- create a split with a live allocation on each side --
        for symbol in ["_gc_allocs", "_gc_frees", "_gc_peak"] {
            let reg = if target().arch == Arch::AArch64 { "x9" } else { "r8" };
            symbol_address(&mut emitter, reg, symbol);
            instruction(&mut emitter, "str xzr, [x9]", "mov QWORD PTR [r8], 0");
        }
        let payload = 80 + 16 + remainder;
        instruction(&mut emitter, &format!("mov x0, #{payload}"), &format!("mov eax, {payload}"));
        call(&mut emitter, "__rt_heap_alloc");
        instruction(&mut emitter, "str x0, [sp]", "mov QWORD PTR [rsp], rax");
        instruction(&mut emitter, "mov x0, #8", "mov eax, 8");
        call(&mut emitter, "__rt_heap_alloc");
        instruction(&mut emitter, "str x0, [sp, #8]", "mov QWORD PTR [rsp + 8], rax");
        instruction(&mut emitter, "ldr x0, [sp]", "mov rax, QWORD PTR [rsp]");
        call(&mut emitter, "__rt_heap_free");
        instruction(&mut emitter, "mov x0, #80", "mov eax, 80");
        call(&mut emitter, "__rt_heap_alloc");
        instruction(&mut emitter, "ldr x9, [sp]", "mov r8, QWORD PTR [rsp]");
        instruction(&mut emitter, "cmp x0, x9", "cmp rax, r8");
        expect_equal(&mut emitter);

        // -- drain the split remainder without allocating it or freeing either live owner --
        for pass in 0..2 {
            expect_symbol(&mut emitter, "_gc_allocs", 3);
            expect_symbol(&mut emitter, "_gc_frees", 1);
            expect_symbol(&mut emitter, "_gc_live", 120);
            expect_symbol(&mut emitter, "_gc_peak", payload + 40);
            expect_symbol(&mut emitter, "_heap_off", payload + 40);
            call(&mut emitter, "__rt_gc_mem_caches");
            let released = if pass == 0 && remainder <= 64 { remainder + 16 } else { 0 };
            instruction(&mut emitter, &format!("cmp x0, #{released}"), &format!("cmp rax, {released}"));
            expect_equal(&mut emitter);
        }
        expect_symbol(&mut emitter, "_gc_allocs", 3);
        expect_symbol(&mut emitter, "_gc_frees", 1);
        expect_symbol(&mut emitter, "_gc_live", 120);
        expect_symbol(&mut emitter, "_gc_peak", payload + 40);
        expect_symbol(&mut emitter, "_heap_off", payload + 40);
        let reg = if target().arch == Arch::AArch64 { "x9" } else { "r8" };
        symbol_address(&mut emitter, reg, "_heap_small_bins");
        for offset in [0, 8, 16, 24] {
            instruction(&mut emitter, &format!("ldr x10, [x9, #{offset}]"),
                &format!("mov r9, QWORD PTR [r8 + {offset}]"));
            instruction(&mut emitter, "cmp x10, #0", "cmp r9, 0");
            expect_equal(&mut emitter);
        }
        call(&mut emitter, "__rt_heap_debug_validate_free_list");

        // -- free the leading owner and check coalescing before trimming the final guard --
        instruction(&mut emitter, "ldr x0, [sp]", "mov rax, QWORD PTR [rsp]");
        call(&mut emitter, "__rt_heap_free");
        symbol_address(&mut emitter, reg, "_heap_free_list");
        instruction(&mut emitter, "ldr x10, [x9]", "mov r9, QWORD PTR [r8]");
        instruction(&mut emitter, "ldr x11, [sp]", "mov r10, QWORD PTR [rsp]");
        instruction(&mut emitter, "sub x11, x11, #16", "sub r10, 16");
        instruction(&mut emitter, "cmp x10, x11", "cmp r9, r10");
        expect_equal(&mut emitter);
        instruction(&mut emitter, "ldr w12, [x10]", "mov ecx, DWORD PTR [r9]");
        instruction(&mut emitter, &format!("cmp w12, #{payload}"), &format!("cmp ecx, {payload}"));
        expect_equal(&mut emitter);
        expect_symbol(&mut emitter, "_gc_allocs", 3);
        expect_symbol(&mut emitter, "_gc_frees", 2);
        expect_symbol(&mut emitter, "_gc_live", 24);
        instruction(&mut emitter, "ldr x0, [sp, #8]", "mov rax, QWORD PTR [rsp + 8]");
        call(&mut emitter, "__rt_heap_free");
        expect_symbol(&mut emitter, "_heap_off", 0);
        expect_symbol(&mut emitter, "_heap_free_list", 0);
        expect_symbol(&mut emitter, "_gc_live", 0);
        expect_symbol(&mut emitter, "_gc_allocs", 3);
        expect_symbol(&mut emitter, "_gc_frees", 3);
    }
    instruction(&mut emitter, "mov x0, #1", "mov eax, 1");
    instruction(&mut emitter, "b split_test_print", "jmp split_test_print");
    emitter.push_str("split_test_fail:\n");
    instruction(&mut emitter, "mov x0, #0", "xor eax, eax");
    emitter.push_str("split_test_print:\n");
    call(&mut emitter, "__rt_itoa");
    instruction(&mut emitter, "mov x0, #1", "mov rsi, rax");
    if target().arch == Arch::X86_64 { instruction(&mut emitter, "", "mov edi, 1"); }
    instruction(&mut emitter, "mov x16, #4", "mov eax, 1");
    instruction(&mut emitter, "svc #0x80", "syscall");
    instruction(&mut emitter, "add sp, sp, #64", "add rsp, 64");
    assert_eq!(compile_harness_and_run_with_heap_debug("<?php", 65536, &emitter), "1");
}

/// Checks repeated nested decoding and reuse without changing existing JSON lifetime semantics.
#[test]
fn test_gc_small_split_remainder_nested_json_php_and_optimizer_modes() {
    let source = "<?php $json='[[0,1,2,3,4,5,6,7,8],[9,10,11,12,13,14,15,16,17]]';for($i=0;$i<4;$i++){$value=json_decode($json,true);echo count($value),':',$value[1][8],'|';unset($value);gc_mem_caches();}";
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = make_cli_test_dir("elephc_split_remainder_json");
        let php_path = dir.join("main.php");
        fs::write(&php_path, source).unwrap();
        let compile = elephc_cli_command(&dir).args([mode, "--heap-debug"])
            .arg(&php_path).output().unwrap();
        let native = if compile.status.success() { Some(run_binary(&dir.join("main"), &dir)) } else { None };
        let php = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| Command::new("php")
            .arg("-n").arg(&php_path).output().unwrap());
        fs::remove_dir_all(&dir).unwrap();
        assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
        let native = native.unwrap();
        assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(native.stdout, b"2:17|2:17|2:17|2:17|");
        // JSON root retention already exists on the base compiler. The exact byte
        // counters are compared separately; allocator metadata must still validate.
        assert!(!String::from_utf8_lossy(&native.stderr).contains("Fatal error:"));
        if let Some(php) = php {
            assert!(php.status.success());
            assert_eq!(native.stdout, php.stdout, "{mode}");
        }
    }
}

/// Preserves predecessor/successor links and a pre-existing bin when splitting an interior node.
#[test]
fn test_gc_small_split_remainder_preserves_both_existing_chains() {
    let mut emitter = String::new();
    instruction(&mut emitter, "sub sp, sp, #96", "sub rsp, 96");
    for (slot, size) in [24, 8, 72, 8, 120, 8, 192, 8].into_iter().enumerate() {
        instruction(&mut emitter, &format!("mov x0, #{size}"), &format!("mov eax, {size}"));
        call(&mut emitter, "__rt_heap_alloc");
        let offset = slot * 8;
        instruction(&mut emitter, &format!("str x0, [sp, #{offset}]"),
            &format!("mov QWORD PTR [rsp + {offset}], rax"));
    }
    for offset in [0, 16, 32, 48] {
        instruction(&mut emitter, &format!("ldr x0, [sp, #{offset}]"),
            &format!("mov rax, QWORD PTR [rsp + {offset}]"));
        call(&mut emitter, "__rt_heap_free");
    }
    instruction(&mut emitter, "mov x0, #80", "mov eax, 80");
    call(&mut emitter, "__rt_heap_alloc");
    instruction(&mut emitter, "ldr x9, [sp, #32]", "mov r8, QWORD PTR [rsp + 32]");
    instruction(&mut emitter, "cmp x0, x9", "cmp rax, r8");
    expect_equal(&mut emitter);
    let register = if target().arch == Arch::AArch64 { "x9" } else { "r8" };
    symbol_address(&mut emitter, register, "_heap_free_list");
    instruction(&mut emitter, "ldr x10, [x9]", "mov r9, QWORD PTR [r8]");
    instruction(&mut emitter, "ldr x11, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]");
    instruction(&mut emitter, "sub x11, x11, #16", "sub r10, 16");
    instruction(&mut emitter, "cmp x10, x11", "cmp r9, r10");
    expect_equal(&mut emitter);
    instruction(&mut emitter, "ldr x10, [x10, #16]", "mov r9, QWORD PTR [r9 + 16]");
    instruction(&mut emitter, "ldr x11, [sp, #48]", "mov r10, QWORD PTR [rsp + 48]");
    instruction(&mut emitter, "sub x11, x11, #16", "sub r10, 16");
    instruction(&mut emitter, "cmp x10, x11", "cmp r9, r10");
    expect_equal(&mut emitter);
    instruction(&mut emitter, "ldr x10, [x10, #16]", "mov r9, QWORD PTR [r9 + 16]");
    instruction(&mut emitter, "cmp x10, #0", "cmp r9, 0");
    expect_equal(&mut emitter);
    for (offset, original_offset, delta) in [(64, 32, 96), (72, 0, 0)] {
        instruction(&mut emitter, "mov x0, #24", "mov eax, 24");
        call(&mut emitter, "__rt_heap_alloc");
        instruction(&mut emitter, &format!("str x0, [sp, #{offset}]"),
            &format!("mov QWORD PTR [rsp + {offset}], rax"));
        instruction(&mut emitter, &format!("ldr x9, [sp, #{original_offset}]"),
            &format!("mov r8, QWORD PTR [rsp + {original_offset}]"));
        instruction(&mut emitter, &format!("add x9, x9, #{delta}"), &format!("add r8, {delta}"));
        instruction(&mut emitter, "cmp x0, x9", "cmp rax, r8");
        expect_equal(&mut emitter);
    }
    for offset in [64, 72, 32, 8, 24, 40, 56] {
        instruction(&mut emitter, &format!("ldr x0, [sp, #{offset}]"),
            &format!("mov rax, QWORD PTR [rsp + {offset}]"));
        call(&mut emitter, "__rt_heap_free");
    }
    call(&mut emitter, "__rt_gc_mem_caches");
    expect_symbol(&mut emitter, "_heap_off", 0);
    expect_symbol(&mut emitter, "_gc_live", 0);
    expect_symbol(&mut emitter, "_gc_allocs", 11);
    expect_symbol(&mut emitter, "_gc_frees", 11);
    instruction(&mut emitter, "mov x0, #1", "mov eax, 1");
    instruction(&mut emitter, "b split_test_print", "jmp split_test_print");
    emitter.push_str("split_test_fail:\n");
    instruction(&mut emitter, "mov x0, #0", "xor eax, eax");
    emitter.push_str("split_test_print:\n");
    call(&mut emitter, "__rt_itoa");
    instruction(&mut emitter, "mov x0, #1", "mov rsi, rax");
    if target().arch == Arch::X86_64 { instruction(&mut emitter, "", "mov edi, 1"); }
    instruction(&mut emitter, "mov x16, #4", "mov eax, 1");
    instruction(&mut emitter, "svc #0x80", "syscall");
    instruction(&mut emitter, "add sp, sp, #96", "add rsp, 96");
    assert_eq!(compile_harness_and_run_with_heap_debug("<?php", 65536, &emitter), "1");
}
