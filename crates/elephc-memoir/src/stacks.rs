//! Purpose:
//! Emits guarded stack allocation and mapping retirement.
//!
//! Called from:
//! - Compiler fiber runtime orchestration.
//!
//! Key details:
//! - The host supplies anonymous private mapping flags for its target platform.

use crate::{abi, Arch, Emitter};

/// Size of the unmapped (PROT_NONE) guard region at the bottom of every fiber
/// stack. Set to 16 KB so the guard fully covers a single page on every
/// supported target; macOS aarch64 uses 16 KB pages, Linux aarch64 typically
/// uses 4 KB but accepts oversized protection ranges silently.
pub const FIBER_GUARD_PAGE_SIZE: i32 = 16384;

/// __rt_fiber_alloc_stack: reserve a usable stack with a guard page.
/// Input:  x0 = requested usable stack size in bytes
/// Output: x0 = stack_base (low address; the mmap region start, includes the guard page)
///         x1 = stack_top  (high address, initial SP, 16-byte aligned)
///         x2 = total mmap'd length in bytes (usable size + guard page); needed for munmap
pub fn emit_fiber_alloc_stack(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_alloc_stack_x86_64(emitter);
        return;
    }

    let map_flags = emitter.anonymous_private_map_flags();

    emitter.blank();
    emitter.comment("--- runtime: fiber_alloc_stack (mmap + guard page) ---");
    emitter.label_global("__rt_fiber_alloc_stack");

    // -- prologue: save x19/x20/x21 (callee-saved) since we use them as scratch --
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame plus three saved-callee slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp]");                                  // preserve caller's x19/x20; used to remember alloc state across libc calls
    emitter.instruction("str x21, [sp, #16]");                                  // preserve caller's x21; used to hold the mmap base across mprotect
    emitter.instruction("add x29, sp, #32");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = requested usable stack size (callee-saved across mmap/mprotect)

    // -- mmap(NULL, requested_size + GUARD, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1, 0) --
    emitter.instruction("mov x0, #0");                                          // addr = NULL; let the kernel pick the address
    emitter.instruction(&format!("add x1, x19, #{}", FIBER_GUARD_PAGE_SIZE));   // length = usable size + guard page
    emitter.instruction("mov x20, x1");                                         // x20 = total mapped length (callee-saved across mprotect)
    emitter.instruction("mov x2, #3");                                          // prot = PROT_READ | PROT_WRITE
    emitter.instruction(&format!("mov x3, #{}", map_flags));                    // flags = MAP_PRIVATE | MAP_ANON for this platform
    emitter.instruction("mov x4, #-1");                                         // fd = -1 for anonymous mappings
    emitter.instruction("mov x5, #0");                                          // offset = 0 for anonymous mappings
    emitter.bl_c("mmap");                                                       // x0 = mapping base on success, MAP_FAILED (-1) on failure

    // -- bail out cleanly when mmap returns MAP_FAILED so callers see (0, 0, 0) --
    emitter.instruction("cmn x0, #1");                                          // is x0 == -1 (MAP_FAILED)?
    emitter.instruction("b.eq __rt_fiber_alloc_stack_fail");                    // skip mprotect and return zeros when mmap failed

    // -- mprotect(base, GUARD_PAGE_SIZE, PROT_NONE) installs the guard at the bottom --
    emitter.instruction("mov x21, x0");                                         // x21 = mapping base (callee-saved across mprotect; libc preserves x19-x28)
    emitter.instruction(&format!("mov x1, #{}", FIBER_GUARD_PAGE_SIZE));        // length = one guard page
    emitter.instruction("mov x2, #0");                                          // prot = PROT_NONE; touching the guard faults via SIGSEGV
    emitter.bl_c("mprotect");                                                   // ignore the return value: a failure here would still leave a usable stack

    // -- compute stack_top = mapping base + total length, aligned down to 16 --
    emitter.instruction("add x1, x21, x20");                                    // x1 = end of mapped region (one past the last usable byte)
    emitter.instruction("and x1, x1, #-16");                                    // round stack_top down to a 16-byte boundary for AArch64 SP alignment

    // -- pack outputs: x0 = base, x1 = top, x2 = total length for munmap --
    emitter.instruction("mov x0, x21");                                         // x0 = mapping base (also serves as stack_base for free)
    emitter.instruction("mov x2, x20");                                         // x2 = total mapped length

    // -- epilogue --
    emitter.instruction("ldr x21, [sp, #16]");                                  // restore caller's x21
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore caller's x19/x20
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the allocator scratch frame
    emitter.instruction("ret");                                                 // hand the (base, top, length) triple back to the constructor

    // -- mmap failure path: report a clean (0, 0, 0) so the constructor can detect it --
    emitter.label("__rt_fiber_alloc_stack_fail");
    emitter.instruction("mov x0, #0");                                          // stack_base = 0 indicates the alloc failed
    emitter.instruction("mov x1, #0");                                          // stack_top = 0 mirrors the failure signal
    emitter.instruction("mov x2, #0");                                          // total length = 0 so a defensive free is a no-op
    emitter.instruction("ldr x21, [sp, #16]");                                  // restore caller's x21
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore caller's x19/x20
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the allocator scratch frame
    emitter.instruction("ret");                                                 // bail out with the failure triple
}

