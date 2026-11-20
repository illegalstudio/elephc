//! Purpose:
//! Emits guarded stack allocation and mapping retirement.
//!
//! Called from:
//! - Compiler fiber runtime orchestration.
//!
//! Key details:
//! - The host supplies anonymous private mapping flags for its target platform.

use crate::{Arch, Emitter};

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
