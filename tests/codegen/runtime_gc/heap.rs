use crate::support::*;

#[test]
fn test_gc_heap_free_coalesces_adjacent_blocks() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$a = array_fill(0, 2000, 1);
$b = array_fill(0, 2000, 2);
$keep = array_fill(0, 2000, 3);
unset($a);
unset($b);
$c = array_fill(0, 3000, 4);
echo $c[0] . "|" . count($c) . "|" . $keep[0];
"#,
        65_536,
    );
    assert_eq!(out, "4|3000|3");
}

#[test]
fn test_gc_heap_free_trims_free_tail_chain() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$a = array_fill(0, 2000, 1);
$b = array_fill(0, 2000, 2);
$tail = array_fill(0, 2000, 3);
unset($b);
unset($tail);
$c = array_fill(0, 5000, 4);
echo $c[0] . "|" . count($c) . "|" . $a[0];
"#,
        65_536,
    );
    assert_eq!(out, "4|5000|1");
}

#[test]
fn test_gc_heap_alloc_splits_oversized_free_block() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$large = array_fill(0, 4000, 1);
$keep = array_fill(0, 2000, 2);
unset($large);
$small = array_fill(0, 1000, 3);
$mid = array_fill(0, 2500, 4);
echo $small[0] . "|" . count($mid) . "|" . $keep[0];
"#,
        65_536,
    );
    assert_eq!(out, "3|2500|2");
}

#[test]
fn test_gc_heap_alloc_walks_past_small_first_free_block() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_free_list@PAGE
    add x9, x9, _heap_free_list@PAGEOFF
    str xzr, [x9]
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #48]
    bl __rt_heap_free
    ldr x0, [sp, #16]
    bl __rt_heap_free
    mov x0, #16
    bl __rt_heap_alloc
    ldr x9, [sp, #16]
    cmp x0, x9
    cset x0, eq
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    lea r9, [rip + _heap_off]
    mov QWORD PTR [r9], 0
    lea r9, [rip + _heap_free_list]
    mov QWORD PTR [r9], 0
    mov eax, 8
    call __rt_heap_alloc
    push rax
    mov eax, 8
    call __rt_heap_alloc
    push rax
    mov eax, 16
    call __rt_heap_alloc
    push rax
    mov eax, 8
    call __rt_heap_alloc
    push rax
    mov rax, QWORD PTR [rsp + 24]
    call __rt_heap_free
    mov rax, QWORD PTR [rsp + 8]
    call __rt_heap_free
    mov eax, 16
    call __rt_heap_alloc
    mov r9, QWORD PTR [rsp + 8]
    cmp rax, r9
    sete al
    movzx eax, al
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run("<?php", 256, harness);
    assert_eq!(out, "1");
}

#[test]
fn test_gc_heap_alloc_reuses_small_bin_before_bump() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_free_list@PAGE
    add x9, x9, _heap_free_list@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_small_bins@PAGE
    add x9, x9, _heap_small_bins@PAGEOFF
    stp xzr, xzr, [x9]
    stp xzr, xzr, [x9, #16]
    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #16]
    bl __rt_heap_free
    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    ldr x10, [x9]
    str x10, [sp, #-16]!
    mov x0, #12
    bl __rt_heap_alloc
    ldr x9, [sp, #32]
    cmp x0, x9
    cset x11, eq
    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    ldr x9, [x9]
    ldr x10, [sp]
    cmp x9, x10
    cset x12, eq
    and x0, x11, x12
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    lea r9, [rip + _heap_off]
    mov QWORD PTR [r9], 0
    lea r9, [rip + _heap_free_list]
    mov QWORD PTR [r9], 0
    lea r9, [rip + _heap_small_bins]
    mov QWORD PTR [r9], 0
    mov QWORD PTR [r9 + 8], 0
    mov QWORD PTR [r9 + 16], 0
    mov QWORD PTR [r9 + 24], 0
    mov eax, 16
    call __rt_heap_alloc
    push rax
    mov eax, 24
    call __rt_heap_alloc
    push rax
    mov rax, QWORD PTR [rsp + 8]
    call __rt_heap_free
    lea r9, [rip + _heap_off]
    mov r10, QWORD PTR [r9]
    push r10
    mov eax, 12
    call __rt_heap_alloc
    mov r9, QWORD PTR [rsp + 16]
    cmp rax, r9
    sete r11b
    lea r9, [rip + _heap_off]
    mov r9, QWORD PTR [r9]
    mov r10, QWORD PTR [rsp]
    cmp r9, r10
    sete r12b
    and r11b, r12b
    movzx eax, r11b
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run("<?php", 256, harness);
    assert_eq!(out, "1");
}

#[test]
fn test_heap_debug_double_free_reports_error() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #16]
    bl __rt_heap_free
    ldr x0, [sp, #16]
    bl __rt_heap_free"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    push rax
    mov eax, 24
    call __rt_heap_alloc
    push rax
    mov rax, QWORD PTR [rsp + 8]
    call __rt_heap_free
    mov rax, QWORD PTR [rsp + 8]
    call __rt_heap_free"#
        }
    };
    let err = compile_harness_expect_failure("<?php", 65_536, harness);
    assert!(err.contains("heap debug detected double free"), "{err}");
}

#[test]
fn test_heap_debug_bad_refcount_reports_error() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    str wzr, [x0, #-12]
    bl __rt_incref"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    mov DWORD PTR [rax - 12], 0
    call __rt_incref"#
        }
    };
    let err = compile_harness_expect_failure("<?php", 65_536, harness);
    assert!(err.contains("heap debug detected bad refcount"), "{err}");
}

