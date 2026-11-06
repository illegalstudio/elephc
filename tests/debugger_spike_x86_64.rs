//! Purpose:
//! x86_64/Linux counterpart to the AArch64 debugger guard spike. It validates
//! the same per-site guard, caller-saved-preserving slow path and
//! frame-pointer-relative local reads on the System V ABI.
//!
//! Called from:
//! - `cargo test --test debugger_spike_x86_64` on an x86_64 Linux host.
//!
//! Key details:
//! - The fixture saves the complete SysV caller-saved set: rax, rcx, rdx, rsi,
//!   rdi, r8-r11, xmm0-xmm15 and flags. The Rust hook is called between the
//!   save and restore.
//! - The guard frame stores scalar, string pointer/length and array
//!   pointer/length values for the hook. Offsets are relative to rbp.
//! - This file is compiled on x86_64 Linux CI; the local aarch64 host validates
//!   only its assembly at build time, not execution.
#![cfg(all(target_arch = "x86_64", target_os = "linux"))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::Instant;

use std::arch::global_asm;

/// Number of loop iterations used by the overhead probe.
const ITERATIONS: u64 = 2_000_000;

/// Hook invocation count. Reset by each test.
static HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);
/// Whether the hook should inspect the synthetic caller frame.
static CHECK_LOCALS: AtomicBool = AtomicBool::new(false);
/// Set by the hook when the scalar/string/array locals match their expected values.
static LOCAL_SEEN: AtomicBool = AtomicBool::new(false);
/// The two probes share the hook counters; serialize them.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Rust side of the slow path: called by `spike_wrapper` with the caller's
/// frame pointer in rdi.
///
/// # Safety
/// `frame` must point at the `spike_guard_probe` frame while `CHECK_LOCALS` is
/// true. The guard fixture owns that frame for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn spike_hook(frame: *const u64) {
    let _ = HOOK_CALLS.fetch_add(1, Ordering::Relaxed);
    if !CHECK_LOCALS.load(Ordering::Relaxed) || frame.is_null() {
        return;
    }

    // Guard frame layout: scalar fp-64, string pointer fp-48, string length
    // fp-56, array pointer fp-32, array length fp-40.
    let scalar = *frame.offset(-8);
    let string_ptr = *frame.offset(-6) as *const u8;
    let string_len = *frame.offset(-7) as usize;
    let array_ptr = *frame.offset(-4) as *const u64;
    let array_len = *frame.offset(-5) as usize;

    let string = std::slice::from_raw_parts(string_ptr, string_len);
    let array = std::slice::from_raw_parts(array_ptr, array_len);
    if scalar == 0x1234 && string == b"hello" && array == [10, 20, 30] {
        LOCAL_SEEN.store(true, Ordering::Relaxed);
    }
}

extern "C" {
    /// Runs `iterations` guard checks, calling the preserving wrapper when
    /// `flag` is non-zero. Returns 0 when the loop completes.
    fn spike_guard_probe(iterations: u64, flag: *const u32) -> i32;
    /// Sets known caller-saved values, calls the wrapper, and returns 0 when
    /// every value survived; 1 on the first mismatch.
    fn spike_abi_probe() -> i32;
}

