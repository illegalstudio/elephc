//! Purpose:
//! Provides native test shims for COW snapshots acquired during PHP capture destructors.
//!
//! Called from:
//! - The parent capture runtime-GC fixture replaces only its typed PHP helper bodies.
//!
//! Key details:
//! - Snapshots own a separate hash; a typed PHP return reads the retained receiver after destruction.
//! - Real hash copying, Mixed cloning, object release, and heap diagnostics remain active.

use crate::support::target;
use elephc::codegen_support::platform::Arch;

/// Creates a COW copy during a destructor, either releasing it immediately or retaining it for PHP.
pub(super) fn copy_shim(load_selected: &str, publish_snapshot: &str) -> String {
    if target().arch == Arch::AArch64 {
        return format!(r#"
.L_capture_test_copy:
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    str x0, [sp]
    {load_selected}
    bl __rt_incref
    {load_selected}
    bl __rt_hash_to_mixed
    ldr x9, [sp]
    cmp x9, #8
    b.eq .L_capture_test_copy_keep
    bl __rt_decref_hash
    b .L_capture_test_copy_done
.L_capture_test_copy_keep:
    {publish_snapshot}
.L_capture_test_copy_done:
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    mov x0, #0
    ret
"#);
    }
    format!(r#"
.L_capture_test_copy:
    sub rsp, 24
    mov QWORD PTR [rsp], rdi
    {load_selected}
    mov rax, rdi
    call __rt_incref
    {load_selected}
    call __rt_hash_to_mixed
    cmp QWORD PTR [rsp], 8
    je .L_capture_test_copy_keep
    call __rt_decref_hash
    jmp .L_capture_test_copy_done
.L_capture_test_copy_keep:
    mov rdi, rax
    {publish_snapshot}
.L_capture_test_copy_done:
    add rsp, 24
    xor eax, eax
    ret
"#)
}
/// Transfers the snapshot's object into a typed PHP return, clearing the temporary hash owner.
pub(super) fn take_shim(load_snapshot: &str, publish_snapshot: &str) -> String {
    if target().arch == Arch::AArch64 {
        return format!(r#"
    sub sp, sp, #48
    stp x29, x30, [sp, #32]
    {load_snapshot}
    str x0, [sp]
    mov x0, #0
    {publish_snapshot}
    mov x9, #0x656b
    movk x9, #0x79, lsl #16
    str x9, [sp, #16]
    ldr x0, [sp]
    add x1, sp, #16
    mov x2, #3
    bl __rt_hash_get
    mov x0, x1
    bl __rt_mixed_unbox
    mov x0, x1
    bl __rt_incref
    str x0, [sp, #8]
    ldr x0, [sp]
    bl __rt_decref_hash
    ldr x0, [sp, #8]
    ldp x29, x30, [sp, #32]
    add sp, sp, #48
    ret
"#);
    }
    format!(r#"
    sub rsp, 40
    {load_snapshot}
    mov QWORD PTR [rsp], rdi
    xor edi, edi
    {publish_snapshot}
    mov QWORD PTR [rsp + 16], 0x79656b
    mov rdi, QWORD PTR [rsp]
    lea rsi, [rsp + 16]
    mov edx, 3
    call __rt_hash_get
    mov rax, rdi
    call __rt_mixed_unbox
    mov rax, rdi
    call __rt_incref
    mov QWORD PTR [rsp + 8], rax
    mov rax, QWORD PTR [rsp]
    call __rt_decref_hash
    mov rax, QWORD PTR [rsp + 8]
    add rsp, 40
    ret
"#)
}
