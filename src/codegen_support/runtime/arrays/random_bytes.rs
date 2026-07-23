//! Purpose:
//! Emits the `__rt_random_bytes` runtime helper assembly for PHP's `random_bytes()`.
//! Allocates an owned binary string and fills it with cryptographically secure bytes,
//! keeping each supported target's CSPRNG source and ABI in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - `random_bytes()` is a CSPRNG: this helper never falls back to a weaker source. macOS
//!   uses libc `arc4random_buf`; Linux uses the `getrandom` syscall; the x86_64 form also
//!   serves Windows through the syscall→shim transform (`__rt_sys_getrandom` = BCryptGenRandom).
//! - A length below 1 or an unavailable entropy source aborts with a fatal diagnostic and
//!   `exit(1)` rather than returning weak or empty output.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::platform::Platform;
use crate::codegen::runtime::data::{RANDOM_BYTES_LENGTH_MSG, RANDOM_BYTES_SOURCE_MSG};

/// Emits the `__rt_random_bytes` runtime helper for `random_bytes(int $length): string`.
///
/// Dispatches to the x86_64 variant (which serves both Linux and Windows) on x86_64
/// targets; otherwise emits the macOS or Linux AArch64 variant. Windows AArch64 is not
/// a supported target and panics.
///
/// # Input
/// - AArch64: requested byte length in `x0`.
/// - x86_64: requested byte length in `rdi` (SysV first argument).
///
/// # Output (an owned, kind-1 elephc string)
/// - AArch64: payload pointer in `x1`, length in `x2`.
/// - x86_64: payload pointer in `rax`, length in `rdx`.
pub fn emit_random_bytes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_random_bytes_x86_64(emitter);
        return;
    }

    match emitter.platform {
        Platform::MacOS => emit_random_bytes_macos_aarch64(emitter),
        Platform::Linux => emit_random_bytes_linux_aarch64(emitter),
        Platform::Windows => {
            panic!("Windows ARM64 target is not yet supported (see issue #379)");
        }
    }
}

/// Emits the macOS AArch64 `__rt_random_bytes` helper backed by libc `arc4random_buf`.
///
/// `arc4random_buf` fills the whole buffer in one call and never fails, so no retry loop
/// is needed. A length below 1 is rejected before allocation.
fn emit_random_bytes_macos_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_bytes ---");
    emitter.label_global("__rt_random_bytes");

    // -- reject non-positive lengths before allocating --
    emitter.instruction("cmp x0, #1");                                          // check whether the requested length is below 1
    emitter.instruction("b.lt __rt_random_bytes_bad_length");                   // reject 0 and negative lengths with a fatal error

    // -- set up a stack frame for the two libc calls --
    emitter.instruction("sub sp, sp, #32");                                     // reserve slots for the length, result pointer, and saved frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the libc calls
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested length for the fill call and the return pair

    // -- allocate an owned string buffer of `length` bytes --
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate length bytes of owned storage (size already in x0)
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a string payload
    emitter.instruction("str x0, [sp, #8]");                                    // save the result payload pointer for the return pair

    // -- fill the buffer with cryptographically secure bytes --
    emitter.instruction("ldr x1, [sp, #0]");                                    // arc4random_buf length argument (buffer already in x0)
    emitter.bl_c("arc4random_buf");                                             // fill the buffer via libc CSPRNG; never fails

    // -- return the owned string pointer/length pair --
    emitter.instruction("ldr x1, [sp, #8]");                                    // return the result payload pointer
    emitter.instruction("ldr x2, [sp, #0]");                                    // return the result length
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the CSPRNG string

    emit_aarch64_bad_length_fatal(emitter);
}