/// __rt_fiber_free_stack: return a fiber stack to the kernel via munmap.
/// Input:  x0 = stack_base (mapping base returned by __rt_fiber_alloc_stack)
///         x1 = total mapped length (the third return value of alloc, stored in stack_size)
pub fn emit_fiber_free_stack(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_free_stack_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fiber_free_stack (munmap) ---");
    emitter.label_global("__rt_fiber_free_stack");

    emitter.instruction("cbz x0, __rt_fiber_free_stack_done");                  // skip NULL bases (alloc failure or already freed)
    emitter.instruction("cbz x1, __rt_fiber_free_stack_done");                  // skip zero-length mappings as a defensive guard against double-free
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save the caller's frame pointer and return address before invoking libc
    emitter.bl_c("munmap");                                                     // ignore the return value: the stack is gone either way
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller's frame pointer and return address
    emitter.label("__rt_fiber_free_stack_done");
    emitter.instruction("ret");                                                 // hand control back to the caller (NULL/zero-length path also lands here)
}

/// x86_64-specific implementation of `__rt_fiber_alloc_stack`.
/// Uses the System V AMD64 ABI convention for mmap/mprotect calls.
/// Mirrors the ARM64 path in `emit_fiber_alloc_stack` but emits x86_64
/// instructions and preserves callee-saved registers (r12-r15, rbp) across
/// libc calls.
fn emit_alloc_stack_x86_64(emitter: &mut impl Emitter) {
    let map_flags = emitter.anonymous_private_map_flags();

    emitter.blank();
    emitter.comment("--- runtime: fiber_alloc_stack (mmap + guard page) ---");
    emitter.label_global("__rt_fiber_alloc_stack");

    // -- prologue: save callee-saved registers used across libc calls --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer for the allocator helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base while libc calls run
    emitter.instruction("push r12");                                            // preserve the requested usable size across mmap and mprotect
    emitter.instruction("push r13");                                            // preserve the total mapped length across mprotect
    emitter.instruction("push r14");                                            // preserve the mapping base across mprotect
    emitter.instruction("push r15");                                            // keep the SysV stack aligned while saving three live registers
    emitter.instruction("mov r12, rdi");                                        // r12 = requested usable stack size

    // -- mmap(NULL, requested_size + GUARD, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1, 0) --
    emitter.instruction("xor edi, edi");                                        // addr = NULL; let the kernel pick the address
    emitter.instruction(&format!("lea rsi, [r12 + {}]", FIBER_GUARD_PAGE_SIZE)); // length = usable size + guard page
    emitter.instruction("mov r13, rsi");                                        // r13 = total mapped length for the eventual munmap
    emitter.instruction("mov edx, 3");                                          // prot = PROT_READ | PROT_WRITE
    emitter.instruction(&format!("mov ecx, {}", map_flags));                    // flags = MAP_PRIVATE | MAP_ANON for this platform
    emitter.instruction("mov r8, -1");                                          // fd = -1 for anonymous mappings
    emitter.instruction("xor r9d, r9d");                                        // offset = 0 for anonymous mappings
    emitter.bl_c("mmap");                                                       // rax = mapping base on success, MAP_FAILED (-1) on failure

    // -- bail out cleanly when mmap returns MAP_FAILED so callers see (0, 0, 0) --
    emitter.instruction("cmp rax, -1");                                         // is rax == MAP_FAILED?
    emitter.instruction("je __rt_fiber_alloc_stack_fail");                      // skip mprotect and return zeros when mmap failed

    // -- mprotect(base, GUARD_PAGE_SIZE, PROT_NONE) installs the guard at the bottom --
    emitter.instruction("mov r14, rax");                                        // r14 = mapping base preserved across mprotect
    emitter.instruction("mov rdi, r14");                                        // base = mapping start for the guard region
    emitter.instruction(&format!("mov esi, {}", FIBER_GUARD_PAGE_SIZE));        // length = one guard page
    emitter.instruction("xor edx, edx");                                        // prot = PROT_NONE; touching the guard faults via SIGSEGV
    emitter.bl_c("mprotect");                                                   // ignore the return value: a failure still leaves a usable stack

    // -- compute stack_top = mapping base + total length, aligned down to 16 --
    emitter.instruction("lea rdx, [r14 + r13]");                                // rdx = end of mapped region, one byte past the usable stack
    emitter.instruction("and rdx, -16");                                        // round stack_top down to a 16-byte boundary for SysV calls

    // -- pack outputs: rax = base, rdx = top, rcx = total length for munmap --
    emitter.instruction("mov rax, r14");                                        // rax = mapping base, also used as stack_base for free
    emitter.instruction("mov rcx, r13");                                        // rcx = total mapped length

    // -- epilogue --
    emitter.instruction("pop r15");                                             // restore the alignment-preserving callee-saved spill register
    emitter.instruction("pop r14");                                             // restore the caller's r14
    emitter.instruction("pop r13");                                             // restore the caller's r13
    emitter.instruction("pop r12");                                             // restore the caller's r12
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // hand the (base, top, length) triple back to the constructor

    // -- mmap failure path: report a clean (0, 0, 0) so the constructor can detect it --
    emitter.label("__rt_fiber_alloc_stack_fail");
    emitter.instruction("xor eax, eax");                                        // stack_base = 0 indicates the alloc failed
    emitter.instruction("xor edx, edx");                                        // stack_top = 0 mirrors the failure signal
    emitter.instruction("xor ecx, ecx");                                        // total length = 0 so a defensive free is a no-op
    emitter.instruction("pop r15");                                             // restore the alignment-preserving callee-saved spill register
    emitter.instruction("pop r14");                                             // restore the caller's r14
    emitter.instruction("pop r13");                                             // restore the caller's r13
    emitter.instruction("pop r12");                                             // restore the caller's r12
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // bail out with the failure triple
}

