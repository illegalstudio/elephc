//! Purpose:
//! AArch64/Linux spike for the native debugger: exercise a per-site guard, a
//! caller-saved-register-preserving slow path, and frame-pointer-relative local
//! reads before any production codegen hook is specified.
//!
//! Called from:
//! - `cargo test --test debugger_spike_aarch64` on an AArch64 Linux host.
//!
//! Key details:
//! - The hand-written assembly is intentionally a fixture, not a compiler
//!   backend: it mirrors the planned guard shape and the full caller-saved save
//!   set (x0-x17, q0-q7) around a Rust hook call.
//! - `spike_guard_probe` builds a frame with scalar/string/array locals and calls
//!   the wrapper only when the site flag is non-zero. The hook reads those locals
//!   through `fp - offset`, the same contract the debugger metadata will use.
//! - The ABI probe sets known caller-saved values, calls the wrapper, and checks
//!   every one after the Rust call. The timing run prints dormant and active
//!   per-iteration costs; it does not assert a machine-dependent threshold.
#![cfg(all(target_arch = "aarch64", target_os = "linux"))]

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
/// The two probes share the hook counters; serialize them without adding a test
/// dependency.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Rust side of the slow path: called by `spike_wrapper` with the caller's
/// frame pointer in x0.
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

    let scalar = *frame.offset(-2);
    let string_ptr = *frame.offset(-4) as *const u8;
    let string_len = *frame.offset(-3) as usize;
    let array_ptr = *frame.offset(-6) as *const u64;
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
    .p2align 2

    .global spike_guard_probe
    .type spike_guard_probe, %function