global_asm!(
    r#"
    .text
    .p2align 4

    .globl spike_guard_probe
    .type spike_guard_probe, @function
spike_guard_probe:
    push rbp
    mov rbp, rsp
    sub rsp, 96
    mov QWORD PTR [rbp - 8], rbx
    mov QWORD PTR [rbp - 16], r12
    mov QWORD PTR [rbp - 24], r13

    lea rax, [rip + spike_array_values]
    mov QWORD PTR [rbp - 32], rax
    mov QWORD PTR [rbp - 40], 3
    lea rax, [rip + spike_local_string]
    mov QWORD PTR [rbp - 48], rax
    mov QWORD PTR [rbp - 56], 5
    mov QWORD PTR [rbp - 64], 0x1234

    mov r12, rdi
    mov r13, rsi
.Lguard_loop:
    test r12, r12
    je .Lguard_done
    cmp DWORD PTR [r13], 0
    je .Lguard_next
    mov rdi, rbp
    call spike_wrapper
.Lguard_next:
    dec r12
    jmp .Lguard_loop

.Lguard_done:
    xor eax, eax
    mov rbx, QWORD PTR [rbp - 8]
    mov r12, QWORD PTR [rbp - 16]
    mov r13, QWORD PTR [rbp - 24]
    leave
    ret
    .size spike_guard_probe, .-spike_guard_probe

    .p2align 4
    .globl spike_wrapper
    .type spike_wrapper, @function
spike_wrapper:
    pushfq
    push rbp
    mov rbp, rsp
    sub rsp, 344

    mov QWORD PTR [rsp + 0], rax
    mov QWORD PTR [rsp + 8], rcx
    mov QWORD PTR [rsp + 16], rdx
    mov QWORD PTR [rsp + 24], rsi
    mov QWORD PTR [rsp + 32], rdi
    mov QWORD PTR [rsp + 40], r8
    mov QWORD PTR [rsp + 48], r9
    mov QWORD PTR [rsp + 56], r10
    mov QWORD PTR [rsp + 64], r11

    movdqu XMMWORD PTR [rsp + 80], xmm0
    movdqu XMMWORD PTR [rsp + 96], xmm1
    movdqu XMMWORD PTR [rsp + 112], xmm2
    movdqu XMMWORD PTR [rsp + 128], xmm3
    movdqu XMMWORD PTR [rsp + 144], xmm4
    movdqu XMMWORD PTR [rsp + 160], xmm5
    movdqu XMMWORD PTR [rsp + 176], xmm6
    movdqu XMMWORD PTR [rsp + 192], xmm7
    movdqu XMMWORD PTR [rsp + 208], xmm8
    movdqu XMMWORD PTR [rsp + 224], xmm9
    movdqu XMMWORD PTR [rsp + 240], xmm10
    movdqu XMMWORD PTR [rsp + 256], xmm11
    movdqu XMMWORD PTR [rsp + 272], xmm12
    movdqu XMMWORD PTR [rsp + 288], xmm13
    movdqu XMMWORD PTR [rsp + 304], xmm14
    movdqu XMMWORD PTR [rsp + 320], xmm15

    mov rdi, QWORD PTR [rsp + 32]
    call spike_hook

    movdqu xmm0, XMMWORD PTR [rsp + 80]
    movdqu xmm1, XMMWORD PTR [rsp + 96]
    movdqu xmm2, XMMWORD PTR [rsp + 112]
    movdqu xmm3, XMMWORD PTR [rsp + 128]
    movdqu xmm4, XMMWORD PTR [rsp + 144]
    movdqu xmm5, XMMWORD PTR [rsp + 160]
    movdqu xmm6, XMMWORD PTR [rsp + 176]
    movdqu xmm7, XMMWORD PTR [rsp + 192]
    movdqu xmm8, XMMWORD PTR [rsp + 208]
    movdqu xmm9, XMMWORD PTR [rsp + 224]
    movdqu xmm10, XMMWORD PTR [rsp + 240]
    movdqu xmm11, XMMWORD PTR [rsp + 256]
    movdqu xmm12, XMMWORD PTR [rsp + 272]
    movdqu xmm13, XMMWORD PTR [rsp + 288]
    movdqu xmm14, XMMWORD PTR [rsp + 304]
    movdqu xmm15, XMMWORD PTR [rsp + 320]

    mov rax, QWORD PTR [rsp + 0]
    mov rcx, QWORD PTR [rsp + 8]
    mov rdx, QWORD PTR [rsp + 16]
    mov rsi, QWORD PTR [rsp + 24]
    mov rdi, QWORD PTR [rsp + 32]
    mov r8, QWORD PTR [rsp + 40]
    mov r9, QWORD PTR [rsp + 48]
    mov r10, QWORD PTR [rsp + 56]
    mov r11, QWORD PTR [rsp + 64]

    mov rax, QWORD PTR [rsp + 0]

    add rsp, 344
    pop rbp
    popfq
    ret
    .size spike_wrapper, .-spike_wrapper

    .p2align 4
    .globl spike_abi_probe
    .type spike_abi_probe, @function
spike_abi_probe:
    push rbp
    mov rbp, rsp
    push rbx
    push r12
    push r13
    sub rsp, 8

    lea r12, [rip + spike_abi_values]

    mov rax, QWORD PTR [r12 + 0]
    mov rcx, QWORD PTR [r12 + 8]
    mov rdx, QWORD PTR [r12 + 16]
    mov rsi, QWORD PTR [r12 + 24]
    mov rdi, rbp
    mov r8, QWORD PTR [r12 + 32]
    mov r9, QWORD PTR [r12 + 40]
    mov r10, QWORD PTR [r12 + 48]
    mov r11, QWORD PTR [r12 + 56]
    movsd xmm0, QWORD PTR [r12 + 64]
    movsd xmm15, QWORD PTR [r12 + 72]

    mov r13d, 1
    cmp r13d, 1
    call spike_wrapper
    jne .Labi_fail

    cmp rdi, rbp
    jne .Labi_fail
    cmp rax, QWORD PTR [r12 + 0]
    jne .Labi_fail
    cmp rcx, QWORD PTR [r12 + 8]
    jne .Labi_fail
    cmp rdx, QWORD PTR [r12 + 16]
    jne .Labi_fail
    cmp rsi, QWORD PTR [r12 + 24]
    jne .Labi_fail
    cmp r8, QWORD PTR [r12 + 32]
    jne .Labi_fail
    cmp r9, QWORD PTR [r12 + 40]
    jne .Labi_fail
    cmp r10, QWORD PTR [r12 + 48]
    jne .Labi_fail
    cmp r11, QWORD PTR [r12 + 56]
    jne .Labi_fail

    movq r13, xmm0
    cmp r13, QWORD PTR [r12 + 64]
    jne .Labi_fail
    movq r13, xmm15
    cmp r13, QWORD PTR [r12 + 72]
    jne .Labi_fail

    xor eax, eax
    add rsp, 8
    pop r13
    pop r12
    pop rbx
    leave
    ret

.Labi_fail:
    mov eax, 1
    add rsp, 8
    pop r13
    pop r12
    pop rbx
    leave
    ret
    .size spike_abi_probe, .-spike_abi_probe

    .section .rodata
    .p2align 3
spike_array_values:
    .quad 10
    .quad 20
    .quad 30
spike_local_string:
    .ascii "hello"
    .p2align 3
spike_abi_values:
    .quad 0x1111
    .quad 0x2222
    .quad 0x3333
    .quad 0x4444
    .quad 0x5555
    .quad 0x6666
    .quad 0x7777
    .quad 0x8888
    .quad 0x3ff0000000000000
    .quad 0x4000000000000000
"#
);

