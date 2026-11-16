//! Purpose:
//! Verifies native traversal slot identity and side-effect-free frame cost counting.
//!
//! Called from:
//! - Focused executable runtime GC harness tests.
//!
//! Key details:
//! - Distinct slots owning the same value remain distinct owners; repeated slot relays do not.
//! - Synthetic addresses test pure metadata operations without fabricating managed heap values.

use crate::support::*;

/// Selects one reviewed instruction dialect for the executable target.
fn op(out: &mut String, arm: &str, x86: &str) {
    out.push_str("    ");
    out.push_str(if target().arch == Arch::AArch64 { arm } else { x86 });
    out.push('\n');
}

/// Rehashing preserves unique physical slots and pure-cost mode never reads a bogus PHP address.
#[test]
fn test_gc_coroutine_slot_identity_survives_rehash_and_cost_only_visits() {
    let mut asm = String::new();
    op(&mut asm, "sub sp, sp, #64", "sub rsp, 64");
    for offset in [0, 8, 16, 24, 32, 40, 48] {
        op(&mut asm, &format!("str xzr, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], 0"));
    }
    for phase in ["insert", "repeat"] {
        asm.push_str(&format!("Lslots_{phase}:\n"));
        op(&mut asm, "ldr x0, [sp, #48]", "mov rax, QWORD PTR [rsp + 48]");
        op(&mut asm, "add x0, x0, #1", "add rax, 1");
        op(&mut asm, "lsl x0, x0, #3", "shl rax, 3");
        op(&mut asm, "mov x1, sp", "mov rdi, rsp");
        op(&mut asm, "bl __rt_gc_frame_slot_unique", "call __rt_gc_frame_slot_unique");
        let expected = usize::from(phase == "insert");
        op(&mut asm, &format!("cmp x0, #{expected}"), &format!("cmp rax, {expected}"));
        op(&mut asm, "b.ne Lslots_fail", "jne Lslots_fail");
        op(&mut asm, "ldr x0, [sp, #48]", "mov rax, QWORD PTR [rsp + 48]");
        op(&mut asm, "add x0, x0, #1", "add rax, 1");
        op(&mut asm, "str x0, [sp, #48]", "mov QWORD PTR [rsp + 48], rax");
        op(&mut asm, "cmp x0, #1500", "cmp rax, 1500");
        op(&mut asm, &format!("b.lo Lslots_{phase}"), &format!("jb Lslots_{phase}"));
        op(&mut asm, "str xzr, [sp, #48]", "mov QWORD PTR [rsp + 48], 0");
    }
    op(&mut asm, "ldr x0, [sp, #40]", "mov rax, QWORD PTR [rsp + 40]");
    op(&mut asm, "cmp x0, #1500", "cmp rax, 1500");
    op(&mut asm, "b.ne Lslots_fail", "jne Lslots_fail");
    op(&mut asm, "mov x0, #3", "mov QWORD PTR [rsp + 8], 3");
    if target().arch == Arch::AArch64 { op(&mut asm, "str x0, [sp, #8]", ""); }
    op(&mut asm, "mov x0, #1", "mov edi, 1");
    op(&mut asm, "mov x1, #0", "xor esi, esi");
    op(&mut asm, "mov x2, sp", "mov rdx, rsp");
    op(&mut asm, "bl __rt_gc_frame_owner_slot", "call __rt_gc_frame_owner_slot");
    op(&mut asm, "ldr x0, [sp, #16]", "mov rax, QWORD PTR [rsp + 16]");
    op(&mut asm, "cmp x0, #1", "cmp rax, 1");
    op(&mut asm, "b.ne Lslots_fail", "jne Lslots_fail");
    op(&mut asm, "ldr x0, [sp, #24]", "mov rdi, QWORD PTR [rsp + 24]");
    op(&mut asm, if target().platform == Platform::MacOS { "bl _free" } else { "bl free" }, "call free");
    op(&mut asm, "mov x0, #1", "mov eax, 1");
    op(&mut asm, "b Lslots_print", "jmp Lslots_print");
    asm.push_str("Lslots_fail:\n");
    op(&mut asm, "mov x0, #0", "xor eax, eax");
    asm.push_str("Lslots_print:\n");
    op(&mut asm, "bl __rt_itoa", "call __rt_itoa");
    if target().arch == Arch::AArch64 {
        op(&mut asm, "mov x0, #1", "");
        if target().platform == Platform::Linux { op(&mut asm, "mov x8, #64", ""); op(&mut asm, "svc #0", ""); }
        else { op(&mut asm, "mov x16, #4", ""); op(&mut asm, "svc #0x80", ""); }
    } else { asm.push_str("    mov rsi, rax\n    mov edi, 1\n    mov eax, 1\n    syscall\n"); }
    op(&mut asm, "add sp, sp, #64", "add rsp, 64");
    assert_eq!(compile_harness_and_run("<?php", 65_536, &asm), "1");
}