/// x86_64-specific implementation of `__rt_fiber_free_stack`.
/// Uses the System V AMD64 ABI convention for the munmap call.
/// Preserves rbp around the libc call to maintain stack alignment.
fn emit_free_stack_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_free_stack (munmap) ---");
    emitter.label_global("__rt_fiber_free_stack");

    emitter.instruction("test rdi, rdi");                                       // skip NULL bases (alloc failure or already freed)
    emitter.instruction("jz __rt_fiber_free_stack_done");                       // no mapping base means there is nothing to unmap
    emitter.instruction("test rsi, rsi");                                       // skip zero-length mappings as a defensive guard
    emitter.instruction("jz __rt_fiber_free_stack_done");                       // no mapped length means there is nothing to unmap
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before invoking libc
    emitter.instruction("mov rbp, rsp");                                        // keep the SysV stack aligned for the munmap call
    emitter.bl_c("munmap");                                                     // ignore the return value: the stack is gone either way
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after munmap
    emitter.label("__rt_fiber_free_stack_done");
    emitter.instruction("ret");                                                 // hand control back to the caller
}

/// Symbol holding the low-water stack address for the currently running context.
/// Zero means "guard disabled"; every prologue check is an unsigned compare against it.
pub const STACK_LIMIT_SYMBOL: &str = "_stack_limit";

