//! Purpose:
//! Checks non-tail free-list insertion cost and preserves actual heap-tail reclamation.
//!
//! Called from:
//! - Focused runtime GC codegen tests on the executable CI targets.
//!
//! Key details:
//! - Test-only instrumentation counts real tail-scan visits without timing thresholds.
//! - Both ordinary frees and cached-block drains must preserve allocator accounting.

use crate::support::*;

/// Removes only the unique directory owned by one fixture, including on assertion failure.
struct FixtureDir(std::path::PathBuf);

impl Drop for FixtureDir {
    /// Releases the current fixture's generated source, objects and executable.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Selects an instruction in the target dialect accepted by the harness transformer.
fn instruction(asm: &mut String, arm: &str, x86: &str) {
    let text = if target().arch == Arch::AArch64 { arm } else { x86 };
    asm.push_str(&format!("    {text}\n"));
}

/// Materializes a runtime symbol without clobbering any other harness register.
fn address(asm: &mut String, arm: &str, x86: &str, symbol: &str) {
    if target().arch == Arch::AArch64 {
        asm.push_str(&format!("    adrp {arm}, {symbol}@PAGE\n    add {arm}, {arm}, {symbol}@PAGEOFF\n"));
    } else {
        asm.push_str(&format!("    lea {x86}, [rip + {symbol}]\n"));
    }
}

/// Calls one runtime helper using the current fixture ABI.
fn call(asm: &mut String, symbol: &str) {
    instruction(asm, &format!("bl {symbol}"), &format!("call {symbol}"));
}

/// Checks an allocator counter and branches to the harness failure output on mismatch.
fn expect(asm: &mut String, symbol: &str, value: usize) {
    address(asm, "x9", "r8", symbol);
    instruction(asm, "ldr x9, [x9]", "mov r8, QWORD PTR [r8]");
    instruction(asm, &format!("cmp x9, #{value}"), &format!("cmp r8, {value}"));
    instruction(asm, "b.ne Lheap_tail_test_fail", "jne Lheap_tail_test_fail");
}

/// Prints the unsigned integer held in the runtime's normal integer result register.
fn print_integer(asm: &mut String) {
    call(asm, "__rt_itoa");
    instruction(asm, "mov x0, #1", "mov rsi, rax");
    instruction(asm, "mov x16, #4", "mov edi, 1");
    instruction(asm, "svc #0x80", "mov eax, 1");
    if target().arch == Arch::X86_64 {
        asm.push_str("    syscall\n");
    }
}

/// Counts visits to the real runtime scan while leaving every allocator register intact.
fn instrument_tail_scan(runtime: &str) -> String {
    let label = "__rt_heap_free_trim_tail_scan:\n";
    assert_eq!(runtime.matches(label).count(), 1, "tail scan label changed");
    let counter = "_heap_tail_test_visits";
    let count = if target().arch == Arch::AArch64 {
        target().transform_assembly(&format!(
            "    adrp x17, {counter}@PAGE\n    add x17, x17, {counter}@PAGEOFF\n    ldr x16, [x17]\n    add x16, x16, #1\n    str x16, [x17]\n"
        ))
    } else {
        format!("    inc QWORD PTR [rip + {counter}]\n")
    };
    let mut asm = runtime.replacen(label, &format!("{label}{count}"), 1);
    asm.push_str(&format!("\n.comm {counter}, 8\n"));
    asm
}

/// Executes an allocator harness with test-only scan counters in both target runtime dialects.
fn run_scan_harness(asm: &str, debug: bool) -> String {
    let dir = FixtureDir(make_cli_test_dir("elephc_heap_tail_scan"));
    let (user, runtime, requirements) =
        compile_source_to_asm_with_options("<?php", &dir.0, 65_536, false, debug);
    let runtime = instrument_tail_scan(&runtime);
    let runtime_obj = runtime_obj_for_asm(&runtime);
    let user = inject_main_exit_harness(&user, asm);
    assemble_and_run(&user, &runtime_obj, &dir.0, &requirements, &default_link_paths(), &[])
}

/// Non-tail large frees skip tail searches, while freeing real tail guards still reclaims the chain.
#[test]
fn test_heap_tail_scan_skips_non_tail_insertions_and_reclaims_exposed_tail() {
    for debug in [false, true] {
        let mut asm = String::new();
        instruction(&mut asm, "sub sp, sp, #64", "sub rsp, 64");
        for (index, size) in [80, 8, 80, 8, 80, 8].into_iter().enumerate() {
            instruction(&mut asm, &format!("mov x0, #{size}"), &format!("mov eax, {size}"));
            call(&mut asm, "__rt_heap_alloc");
            instruction(&mut asm, &format!("str x0, [sp, #{}]", index * 8),
                &format!("mov QWORD PTR [rsp + {}], rax", index * 8));
        }
        for index in [4, 2, 0] {
            instruction(&mut asm, &format!("ldr x0, [sp, #{}]", index * 8),
                &format!("mov rax, QWORD PTR [rsp + {}]", index * 8));
            call(&mut asm, "__rt_heap_free");
        }
        address(&mut asm, "x9", "r8", "_heap_tail_test_visits");
        instruction(&mut asm, "ldr x0, [x9]", "mov rax, QWORD PTR [r8]");
        print_integer(&mut asm);
        expect(&mut asm, "_gc_allocs", 6);
        expect(&mut asm, "_gc_frees", 3);
        expect(&mut asm, "_gc_live", 72);
        expect(&mut asm, "_heap_off", 360);
        for index in [5, 3, 1] {
            instruction(&mut asm, &format!("ldr x0, [sp, #{}]", index * 8),
                &format!("mov rax, QWORD PTR [rsp + {}]", index * 8));
            call(&mut asm, "__rt_heap_free");
        }
        // Blocks up to 2048 bytes wait in their exact bins, so the exposed tail they form is
        // returned to the bump region by the consolidation pass, not by the free itself.
        call(&mut asm, "__rt_gc_mem_caches");
        expect(&mut asm, "_gc_frees", 6);
        expect(&mut asm, "_gc_live", 0);
        expect(&mut asm, "_heap_off", 0);
        expect(&mut asm, "_heap_free_list", 0);
        address(&mut asm, "x9", "r8", "_heap_tail_test_visits");
        instruction(&mut asm, "ldr x9, [x9]", "mov r8, QWORD PTR [r8]");
        instruction(&mut asm, "cbz x9, Lheap_tail_test_fail", "test r8, r8");
        if target().arch == Arch::X86_64 {
            asm.push_str("    jz Lheap_tail_test_fail\n");
        }
        instruction(&mut asm, "mov x0, #1", "mov eax, 1");
        instruction(&mut asm, "b Lheap_tail_test_print", "jmp Lheap_tail_test_print");
        asm.push_str("Lheap_tail_test_fail:\n");
        instruction(&mut asm, "mov x0, #0", "xor eax, eax");
        asm.push_str("Lheap_tail_test_print:\n");
        print_integer(&mut asm);
        instruction(&mut asm, "add sp, sp, #64", "add rsp, 64");
        let out = run_scan_harness(&asm, debug);
        assert_eq!(out, "01", "heap debug {debug}");
    }
}

/// Draining a cached middle block merges both neighbors and trims only if the merged end is the tail.
#[test]
fn test_heap_tail_scan_cached_drain_preserves_coalescing_and_counters() {
    for debug in [false, true] {
        for tail_live in [false, true] {
            let mut asm = String::new();
            instruction(&mut asm, "sub sp, sp, #48", "sub rsp, 48");
            for (index, size) in [80, 8, 80, 8].into_iter().enumerate() {
                instruction(&mut asm, &format!("mov x0, #{size}"), &format!("mov eax, {size}"));
                call(&mut asm, "__rt_heap_alloc");
                instruction(&mut asm, &format!("str x0, [sp, #{}]", index * 8),
                    &format!("mov QWORD PTR [rsp + {}], rax", index * 8));
            }
            for index in [0, 2, 1] {
                instruction(&mut asm, &format!("ldr x0, [sp, #{}]", index * 8),
                    &format!("mov rax, QWORD PTR [rsp + {}]", index * 8));
                call(&mut asm, "__rt_heap_free");
            }
            if !tail_live {
                instruction(&mut asm, "ldr x0, [sp, #24]", "mov rax, QWORD PTR [rsp + 24]");
                call(&mut asm, "__rt_heap_free");
            }
            address(&mut asm, "x9", "r8", "_heap_tail_test_visits");
            instruction(&mut asm, "str xzr, [x9]", "mov QWORD PTR [r8], 0");
            call(&mut asm, "__rt_gc_mem_caches");
            // Every freed block up to 2048 bytes waits in its exact bin: two 80-byte and one
            // 8-byte block, headers included, are drained.
            instruction(&mut asm, "cmp x0, #216", "cmp rax, 216");
            instruction(&mut asm, "b.ne Lheap_tail_test_fail", "jne Lheap_tail_test_fail");
            expect(&mut asm, "_gc_allocs", 4);
            expect(&mut asm, "_gc_frees", if tail_live { 3 } else { 4 });
            expect(&mut asm, "_gc_live", if tail_live { 24 } else { 0 });
            expect(&mut asm, "_heap_off", if tail_live { 240 } else { 0 });
            if tail_live {
                expect(&mut asm, "_heap_tail_test_visits", 0);
                address(&mut asm, "x9", "r8", "_heap_free_list");
                instruction(&mut asm, "ldr x9, [x9]", "mov r8, QWORD PTR [r8]");
                instruction(&mut asm, "ldr w9, [x9]", "mov r8d, DWORD PTR [r8]");
                instruction(&mut asm, "cmp x9, #200", "cmp r8, 200");
                instruction(&mut asm, "b.ne Lheap_tail_test_fail", "jne Lheap_tail_test_fail");
                instruction(&mut asm, "ldr x0, [sp, #24]", "mov rax, QWORD PTR [rsp + 24]");
                call(&mut asm, "__rt_heap_free");
            }
            expect(&mut asm, "_gc_frees", 4);
            expect(&mut asm, "_gc_live", 0);
            expect(&mut asm, "_heap_off", 0);
            expect(&mut asm, "_heap_free_list", 0);
            expect(&mut asm, "_heap_small_bins", 0);
            // With the tail already free, the consolidation pass returns it to the bump region
            // without the free path's tail scan; a live tail freed last does take that scan.
            if tail_live {
                address(&mut asm, "x9", "r8", "_heap_tail_test_visits");
                instruction(&mut asm, "ldr x9, [x9]", "mov r8, QWORD PTR [r8]");
                instruction(&mut asm, "cbz x9, Lheap_tail_test_fail", "test r8, r8");
                if target().arch == Arch::X86_64 {
                    asm.push_str("    jz Lheap_tail_test_fail\n");
                }
            }
            instruction(&mut asm, "mov x0, #1", "mov eax, 1");
            instruction(&mut asm, "b Lheap_tail_test_print", "jmp Lheap_tail_test_print");
            asm.push_str("Lheap_tail_test_fail:\n");
            instruction(&mut asm, "mov x0, #0", "xor eax, eax");
            asm.push_str("Lheap_tail_test_print:\n");
            print_integer(&mut asm);
            instruction(&mut asm, "add sp, sp, #48", "add rsp, 48");
            assert_eq!(run_scan_harness(&asm, debug), "1", "debug {debug}, tail live {tail_live}");
        }
    }
}

/// Nested decode, aliases and cache drains retain PHP values and finish with balanced ownership.
#[test]
fn test_heap_tail_scan_nested_json_alias_release_php_and_optimizer_modes() {
    let source = r#"<?php
function releaseRows(int $n): int {
    $total = 0;
    $source = '[[1,2,3],[4,5,6]]';
    for ($i = 0; $i < $n; $i++) {
        $value = json_decode($source, true);
        $alias = $value;
        unset($value);
        $total += (int)$alias[0][1] + (int)$alias[1][2];
        unset($alias);
        gc_mem_caches();
    }
    return $total;
}
echo releaseRows($argc + 9);
"#;
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = FixtureDir(make_cli_test_dir("elephc_heap_tail_json"));
        let php = dir.0.join("main.php");
        fs::write(&php, source).expect("write heap-tail JSON fixture");
        let compiled = elephc_cli_command(&dir.0).args([mode, "--heap-debug"])
            .arg(&php).output().expect("compile heap-tail JSON fixture");
        assert!(compiled.status.success(), "{mode}: {}", String::from_utf8_lossy(&compiled.stderr));
        let output = run_binary(&dir.0.join("main"), &dir.0);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{mode}: {stderr}");
        assert_eq!(output.stdout, b"80", "{mode}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {stderr}");
        if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
            let oracle = Command::new("php").arg("-n").arg(&php).output().expect("run PHP oracle");
            assert!(oracle.status.success(), "{}", String::from_utf8_lossy(&oracle.stderr));
            assert_eq!(output.stdout, oracle.stdout, "PHP mismatch with {mode}");
        }
    }
}
