//! Purpose:
//! Verifies the native retained-edge count used by application GC work accounting.
//!
//! Called from:
//! - Focused native runtime GC codegen tests.
//!
//! Key details:
//! - The callback returns opaque nonzero cells; pure counting must not trace them.
//! - The complete object cost includes native slots even without a fixed PHP layout.

use crate::support::*;

/// Selects the executable fixture's register and instruction dialect.
fn instruction(out: &mut String, arm: &str, x86: &str) {
    out.push_str("    ");
    out.push_str(if target().arch == Arch::AArch64 { arm } else { x86 });
    out.push('\n');
}

/// Addresses one harness or runtime symbol without borrowing a live operand.
fn address(out: &mut String, arm: &str, x86: &str, symbol: &str) {
    if target().arch == Arch::AArch64 {
        if target().platform == Platform::Linux {
            out.push_str(&format!("    adrp {arm}, {symbol}\n    add {arm}, {arm}, :lo12:{symbol}\n"));
        } else {
            out.push_str(&format!("    adrp {arm}, {symbol}@PAGE\n    add {arm}, {arm}, {symbol}@PAGEOFF\n"));
        }
    } else { out.push_str(&format!("    lea {x86}, [rip + {symbol}]\n")); }
}

/// Pure enumeration counts two native slots and contributes them to an object's cost.
#[test]
fn test_gc_candidate_cost_counts_native_slots_without_tracing() {
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #64", "sub rsp, 64");
    address(&mut asm, "x9", "r8", "_elephc_eval_object_gc_child_fn");
    address(&mut asm, "x10", "r9", "Lnative_cost_child");
    instruction(&mut asm, "str x10, [x9]", "mov QWORD PTR [r8], r9");
    instruction(&mut asm, "mov x0, #0", "xor edi, edi");
    instruction(&mut asm, "mov x1, #0", "xor esi, esi");
    instruction(&mut asm, "mov x2, #3", "mov edx, 3");
    instruction(&mut asm, "bl __rt_gc_eval_object_children", "call __rt_gc_eval_object_children");
    instruction(&mut asm, "cmp x0, #2", "cmp rax, 2");
    instruction(&mut asm, "b.ne Lnative_cost_fail", "jne Lnative_cost_fail");
    instruction(&mut asm, "mov x0, #8", "mov eax, 8");
    instruction(&mut asm, "bl __rt_heap_alloc", "call __rt_heap_alloc");
    instruction(&mut asm, "str x0, [sp]", "mov QWORD PTR [rsp], rax");
    instruction(&mut asm, "mov x9, #-1", "mov QWORD PTR [rax], -1");
    if target().arch == Arch::AArch64 { instruction(&mut asm, "str x9, [x0]", ""); }
    instruction(&mut asm, "mov w9, #4", "mov BYTE PTR [rax - 8], 4");
    if target().arch == Arch::AArch64 { instruction(&mut asm, "strb w9, [x0, #-8]", ""); }
    instruction(&mut asm, "bl __rt_gc_node_scan_cost", "call __rt_gc_node_scan_cost");
    instruction(&mut asm, "cmp x0, #3", "cmp rax, 3");
    instruction(&mut asm, "b.ne Lnative_cost_fail", "jne Lnative_cost_fail");
    instruction(&mut asm, "ldr x0, [sp]", "mov rax, QWORD PTR [rsp]");
    instruction(&mut asm, "bl __rt_heap_free", "call __rt_heap_free");
    instruction(&mut asm, "mov x0, #1", "mov eax, 1");
    instruction(&mut asm, "b Lnative_cost_print", "jmp Lnative_cost_print");
    asm.push_str("Lnative_cost_fail:\n");
    instruction(&mut asm, "mov x0, #0", "xor eax, eax");
    asm.push_str("Lnative_cost_print:\n");
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
    address(&mut asm, "x9", "r8", "_elephc_eval_object_gc_child_fn");
    instruction(&mut asm, "str xzr, [x9]", "mov QWORD PTR [r8], 0");
    instruction(&mut asm, "b Lnative_cost_end", "jmp Lnative_cost_end");
    asm.push_str("Lnative_cost_child:\n");
    instruction(&mut asm, "mov x0, #0", "xor eax, eax");
    instruction(&mut asm, "cmp x1, #2", "cmp rsi, 2");
    instruction(&mut asm, "b.hs Lnative_cost_child_return", "jae Lnative_cost_child_return");
    instruction(&mut asm, "add x0, x1, #1", "lea rax, [rsi + 1]");
    asm.push_str("Lnative_cost_child_return:\n");
    instruction(&mut asm, "ret", "ret");
    asm.push_str("Lnative_cost_end:\n");
    instruction(&mut asm, "add sp, sp, #64", "add rsp, 64");
    assert_eq!(compile_harness_and_run("<?php", 65_536, &asm), "1");
}