/// Symbol holding the OS-thread (non-fiber) stack floor, used to restore
/// `_stack_limit` when a fiber switch returns to the main context.
pub const STACK_LIMIT_MAIN_SYMBOL: &str = "_stack_limit_main";


/// Publishes the current stack's floor as seen by native function prologues.
pub fn emit_publish_active_floor(emitter: &mut impl crate::TargetAbi, floor: &str) {
    emitter.emit_store_reg_to_symbol(floor, STACK_LIMIT_SYMBOL, 0);
}

/// Publishes a newly measured main stack floor to both slots.
pub fn emit_publish_main_floor(emitter: &mut impl crate::TargetAbi, floor: &str) {
    emit_publish_active_floor(emitter, floor);
    emitter.emit_store_reg_to_symbol(floor, STACK_LIMIT_MAIN_SYMBOL, 0);
}

/// Restores the remembered thread stack floor after leaving a coroutine stack.
pub fn emit_restore_main_floor(emitter: &mut impl crate::TargetAbi, scratch: &str) {
    emitter.emit_load_symbol_to_reg(scratch, STACK_LIMIT_MAIN_SYMBOL, 0);
    emit_publish_active_floor(emitter, scratch);
}

/// Places floors near text so AArch64 prologues can reach them even with large arenas.
pub fn append_stack_floor_data(out: &mut String) {
    for symbol in [STACK_LIMIT_SYMBOL, STACK_LIMIT_MAIN_SYMBOL] {
        out.push_str(&format!(".p2align 3\n.globl {symbol}\n{symbol}:\n    .quad 0\n"));
    }
}

/// Bytes kept in reserve below the published limit.
///
/// The prologue check runs *after* the frame has been reserved, so this margin only has to
/// cover what a single guarded frame can still consume before the next guarded call:
/// outgoing stack-argument areas, `__rt_*` helper frames, and the libc calls those make.
/// 32 KiB is far above any of those, costs 0.4% of a default 8 MiB OS stack, and stays at
/// one eighth of the 256 KiB coroutine stack a Fiber or Generator body runs on, the one
/// place where an over-generous reserve would visibly cut the usable recursion depth.
pub const STACK_GUARD_RESERVE_BYTES: i64 = 32 * 1024;

/// Upper clamp on the measured stack budget.
///
/// `getrlimit` reports `RLIM_INFINITY` for an unlimited stack (`0xFFFF_FFFF_FFFF_FFFF` on
/// Linux, `0x7FFF_FFFF_FFFF_FFFF` on macOS). Clamping keeps the computed floor a real
/// address instead of wrapping below zero; the cost is that a genuinely unlimited stack
/// reports the fatal after 64 MiB instead of running until the OS refuses to grow it.
const STACK_BUDGET_CAP_BYTES: i64 = 64 * 1024 * 1024;

/// Budget used when `getrlimit` fails outright (8 MiB, the default on both platforms).
const STACK_BUDGET_FALLBACK_BYTES: i64 = 8 * 1024 * 1024;

/// Smallest budget worth guarding. Below this the reserve would swallow the whole stack,
/// so the guard is disabled instead of publishing a floor that is effectively at the
/// current stack pointer.
const STACK_BUDGET_MIN_BYTES: i64 = 256 * 1024;