/// Verifies the hook sees the scalar, string and array locals through
/// frame-pointer-relative offsets while the guard is armed, and sees none while
/// it is dormant. Prints the measured per-iteration cost of both paths.
#[test]
fn guard_preserves_locals_and_measures_overhead() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    CHECK_LOCALS.store(true, Ordering::Relaxed);
    LOCAL_SEEN.store(false, Ordering::Relaxed);
    HOOK_CALLS.store(0, Ordering::Relaxed);

    let dormant = AtomicU32::new(0);
    let start = Instant::now();
    let code = unsafe { spike_guard_probe(ITERATIONS, dormant.as_ptr()) };
    let dormant_elapsed = start.elapsed();
    assert_eq!(code, 0, "dormant guard loop failed");
    assert_eq!(HOOK_CALLS.load(Ordering::Relaxed), 0, "dormant guard called the hook");

    HOOK_CALLS.store(0, Ordering::Relaxed);
    let active = AtomicU32::new(1);
    let start = Instant::now();
    let code = unsafe { spike_guard_probe(ITERATIONS, active.as_ptr()) };
    let active_elapsed = start.elapsed();
    assert_eq!(code, 0, "active guard loop failed");
    assert_eq!(
        HOOK_CALLS.load(Ordering::Relaxed),
        ITERATIONS as usize,
        "active guard did not call the hook once per iteration"
    );
    assert!(
        LOCAL_SEEN.load(Ordering::Relaxed),
        "hook did not read the scalar/string/array locals correctly"
    );

    let dormant_ns = dormant_elapsed.as_nanos() as f64 / ITERATIONS as f64;
    let active_ns = active_elapsed.as_nanos() as f64 / ITERATIONS as f64;
    println!(
        "debugger spike x86_64: dormant={dormant_ns:.3} ns/iter, active={active_ns:.3} ns/iter, delta={:.3} ns/iter",
        active_ns - dormant_ns
    );

    CHECK_LOCALS.store(false, Ordering::Relaxed);
}

/// Verifies the tested caller-saved integer/SIMD registers and flags survive
/// the wrapper's Rust hook call.
#[test]
fn wrapper_preserves_caller_saved_registers() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    CHECK_LOCALS.store(false, Ordering::Relaxed);
    let code = unsafe { spike_abi_probe() };
    assert_eq!(code, 0, "caller-saved register or flags was clobbered");
}