#[test]
fn test_heap_debug_free_list_corruption_reports_error() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    ldr x0, [sp], #16
    bl __rt_heap_free
    sub x9, x0, #16
    str x9, [x9, #16]
    mov x0, #8
    bl __rt_heap_alloc"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    push rax
    mov eax, 24
    call __rt_heap_alloc
    mov rax, QWORD PTR [rsp]
    call __rt_heap_free
    mov r9, QWORD PTR [rsp]
    lea r9, [r9 - 16]
    mov QWORD PTR [r9 + 16], r9
    mov eax, 8
    call __rt_heap_alloc"#
        }
    };
    let err = compile_harness_expect_failure("<?php", 65_536, harness);
    assert!(
        err.contains("heap debug detected free-list corruption"),
        "{err}"
    );
}

#[test]
fn test_heap_debug_reports_exit_summary() {
    let out = compile_and_run_with_heap_debug("<?php $a = [1, 2, 3]; unset($a);");
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: allocs="), "{}", out.stderr);
    assert!(out.stderr.contains("peak_live_bytes="), "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary:"),
        "{}",
        out.stderr
    );
}

#[test]
fn test_heap_debug_poison_freed_payload() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    bl __rt_heap_free
    ldr x0, [sp], #16
    ldrb w0, [x0, #8]
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    push rax
    call __rt_heap_free
    mov rax, QWORD PTR [rsp]
    add rsp, 8
    movzx eax, BYTE PTR [rax + 8]
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run_with_heap_debug("<?php", 65_536, harness);
    assert_eq!(out, "165");
}

#[test]
fn test_heap_kind_tags_raw_array_hash_and_string() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    mov x0, #4
    mov x1, #8
    bl __rt_array_new
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    mov x0, #4
    mov x1, #0
    bl __rt_hash_new
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    adrp x1, _concat_buf@PAGE
    add x1, x1, _concat_buf@PAGEOFF
    mov w3, #65
    strb w3, [x1]
    mov w3, #66
    strb w3, [x1, #1]
    mov w3, #67
    strb w3, [x1, #2]
    mov x2, #3
    bl __rt_str_persist
    mov x0, x1
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    call __rt_heap_kind
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall
    mov edi, 4
    mov esi, 8
    call __rt_array_new
    call __rt_heap_kind
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall
    mov edi, 4
    xor esi, esi
    call __rt_hash_new
    call __rt_heap_kind
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall
    lea rax, [rip + _concat_buf]
    mov BYTE PTR [rax], 65
    mov BYTE PTR [rax + 1], 66
    mov BYTE PTR [rax + 2], 67
    mov rdx, 3
    call __rt_str_persist
    call __rt_heap_kind
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run("<?php", 65_536, harness);
    assert_eq!(out, "0231");
}