/// Emits `__rt_stack_limit_init`, which measures the running OS stack once at process
/// start and publishes the resulting floor into `_stack_limit` and `_stack_limit_main`.
///
/// Takes no arguments and returns nothing. Clobbers the caller-saved registers a plain
/// `getrlimit(RLIMIT_STACK, &rlimit)` call clobbers, so callers must invoke it before any
/// live argument register matters (the `main` prologue calls it after argc/argv have been
/// stored to globals).
///
/// The published floor is `entry_sp - min(rlim_cur, 64 MiB) + 32 KiB`. When `getrlimit`
/// fails, the reported limit is implausibly small, or the subtraction would wrap, zero is
/// published instead and the guard stays inert for the whole process.
pub fn emit_stack_limit_init(emitter: &mut impl Emitter, resource: i64) {
    emitter.blank();
    emitter.comment("--- runtime: stack_limit_init (publish the call-stack floor) ---");
    emitter.label_global("__rt_stack_limit_init");
    match emitter.arch() {
        Arch::AArch64 => emit_stack_limit_init_aarch64(emitter, resource),
        Arch::X86_64 => emit_stack_limit_init_x86_64(emitter, resource),
    }
}

/// AArch64 implementation of `__rt_stack_limit_init` (macOS and Linux share it).
///
/// Reserves a 32-byte frame: the low 16 bytes are the `struct rlimit` output buffer and
/// the high 16 bytes hold the saved x29/x30 pair. x29 doubles as the "stack top" reference
/// because it is callee-saved and therefore survives the `getrlimit` call unchanged.
fn emit_stack_limit_init_aarch64(emitter: &mut impl Emitter, resource: i64) {
    emitter.instruction("sub sp, sp, #32");                                     // reserve the rlimit output buffer plus this helper's frame footer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save the caller frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // anchor the frame pointer and remember it as the stack-top reference

    // -- getrlimit(RLIMIT_STACK, &rlimit) --
    emitter.instruction(&format!("mov x0, #{}", resource));                     // resource = RLIMIT_STACK
    emitter.instruction("mov x1, sp");                                          // destination = the 16-byte rlimit buffer at the bottom of this frame
    emitter.bl_c("getrlimit");                                                  // x0 = 0 on success, -1 on failure

    // -- pick the budget: rlim_cur on success, the platform default otherwise --
    abi::emit_load_int_immediate(emitter, "x1", STACK_BUDGET_FALLBACK_BYTES);
    emitter.instruction("cbnz x0, __rt_stack_limit_init_clamp");                // keep the fallback budget when getrlimit reported a failure
    emitter.instruction("ldr x1, [sp]");                                        // x1 = rlim_cur, the soft stack limit in bytes

    // -- clamp the budget into a range that yields a real address --
    emitter.label("__rt_stack_limit_init_clamp");
    abi::emit_load_int_immediate(emitter, "x2", STACK_BUDGET_CAP_BYTES);
    emitter.instruction("cmp x1, x2");                                          // is the reported budget above the cap (or RLIM_INFINITY)?
    emitter.instruction("csel x1, x1, x2, lo");                                 // x1 = min(budget, cap) using an unsigned comparison
    abi::emit_load_int_immediate(emitter, "x2", STACK_BUDGET_MIN_BYTES);
    emitter.instruction("cmp x1, x2");                                          // is the budget too small to be worth guarding?
    emitter.instruction("b.lo __rt_stack_limit_init_disable");                  // yes, leave the guard disabled rather than publish a bogus floor

    // -- floor = entry stack pointer - (budget - reserve) --
    abi::emit_load_int_immediate(emitter, "x2", STACK_GUARD_RESERVE_BYTES);
    emitter.instruction("sub x1, x1, x2");                                      // subtract the reserve kept for helper frames and the fatal path
    emitter.instruction("sub x0, x29, x1");                                     // x0 = the lowest stack address compiled prologues may reach
    emitter.instruction("cmp x0, x29");                                         // did the subtraction wrap below address zero?
    emitter.instruction("b.hs __rt_stack_limit_init_disable");                  // yes, an unusable floor, so disable the guard instead
    emitter.instruction("b __rt_stack_limit_init_store");                       // publish the computed floor

    // -- disabled: publish zero so every prologue compare passes --
    emitter.label("__rt_stack_limit_init_disable");
    emitter.instruction("mov x0, #0");                                          // zero disables the guard for the rest of the process

    emitter.label("__rt_stack_limit_init_store");
    emit_publish_main_floor(emitter, "x0");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the rlimit buffer and frame footer
    emitter.instruction("ret");                                                 // return to the process entry prologue
}

