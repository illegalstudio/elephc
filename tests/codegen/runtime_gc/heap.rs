//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of runtime GC heap, including GC heap free coalesces adjacent blocks, GC heap free trims free tail chain, and GC heap alloc splits oversized free block.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use crate::support::*;

/// Verifies GC heap free coalesces adjacent freed blocks into a single larger block.
/// Allocates two 2000-element arrays, frees them with unset, then allocates a 3000-element array
/// to confirm the freed adjacent blocks are merged and reused.
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

/// Verifies GC heap free trims the free tail chain when allocating from a block with trailing free space.
/// Allocates three 2000-element arrays, frees the second and third (tail), then allocates a 5000-element
/// array to confirm the free tail chain is properly trimmed and the block is reused.
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

/// Verifies GC heap alloc splits an oversized free block when the request fits in the remainder.
/// Allocates a 4000-element array, frees it, then allocates a 1000-element array and a 2500-element
/// array to confirm the large free block is split and both requests are satisfied.
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

/// Verifies heap_alloc walks past a small free block when it cannot satisfy the current request,
/// rather than incorrectly returning it. Inline assembly harness: allocates 8, 8, 16, 8 bytes
/// (pushing results), frees the 16-byte block at offset 48, frees the 8-byte block at offset 16,
/// then requests 16 bytes — the result must NOT be the first small block. Target-specific ARM64/x86_64
/// assembly is embedded directly in the test.
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

/// Verifies heap_alloc reuses a small bin entry before bumping the heap pointer.
/// Inline assembly harness: zeroes small bins, allocates 16 then 24 bytes, frees the 16-byte block,
/// saves heap_off to the stack, allocates 12 bytes, and confirms (1) the new 12-byte alloc reuses
/// the freed 16-byte bin slot and (2) heap_off is unchanged. Target-specific ARM64/x86_64 assembly.
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

/// Verifies heap debug mode detects and reports a double free error.
/// Inline assembly harness: allocates 16 bytes, then 24 bytes, pushes both pointers,
/// frees the first block, then frees it again — expecting "heap debug detected double free" error.
/// Target-specific ARM64/x86_64 assembly.
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

/// Verifies heap debug mode detects a bad refcount by writing zero to the refcount field before calling incref.
/// Inline assembly harness: allocates 16 bytes, sets refcount to zero at offset -12, calls __rt_incref —
/// expecting "heap debug detected bad refcount" error. Target-specific ARM64/x86_64 assembly.
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

/// Verifies heap debug mode detects free-list corruption caused by an incorrect link in the free chain.
/// Inline assembly harness: allocates 16 bytes, pushes it, allocates 24 bytes, pops and frees the 24-byte
/// block, then writes an invalid address (block minus 16) as its own forward link at offset +16, and
/// requests 8 bytes — expecting "heap debug detected free-list corruption" error. Target-specific ARM64/x86_64 assembly.
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

/// Verifies heap debug mode reports an exit summary on stderr including allocs count and peak_live_bytes.
/// PHP fixture: allocates an array and unsets it; the exit summary must contain "HEAP DEBUG: allocs=",
/// "peak_live_bytes=", and "HEAP DEBUG: leak summary:".
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

/// Verifies heap debug mode preserves correct alloc size during associative string-key insertions.
/// PHP fixture: creates a string-keyed array, accesses and echoes two keys — confirms no memory corruption
/// or size bookkeeping errors occur during string-key insert and read.
#[test]
fn test_heap_debug_preserves_alloc_size_during_assoc_string_insertions() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$user = ["name" => "Alice", "city" => "NYC", "lang" => "PHP"];
echo "Name: " . $user["name"] . "\n";
echo "City: " . $user["city"] . "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Name: Alice\nCity: NYC\n");
}

/// Verifies freed payload memory is poisoned with 0xA5 and remains readable.
/// Inline assembly harness: allocates 16 bytes, frees the block, pops the pointer, reads the byte at
/// offset +8 (poison byte), and prints it — expects 165 (0xA5). Target-specific ARM64/x86_64 assembly.
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

/// Verifies __rt_heap_kind returns distinct tags for raw alloc, array, hash, and string.
/// Inline assembly harness: allocates raw 16 bytes, creates an array with __rt_array_new,
/// creates a hash with __rt_hash_new, persists a 3-byte string "ABC" with __rt_str_persist,
/// and calls __rt_heap_kind after each to produce output "0231". Target-specific ARM64/x86_64 assembly.
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

/// Verifies __rt_heap_free_safe ignores non-heap pointers and does not crash or corrupt state.
/// Inline assembly harness: calls __rt_heap_free_safe with null (0), the concat_buf address,
/// and the max address (0x7FFF_FFFF_FFFF_FFFE), then prints 1. None of these should trigger
/// a heap error or prevent the final output. Target-specific ARM64/x86_64 assembly.
#[test]
fn test_heap_free_safe_ignores_non_heap_pointers() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #0
    bl __rt_heap_free_safe
    adrp x0, _concat_buf@PAGE
    add x0, x0, _concat_buf@PAGEOFF
    bl __rt_heap_free_safe
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    bl __rt_heap_free_safe
    mov x0, #1
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    xor rax, rax
    call __rt_heap_free_safe
    lea rax, [rip + _concat_buf]
    call __rt_heap_free_safe
    mov rax, 9223372036854775806
    call __rt_heap_free_safe
    mov eax, 1
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run("<?php", 65_536, harness);
    assert_eq!(out, "1");
}

/// Verifies __rt_decref_* helpers ignore the null sentinel value (0xFFFF_FFFF_FFFF_FFFE) and return safely.
/// Inline assembly harness: loads the null sentinel value and calls __rt_decref_array, __rt_decref_hash,
/// __rt_decref_object, and __rt_decref_mixed — none should crash. Prints 1 on success.
/// Target-specific ARM64/x86_64 assembly.
#[test]
fn test_direct_decref_helpers_ignore_null_sentinel() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    bl __rt_decref_array
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    bl __rt_decref_hash
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    bl __rt_decref_object
    movz x0, #0xfffe
    movk x0, #0xffff, lsl #16
    movk x0, #0xffff, lsl #32
    movk x0, #0x7fff, lsl #48
    bl __rt_decref_mixed
    mov x0, #1
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    mov rax, 9223372036854775806
    call __rt_decref_array
    mov rax, 9223372036854775806
    call __rt_decref_hash
    mov rax, 9223372036854775806
    call __rt_decref_object
    mov rax, 9223372036854775806
    call __rt_decref_mixed
    mov eax, 1
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run("<?php", 65_536, harness);
    assert_eq!(out, "1");
}