/// Emits the Linux AArch64 `__rt_random_bytes` helper backed by the `getrandom` syscall.
///
/// Keeps the buffer cursor and remaining count in callee-saved registers, reloads the
/// syscall number and arguments each iteration, advances on a partial fill, retries on
/// `-EINTR`, and aborts on any other negative return or if a sanity iteration cap is hit.
fn emit_random_bytes_linux_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_bytes ---");
    emitter.label_global("__rt_random_bytes");

    // -- reject non-positive lengths before allocating --
    emitter.instruction("cmp x0, #1");                                          // check whether the requested length is below 1
    emitter.instruction("b.lt __rt_random_bytes_bad_length");                   // reject 0 and negative lengths with a fatal error

    // -- set up a frame preserving the callee-saved loop registers (this helper returns) --
    emitter.instruction("sub sp, sp, #64");                                     // reserve the loop-register save area and frame slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve the caller's x19 (cursor) and x20 (remaining)
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve the caller's x21 (base) and x22 (length)
    emitter.instruction("str x23, [sp, #32]");                                  // preserve the caller's x23 (iteration cap)

    // -- allocate an owned string buffer of `length` bytes --
    emitter.instruction("str x0, [sp, #40]");                                   // spill length to the free frame slot across the alloc call (do not trust heap_alloc register discipline)
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate length bytes of owned storage (size already in x0)
    emitter.instruction("ldr x22, [sp, #40]");                                  // reload the requested length after the alloc call
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a string payload
    emitter.instruction("mov x21, x0");                                         // result payload base pointer
    emitter.instruction("mov x19, x0");                                         // buffer cursor = payload start
    emitter.instruction("mov x20, x22");                                        // remaining bytes = length
    emitter.instruction("mov x23, #256");                                       // sanity iteration cap for pathological partial fills

    // -- fill loop: getrandom may return fewer bytes or -EINTR --
    emitter.label("__rt_random_bytes_fill");
    emitter.instruction("cbz x20, __rt_random_bytes_done");                     // done once every requested byte is filled
    emitter.instruction("subs x23, x23, #1");                                   // consume one iteration from the sanity cap
    emitter.instruction("b.lt __rt_random_bytes_source_fail");                  // too many iterations → treat as a source failure
    emitter.instruction("mov x0, x19");                                         // getrandom buffer = current cursor
    emitter.instruction("mov x1, x20");                                         // getrandom length = remaining bytes
    emitter.instruction("mov x2, #0");                                          // getrandom flags = 0
    emitter.instruction("mov x8, #278");                                        // Linux aarch64 getrandom syscall number
    emitter.instruction("svc #0");                                              // request random bytes from the kernel CSPRNG
    emitter.instruction("cmp x0, #0");                                          // did getrandom return a negative errno?
    emitter.instruction("b.lt __rt_random_bytes_check_eintr");                  // negative → distinguish EINTR from a hard failure
    emitter.instruction("add x19, x19, x0");                                    // advance the cursor by the bytes filled
    emitter.instruction("sub x20, x20, x0");                                    // decrement the remaining byte count
    emitter.instruction("b __rt_random_bytes_fill");                            // continue until the buffer is full

    // -- classify a negative getrandom return --
    emitter.label("__rt_random_bytes_check_eintr");
    emitter.instruction("cmn x0, #4");                                          // was the syscall interrupted (errno -EINTR)?
    emitter.instruction("b.eq __rt_random_bytes_fill");                         // retry on interruption
    emitter.instruction("b __rt_random_bytes_source_fail");                     // any other negative → fatal source failure

    // -- return the owned string pointer/length pair --
    emitter.label("__rt_random_bytes_done");
    emitter.instruction("mov x1, x21");                                         // return the result payload pointer
    emitter.instruction("mov x2, x22");                                         // return the result length
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore the caller's x19 and x20
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the caller's x21 and x22
    emitter.instruction("ldr x23, [sp, #32]");                                  // restore the caller's x23
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the CSPRNG string

    emit_aarch64_bad_length_fatal(emitter);
    emit_aarch64_source_fail_fatal(emitter);
}

/// Emits the shared AArch64 fatal path for a `random_bytes()` length below 1.
///
/// Writes the invalid-length diagnostic to stderr and terminates the process. Uses the
/// platform-aware `syscall` helper so it lowers correctly on macOS and Linux.
fn emit_aarch64_bad_length_fatal(emitter: &mut Emitter) {
    emitter.label("__rt_random_bytes_bad_length");
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the invalid-length diagnostic
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_random_bytes_length_msg");
    emitter.instruction(&format!("mov x2, #{}", RANDOM_BYTES_LENGTH_MSG.len())); // pass the exact diagnostic byte count
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1 for the invalid-length abort
    emitter.syscall(1);
}

/// Emits the AArch64 fatal path for an unavailable `random_bytes()` entropy source.
///
/// Writes the CSPRNG-unavailable diagnostic to stderr and terminates the process rather
/// than returning weak or partially filled output.
fn emit_aarch64_source_fail_fatal(emitter: &mut Emitter) {
    emitter.label("__rt_random_bytes_source_fail");
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the entropy-source diagnostic
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_random_bytes_source_msg");
    emitter.instruction(&format!("mov x2, #{}", RANDOM_BYTES_SOURCE_MSG.len())); // pass the exact diagnostic byte count
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1 for the entropy-source abort
    emitter.syscall(1);
}