/// x86_64 (Linux) implementation of `__rt_stack_limit_init`.
///
/// Mirrors the AArch64 path with SysV registers. `rbp` is the stack-top reference because
/// it is callee-saved and therefore preserved across the `getrlimit` call; the 16 bytes
/// below it are the `struct rlimit` output buffer, which also keeps `rsp` 16-byte aligned
/// at the call site.
fn emit_stack_limit_init_x86_64(emitter: &mut impl Emitter, resource: i64) {
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // anchor the frame pointer and remember it as the stack-top reference
    emitter.instruction("sub rsp, 16");                                         // reserve the 16-byte rlimit output buffer, keeping rsp 16-byte aligned

    // -- getrlimit(RLIMIT_STACK, &rlimit) --
    emitter.instruction(&format!("mov edi, {}", resource));                     // resource = RLIMIT_STACK
    emitter.instruction("mov rsi, rsp");                                        // destination = the 16-byte rlimit buffer below the frame pointer
    emitter.bl_c("getrlimit");                                                  // eax = 0 on success, -1 on failure

    // -- pick the budget: rlim_cur on success, the platform default otherwise --
    emitter.instruction(&format!("mov rcx, {}", STACK_BUDGET_FALLBACK_BYTES));  // preload the fallback budget without disturbing the result flags
    emitter.instruction("test eax, eax");                                       // did getrlimit succeed?
    emitter.instruction("jnz __rt_stack_limit_init_clamp");                     // keep the fallback budget when getrlimit reported a failure
    emitter.instruction("mov rcx, QWORD PTR [rsp]");                            // rcx = rlim_cur, the soft stack limit in bytes

    // -- clamp the budget into a range that yields a real address --
    emitter.label("__rt_stack_limit_init_clamp");
    emitter.instruction(&format!("mov rdx, {}", STACK_BUDGET_CAP_BYTES));       // materialize the budget cap for the clamp comparison
    emitter.instruction("cmp rcx, rdx");                                        // is the reported budget above the cap (or RLIM_INFINITY)?
    emitter.instruction("cmova rcx, rdx");                                      // rcx = min(budget, cap) using an unsigned comparison
    emitter.instruction(&format!("mov rdx, {}", STACK_BUDGET_MIN_BYTES));       // materialize the smallest budget worth guarding
    emitter.instruction("cmp rcx, rdx");                                        // is the budget too small to be worth guarding?
    emitter.instruction("jb __rt_stack_limit_init_disable");                    // yes, leave the guard disabled rather than publish a bogus floor

    // -- floor = entry stack pointer - (budget - reserve) --
    emitter.instruction(&format!("sub rcx, {}", STACK_GUARD_RESERVE_BYTES));    // subtract the reserve kept for helper frames and the fatal path
    emitter.instruction("mov rax, rbp");                                        // start from the remembered stack-top reference
    emitter.instruction("sub rax, rcx");                                        // rax = the lowest stack address compiled prologues may reach
    emitter.instruction("cmp rax, rbp");                                        // did the subtraction wrap below address zero?
    emitter.instruction("jae __rt_stack_limit_init_disable");                   // yes, an unusable floor, so disable the guard instead
    emitter.instruction("jmp __rt_stack_limit_init_store");                     // publish the computed floor

    // -- disabled: publish zero so every prologue compare passes --
    emitter.label("__rt_stack_limit_init_disable");
    emitter.instruction("xor eax, eax");                                        // zero disables the guard for the rest of the process

    emitter.label("__rt_stack_limit_init_store");
    emit_publish_main_floor(emitter, "rax");
    emitter.instruction("mov rsp, rbp");                                        // release the rlimit output buffer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the process entry prologue
}

const _: () = assert!(STACK_BUDGET_MIN_BYTES > STACK_GUARD_RESERVE_BYTES);
const _: () = assert!(STACK_BUDGET_FALLBACK_BYTES <= STACK_BUDGET_CAP_BYTES);