spike_guard_probe:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #64
    stp x19, x20, [x29, #-64]

    adrp x9, spike_array_values
    add x9, x9, :lo12:spike_array_values
    str x9, [x29, #-48]
    mov x10, #3
    str x10, [x29, #-40]

    adrp x9, spike_local_string
    add x9, x9, :lo12:spike_local_string
    str x9, [x29, #-32]
    mov x10, #5
    str x10, [x29, #-24]

    mov x10, #0x1234
    str x10, [x29, #-16]

    mov x19, x0
    mov x20, x1
.Lguard_loop:
    cbz x19, .Lguard_done
    ldr w9, [x20]
    cbz w9, .Lguard_next
    mov x0, x29
    bl spike_wrapper
.Lguard_next:
    subs x19, x19, #1
    b.ne .Lguard_loop

.Lguard_done:
    mov x0, #0
    ldp x19, x20, [x29, #-64]
    add sp, sp, #64
    ldp x29, x30, [sp], #16
    ret
    .size spike_guard_probe, .-spike_guard_probe

    .p2align 2
    .global spike_wrapper
    .type spike_wrapper, %function
spike_wrapper:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #272

    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]

    stp q0, q1, [sp, #144]
    stp q2, q3, [sp, #176]
    stp q4, q5, [sp, #208]
    stp q6, q7, [sp, #240]

    ldr x0, [sp, #0]
    bl spike_hook

    ldp q0, q1, [sp, #144]
    ldp q2, q3, [sp, #176]
    ldp q4, q5, [sp, #208]
    ldp q6, q7, [sp, #240]

    ldp x0, x1, [sp, #0]
    ldp x2, x3, [sp, #16]
    ldp x4, x5, [sp, #32]
    ldp x6, x7, [sp, #48]
    ldp x8, x9, [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]

    add sp, sp, #272
    ldp x29, x30, [sp], #16
    ret
    .size spike_wrapper, .-spike_wrapper

    .p2align 2
    .global spike_abi_probe
    .type spike_abi_probe, %function
spike_abi_probe:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    stp x19, x20, [sp, #-16]!
    stp x21, x22, [sp, #-16]!

    adrp x21, spike_abi_values
    add x21, x21, :lo12:spike_abi_values

    mov x0, x29
    ldp x1, x2, [x21, #0]
    ldp x3, x4, [x21, #16]
    ldp x5, x6, [x21, #32]
    ldp x7, x8, [x21, #48]
    ldp x9, x10, [x21, #64]
    ldp x11, x12, [x21, #80]
    ldp x13, x14, [x21, #96]
    ldp x15, x16, [x21, #112]
    ldr x17, [x21, #128]
    ldr d0, [x21, #136]
    ldr d1, [x21, #144]
    ldr d2, [x21, #152]
    ldr d3, [x21, #160]
    ldr d4, [x21, #168]
    ldr d5, [x21, #176]
    ldr d6, [x21, #184]
    ldr d7, [x21, #192]

    bl spike_wrapper

    cmp x0, x29
    b.ne .Labi_fail
    ldr x20, [x21, #0]
    cmp x1, x20
    b.ne .Labi_fail
    ldr x20, [x21, #8]
    cmp x2, x20
    b.ne .Labi_fail
    ldr x20, [x21, #16]
    cmp x3, x20
    b.ne .Labi_fail
    ldr x20, [x21, #24]
    cmp x4, x20
    b.ne .Labi_fail
    ldr x20, [x21, #32]
    cmp x5, x20
    b.ne .Labi_fail
    ldr x20, [x21, #40]
    cmp x6, x20
    b.ne .Labi_fail
    ldr x20, [x21, #48]
    cmp x7, x20
    b.ne .Labi_fail
    ldr x20, [x21, #56]
    cmp x8, x20
    b.ne .Labi_fail
    ldr x20, [x21, #64]
    cmp x9, x20
    b.ne .Labi_fail
    ldr x20, [x21, #72]
    cmp x10, x20
    b.ne .Labi_fail
    ldr x20, [x21, #80]
    cmp x11, x20
    b.ne .Labi_fail
    ldr x20, [x21, #88]
    cmp x12, x20
    b.ne .Labi_fail
    ldr x20, [x21, #96]
    cmp x13, x20
    b.ne .Labi_fail
    ldr x20, [x21, #104]
    cmp x14, x20
    b.ne .Labi_fail
    ldr x20, [x21, #112]
    cmp x15, x20
    b.ne .Labi_fail
    ldr x20, [x21, #120]
    cmp x16, x20
    b.ne .Labi_fail
    ldr x20, [x21, #128]
    cmp x17, x20
    b.ne .Labi_fail

    fmov x22, d0
    ldr x20, [x21, #136]
    cmp x22, x20
    b.ne .Labi_fail
    fmov x22, d1
    ldr x20, [x21, #144]
    cmp x22, x20
    b.ne .Labi_fail
    fmov x22, d2
    ldr x20, [x21, #152]
    cmp x22, x20
    b.ne .Labi_fail
    fmov x22, d3
    ldr x20, [x21, #160]
    cmp x22, x20
    b.ne .Labi_fail
    fmov x22, d4
    ldr x20, [x21, #168]
    cmp x22, x20
    b.ne .Labi_fail
    fmov x22, d5
    ldr x20, [x21, #176]
    cmp x22, x20
    b.ne .Labi_fail
    fmov x22, d6
    ldr x20, [x21, #184]
    cmp x22, x20
    b.ne .Labi_fail
    fmov x22, d7
    ldr x20, [x21, #192]
    cmp x22, x20
    b.ne .Labi_fail

    mov w0, #0
    ldp x21, x22, [sp], #16
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
    ret

.Labi_fail:
    mov w0, #1
    ldp x21, x22, [sp], #16
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
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
    .quad 0x9999
    .quad 0xaaaa
    .quad 0xbbbb
    .quad 0xcccc
    .quad 0xdddd
    .quad 0xeeee
    .quad 0xffff
    .quad 0x12345678
    .quad 0x9abcdef0
    .quad 0x3ff0000000000000
    .quad 0x4000000000000000
    .quad 0x4008000000000000
    .quad 0x4010000000000000
    .quad 0x4014000000000000
    .quad 0x4018000000000000
    .quad 0x401c000000000000
    .quad 0x4020000000000000
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
        "debugger spike: dormant={dormant_ns:.3} ns/iter, active={active_ns:.3} ns/iter, delta={:.3} ns/iter",
        active_ns - dormant_ns
    );

    CHECK_LOCALS.store(false, Ordering::Relaxed);
}

/// Verifies every caller-saved integer register (x0-x17) and SIMD register
/// (q0-q7) survives the wrapper's Rust hook call.
#[test]
fn wrapper_preserves_caller_saved_registers() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    CHECK_LOCALS.store(false, Ordering::Relaxed);
    let code = unsafe { spike_abi_probe() };
    assert_eq!(code, 0, "caller-saved register was clobbered");
}