/// Emits the x86_64 `__rt_random_bytes` helper, serving both Linux and Windows.
///
/// Uses the Linux `getrandom` syscall form (`mov eax, 318` + `syscall`); on Windows the
/// syscall→shim transform rewrites that adjacent pair into `call __rt_sys_getrandom`
/// (BCryptGenRandom), which fills the whole buffer in one call. Loop state lives in
/// callee-saved registers (r12/r13) because the Windows shim clobbers rdi/rsi, and the
/// syscall arguments are reloaded every iteration. A length below 1, an iteration-cap
/// overflow, or any hard failure (a negative return other than `-EINTR`, including the
/// shim's `-1`) aborts with a fatal diagnostic and `exit(1)`.
fn emit_random_bytes_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_bytes ---");
    emitter.label_global("__rt_random_bytes");

    // -- prologue: preserve callee-saved registers (this helper returns) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push rbx");                                            // preserve rbx (iteration cap counter)
    emitter.instruction("push r12");                                            // preserve r12 (buffer cursor)
    emitter.instruction("push r13");                                            // preserve r13 (remaining bytes)
    emitter.instruction("push r14");                                            // preserve r14 (result base pointer)
    emitter.instruction("push r15");                                            // preserve r15 (result length)
    emitter.instruction("sub rsp, 8");                                          // realign the stack to 16 bytes before nested calls

    // -- reject non-positive lengths before allocating --
    emitter.instruction("test rdi, rdi");                                       // inspect the requested length
    emitter.instruction("jle __rt_random_bytes_bad_length_x86");                // reject 0 and negative lengths with a fatal error

    // -- allocate an owned string buffer of `length` bytes --
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // spill length to the reserved slot across the alloc call (do not trust heap_alloc register discipline)
    emitter.instruction("mov rax, rdi");                                        // allocation size = length
    emitter.instruction("call __rt_heap_alloc");                                // allocate length bytes of owned storage (pointer in rax)
    emitter.instruction("mov r15, QWORD PTR [rsp]");                            // reload the requested length after the alloc call
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
    ));                                                                          // owned-string heap kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocation as a string payload
    emitter.instruction("mov r14, rax");                                        // save the result payload base pointer
    emitter.instruction("mov r12, rax");                                        // buffer cursor = payload start
    emitter.instruction("mov r13, r15");                                        // remaining bytes = length
    emitter.instruction("mov ebx, 256");                                        // sanity iteration cap for pathological partial fills

    // -- fill loop: getrandom may return fewer bytes or -EINTR (Windows fills in one call) --
    emitter.label("__rt_random_bytes_fill_x86");
    emitter.instruction("test r13, r13");                                       // any bytes left to fill?
    emitter.instruction("jz __rt_random_bytes_done_x86");                       // done once the buffer is full
    emitter.instruction("dec rbx");                                             // consume one iteration from the sanity cap
    emitter.instruction("js __rt_random_bytes_source_fail_x86");                // too many iterations → treat as a source failure
    emitter.instruction("mov rdi, r12");                                        // getrandom buffer = current cursor (reloaded; the shim clobbers rdi)
    emitter.instruction("mov rsi, r13");                                        // getrandom length = remaining bytes (reloaded; the shim clobbers rsi)
    emitter.instruction("xor edx, edx");                                        // getrandom flags = 0
    emitter.instruction("mov eax, 318");                                        // Linux x86_64 getrandom (Windows: → call __rt_sys_getrandom)
    emitter.instruction("syscall");                                             // request random bytes from the CSPRNG
    emitter.instruction("test rax, rax");                                       // did getrandom return a negative errno?
    emitter.instruction("js __rt_random_bytes_check_eintr_x86");                // negative → distinguish EINTR from a hard failure
    emitter.instruction("add r12, rax");                                        // advance the cursor by the bytes filled
    emitter.instruction("sub r13, rax");                                        // decrement the remaining byte count
    emitter.instruction("jmp __rt_random_bytes_fill_x86");                      // continue until the buffer is full

    // -- classify a negative getrandom return --
    emitter.label("__rt_random_bytes_check_eintr_x86");
    emitter.instruction("cmp rax, -4");                                         // was the syscall interrupted (errno -EINTR)?
    emitter.instruction("je __rt_random_bytes_fill_x86");                       // retry on interruption
    emitter.instruction("jmp __rt_random_bytes_source_fail_x86");               // any other negative (incl the shim's -1) → fatal

    // -- return the owned string pointer/length pair --
    emitter.label("__rt_random_bytes_done_x86");
    emitter.instruction("mov rax, r14");                                        // return the result payload pointer
    emitter.instruction("mov rdx, r15");                                        // return the result length
    emitter.instruction("add rsp, 8");                                          // release the alignment padding
    emitter.instruction("pop r15");                                             // restore r15
    emitter.instruction("pop r14");                                             // restore r14
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the CSPRNG string

    // -- fatal: length below 1 --
    emitter.label("__rt_random_bytes_bad_length_x86");
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_random_bytes_length_msg");
    emitter.instruction(&format!("mov rdx, {}", RANDOM_BYTES_LENGTH_MSG.len())); // pass the exact diagnostic byte count
    emitter.instruction("jmp __rt_random_bytes_fatal_write_x86");               // write the diagnostic and exit

    // -- fatal: entropy source unavailable --
    emitter.label("__rt_random_bytes_source_fail_x86");
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_random_bytes_source_msg");
    emitter.instruction(&format!("mov rdx, {}", RANDOM_BYTES_SOURCE_MSG.len())); // pass the exact diagnostic byte count
    emitter.instruction("jmp __rt_random_bytes_fatal_write_x86");               // write the diagnostic and exit

    // -- shared fatal writer: stderr message (rsi/rdx) then exit(1) --
    emitter.label("__rt_random_bytes_fatal_write_x86");
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the fatal diagnostic
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 write (Windows: → call __rt_sys_write)
    emitter.instruction("syscall");                                             // emit the fatal diagnostic before terminating
    emitter.instruction("mov edi, 1");                                          // exit code 1 for the abort path
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 exit (Windows: → call __rt_sys_exit)
    emitter.instruction("syscall");                                             // terminate the process after reporting the failure
}
