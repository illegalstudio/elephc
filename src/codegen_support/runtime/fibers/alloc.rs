//! Purpose:
//! Emits runtime helpers that allocate and free per-fiber stacks.
//! Owns the mmap/protection setup that gives each Fiber an isolated stack and guard page.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::fibers`.
//!
//! Key details:
//! - Stack regions must be unmapped when Fiber objects are freed, and guard-page sizing must match supported targets.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::abi;
use crate::codegen_support::platform::{Arch, Platform};
use super::{FIBER_LIVE_STACK_SLOTS, FIBER_STACK_CACHE_SLOTS};

/// Size of the unmapped (PROT_NONE) guard region at the bottom of every fiber
/// stack. Set to 16 KB so the guard fully covers a single page on every
/// supported target — macOS aarch64 uses 16 KB pages, Linux aarch64 typically
/// uses 4 KB but accepts oversized protection ranges silently.
pub(super) const FIBER_GUARD_PAGE_SIZE: i32 = 16384;

/// Returns the platform-specific flag word for `MAP_PRIVATE | MAP_ANONYMOUS`.
/// - macOS: `0x1002` (MAP_PRIVATE | MAP_ANON)
/// - Linux: `0x22` (MAP_PRIVATE | MAP_ANONYMOUS)
fn map_anon_private_flags(platform: Platform) -> i32 {
    match platform {
        Platform::MacOS => 0x1002, // MAP_PRIVATE | MAP_ANON
        Platform::Linux => 0x22,   // MAP_PRIVATE | MAP_ANONYMOUS
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    }
}

/// __rt_fiber_alloc_stack: reserve a usable stack with a guard page.
/// Input:  x0 = requested usable stack size in bytes
/// Output: x0 = stack_base (low address — the mmap region start, includes the guard page)
///         x1 = stack_top  (high address, initial SP, 16-byte aligned)
///         x2 = total mmap'd length in bytes (usable size + guard page) — needed for munmap
pub fn emit_fiber_alloc_stack(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_alloc_stack_x86_64(emitter);
        return;
    }

    let map_flags = map_anon_private_flags(emitter.target.platform);

    emitter.blank();
    emitter.comment("--- runtime: fiber_alloc_stack (mmap + guard page) ---");
    emitter.label_global("__rt_fiber_alloc_stack");
    emit_stack_cache_pop_aarch64(emitter);

    // -- prologue: save x19/x20/x21 (callee-saved) since we use them as scratch --
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame plus three saved-callee slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp]");                                  // preserve caller's x19/x20 — used to remember alloc state across libc calls
    emitter.instruction("str x21, [sp, #16]");                                  // preserve caller's x21 — used to hold the mmap base across mprotect
    emitter.instruction("add x29, sp, #32");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = requested usable stack size (callee-saved across mmap/mprotect)

    // -- mmap(NULL, requested_size + GUARD, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1, 0) --
    emitter.instruction("mov x0, #0");                                          // addr = NULL — let the kernel pick the address
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
    emitter.instruction("mov x2, #0");                                          // prot = PROT_NONE — touching the guard faults via SIGSEGV
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
    emitter.instruction("b __rt_fiber_alloc_stack_register");                   // track the fresh stack before handing it back

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
    emit_live_stack_register_aarch64(emitter);
}

/// __rt_fiber_free_stack: return a fiber stack to the kernel via munmap.
/// Input:  x0 = stack_base (mapping base returned by __rt_fiber_alloc_stack)
///         x1 = total mapped length (the third return value of alloc, stored in stack_size)
pub fn emit_fiber_free_stack(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_free_stack_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fiber_free_stack (untrack, then cache or munmap) ---");
    emitter.label_global("__rt_fiber_free_stack");
    emitter.instruction("cbz x0, __rt_fiber_free_stack_done");                  // skip NULL bases (alloc failure or already freed)
    emitter.instruction("cbz x1, __rt_fiber_free_stack_done");                  // skip zero-length mappings as a defensive guard against double-free
    emit_live_stack_unregister_aarch64(emitter);
    emitter.instruction("b __rt_fiber_free_stack_untracked");                   // cache or unmap the stack now that it is no longer tracked
    emitter.label("__rt_fiber_free_stack_done");
    emitter.instruction("ret");                                                 // NULL/zero-length stacks have nothing to release

    emitter.blank();
    emitter.comment("--- runtime: fiber_free_stack_untracked (cache or munmap) ---");
    emitter.label_global("__rt_fiber_free_stack_untracked");
    emitter.instruction("cbz x0, __rt_fiber_free_stack_untracked_done");        // skip NULL bases
    emitter.instruction("cbz x1, __rt_fiber_free_stack_untracked_done");        // skip zero-length mappings
    emit_stack_cache_push_aarch64(emitter);
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save the caller's frame pointer and return address before invoking libc
    emitter.bl_c("munmap");                                                     // ignore the return value: the stack is gone either way
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller's frame pointer and return address
    emitter.label("__rt_fiber_free_stack_untracked_done");
    emitter.instruction("ret");                                                 // hand control back to the caller

    emit_release_request_stacks_aarch64(emitter);
}

/// Records the stack `__rt_fiber_alloc_stack` is about to return: `x0` = base, `x2` = mapped
/// length, `x1` = top (all preserved). Ends the allocator, so it returns to its caller.
fn emit_live_stack_register_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_fiber_alloc_stack_register");
    abi::emit_symbol_address(emitter, "x9", "_fiber_live_stacks");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = number of tracked stacks
    emitter.instruction(&format!("cmp x10, #{}", FIBER_LIVE_STACK_SLOTS));      // is the table full?
    emitter.instruction("b.hs __rt_fiber_alloc_stack_register_done");           // an untracked stack keeps the pre-table behavior
    emitter.instruction("add x11, x9, x10, lsl #4");                            // x11 = table base + 16 * count (entry sits at +8)
    emitter.instruction("stp x0, x2, [x11, #8]");                               // record the base and mapped length
    emitter.instruction("add x10, x10, #1");                                    // one more tracked stack
    emitter.instruction("str x10, [x9]");                                       // publish the new count
    emitter.label("__rt_fiber_alloc_stack_register_done");
    emitter.instruction("ret");                                                 // hand the (base, top, length) triple back
}

/// Drops the stack in `x0` from the live table, moving the last entry into its slot; a stack the
/// table does not hold is left alone. Searches from the most recent entry, because fibers are
/// mostly freed in the reverse order they were made. Preserves `x0`/`x1`; clobbers `x9`-`x15`.
fn emit_live_stack_unregister_aarch64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "x9", "_fiber_live_stacks");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = number of tracked stacks
    emitter.instruction("mov x11, x10");                                        // x11 = search index, walking down from the top
    emitter.label("__rt_fiber_free_stack_find");
    emitter.instruction("cbz x11, __rt_fiber_free_stack_find_done");            // not tracked: nothing to drop
    emitter.instruction("sub x11, x11, #1");                                    // step to the next older entry
    emitter.instruction("add x12, x9, x11, lsl #4");                            // x12 = that entry's slot (base at +8)
    emitter.instruction("ldr x13, [x12, #8]");                                  // x13 = its recorded base
    emitter.instruction("cmp x13, x0");                                         // is it the stack being freed?
    emitter.instruction("b.ne __rt_fiber_free_stack_find");                     // keep searching older entries
    emitter.instruction("sub x10, x10, #1");                                    // x10 = index of the last entry, and the new count
    emitter.instruction("add x13, x9, x10, lsl #4");                            // x13 = the last entry's slot
    emitter.instruction("ldp x14, x15, [x13, #8]");                             // load the last entry
    emitter.instruction("stp x14, x15, [x12, #8]");                             // move it into the freed slot
    emitter.instruction("str x10, [x9]");                                       // publish the shorter table
    emitter.label("__rt_fiber_free_stack_find_done");
}

/// `__rt_fiber_release_request_stacks`: returns every still-tracked stack to the cache (or the
/// kernel) at the `--web` request boundary, where the generators owning them are dropped with
/// the heap arena instead of being freed.
fn emit_release_request_stacks_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_release_request_stacks ---");
    emitter.label_global("__rt_fiber_release_request_stacks");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // keep the return address across the per-stack releases
    emitter.instruction("mov x29, sp");                                         // anchor a frame for the loop's calls
    emitter.label("__rt_fiber_release_request_stacks_loop");
    abi::emit_symbol_address(emitter, "x9", "_fiber_live_stacks");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = number of stacks still tracked
    emitter.instruction("cbz x10, __rt_fiber_release_request_stacks_done");     // every stack is back: done
    emitter.instruction("sub x10, x10, #1");                                    // x10 = index of the last entry
    emitter.instruction("str x10, [x9]");                                       // pop it before releasing, so the loop always advances
    emitter.instruction("add x11, x9, x10, lsl #4");                            // x11 = that entry's slot
    emitter.instruction("ldp x0, x1, [x11, #8]");                               // x0 = base, x1 = mapped length
    emitter.instruction("bl __rt_fiber_free_stack_untracked");                  // cache or unmap it
    emitter.instruction("b __rt_fiber_release_request_stacks_loop");            // continue with the next tracked stack
    emitter.label("__rt_fiber_release_request_stacks_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame
    emitter.instruction("ret");                                                 // back to the request reset
}

/// x86_64-specific implementation of `__rt_fiber_alloc_stack`.
/// Uses the System V AMD64 ABI convention for mmap/mprotect calls.
/// Mirrors the ARM64 path in `emit_fiber_alloc_stack` but emits x86_64
/// instructions and preserves callee-saved registers (r12–r15, rbp) across
/// libc calls.
fn emit_alloc_stack_x86_64(emitter: &mut Emitter) {
    let map_flags = map_anon_private_flags(emitter.target.platform);

    emitter.blank();
    emitter.comment("--- runtime: fiber_alloc_stack (mmap + guard page) ---");
    emitter.label_global("__rt_fiber_alloc_stack");
    emit_stack_cache_pop_x86_64(emitter);

    // -- prologue: save callee-saved registers used across libc calls --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer for the allocator helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base while libc calls run
    emitter.instruction("push r12");                                            // preserve the requested usable size across mmap and mprotect
    emitter.instruction("push r13");                                            // preserve the total mapped length across mprotect
    emitter.instruction("push r14");                                            // preserve the mapping base across mprotect
    emitter.instruction("push r15");                                            // keep the SysV stack aligned while saving three live registers
    emitter.instruction("mov r12, rdi");                                        // r12 = requested usable stack size

    // -- mmap(NULL, requested_size + GUARD, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1, 0) --
    emitter.instruction("xor edi, edi");                                        // addr = NULL — let the kernel pick the address
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
    emitter.instruction("xor edx, edx");                                        // prot = PROT_NONE — touching the guard faults via SIGSEGV
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
    emitter.instruction("jmp __rt_fiber_alloc_stack_register");                 // track the fresh stack before handing it back

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
    emit_live_stack_register_x86_64(emitter);
}

/// x86_64-specific implementation of `__rt_fiber_free_stack`.
/// Uses the System V AMD64 ABI convention for the munmap call.
/// Preserves rbp around the libc call to maintain stack alignment.
fn emit_free_stack_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_free_stack (untrack, then cache or munmap) ---");
    emitter.label_global("__rt_fiber_free_stack");
    emitter.instruction("test rdi, rdi");                                       // skip NULL bases (alloc failure or already freed)
    emitter.instruction("jz __rt_fiber_free_stack_done");                       // no mapping base means there is nothing to release
    emitter.instruction("test rsi, rsi");                                       // skip zero-length mappings as a defensive guard
    emitter.instruction("jz __rt_fiber_free_stack_done");                       // no mapped length means there is nothing to release
    emit_live_stack_unregister_x86_64(emitter);
    emitter.instruction("jmp __rt_fiber_free_stack_untracked");                 // cache or unmap the stack now that it is no longer tracked
    emitter.label("__rt_fiber_free_stack_done");
    emitter.instruction("ret");                                                 // NULL/zero-length stacks have nothing to release

    emitter.blank();
    emitter.comment("--- runtime: fiber_free_stack_untracked (cache or munmap) ---");
    emitter.label_global("__rt_fiber_free_stack_untracked");
    emitter.instruction("test rdi, rdi");                                       // skip NULL bases
    emitter.instruction("jz __rt_fiber_free_stack_untracked_done");             // no mapping base means there is nothing to unmap
    emitter.instruction("test rsi, rsi");                                       // skip zero-length mappings
    emitter.instruction("jz __rt_fiber_free_stack_untracked_done");             // no mapped length means there is nothing to unmap
    emit_stack_cache_push_x86_64(emitter);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before invoking libc
    emitter.instruction("mov rbp, rsp");                                        // keep the SysV stack aligned for the munmap call
    emitter.bl_c("munmap");                                                     // ignore the return value: the stack is gone either way
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after munmap
    emitter.label("__rt_fiber_free_stack_untracked_done");
    emitter.instruction("ret");                                                 // hand control back to the caller

    emit_release_request_stacks_x86_64(emitter);
}

/// x86_64 twin of [`emit_live_stack_register_aarch64`]: `rax` = base, `rcx` = mapped length,
/// `rdx` = top (all preserved).
fn emit_live_stack_register_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_fiber_alloc_stack_register");
    abi::emit_symbol_address(emitter, "r9", "_fiber_live_stacks");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // r10 = number of tracked stacks
    emitter.instruction(&format!("cmp r10, {}", FIBER_LIVE_STACK_SLOTS));       // is the table full?
    emitter.instruction("jae __rt_fiber_alloc_stack_register_done");            // an untracked stack keeps the pre-table behavior
    emitter.instruction("mov r11, r10");                                        // r11 = count
    emitter.instruction("shl r11, 4");                                          // r11 = 16 * count
    emitter.instruction("add r11, r9");                                         // r11 = table base + 16 * count (entry sits at +8)
    emitter.instruction("mov QWORD PTR [r11 + 8], rax");                        // record the base
    emitter.instruction("mov QWORD PTR [r11 + 16], rcx");                       // record the mapped length
    emitter.instruction("add r10, 1");                                          // one more tracked stack
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the new count
    emitter.label("__rt_fiber_alloc_stack_register_done");
    emitter.instruction("ret");                                                 // hand the (base, top, length) triple back
}

/// x86_64 twin of [`emit_live_stack_unregister_aarch64`]. Preserves `rdi`/`rsi`; clobbers
/// `rax`, `rcx`, `r8`-`r11`.
fn emit_live_stack_unregister_x86_64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "r9", "_fiber_live_stacks");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // r10 = number of tracked stacks
    emitter.instruction("mov r11, r10");                                        // r11 = search index, walking down from the top
    emitter.label("__rt_fiber_free_stack_find");
    emitter.instruction("test r11, r11");                                       // any older entry left?
    emitter.instruction("jz __rt_fiber_free_stack_find_done");                  // not tracked: nothing to drop
    emitter.instruction("sub r11, 1");                                          // step to the next older entry
    emitter.instruction("mov r8, r11");                                         // r8 = that entry's index
    emitter.instruction("shl r8, 4");                                           // r8 = 16 * index
    emitter.instruction("add r8, r9");                                          // r8 = that entry's slot (base at +8)
    emitter.instruction("cmp QWORD PTR [r8 + 8], rdi");                         // is it the stack being freed?
    emitter.instruction("jne __rt_fiber_free_stack_find");                      // keep searching older entries
    emitter.instruction("sub r10, 1");                                          // r10 = index of the last entry, and the new count
    emitter.instruction("mov rax, r10");                                        // rax = last entry's index
    emitter.instruction("shl rax, 4");                                          // rax = 16 * last index
    emitter.instruction("add rax, r9");                                         // rax = the last entry's slot
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // load the last entry's base
    emitter.instruction("mov QWORD PTR [r8 + 8], rcx");                         // move it into the freed slot
    emitter.instruction("mov rcx, QWORD PTR [rax + 16]");                       // load the last entry's mapped length
    emitter.instruction("mov QWORD PTR [r8 + 16], rcx");                        // move it into the freed slot
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the shorter table
    emitter.label("__rt_fiber_free_stack_find_done");
}

/// x86_64 twin of [`emit_release_request_stacks_aarch64`].
fn emit_release_request_stacks_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_release_request_stacks ---");
    emitter.label_global("__rt_fiber_release_request_stacks");
    emitter.instruction("push rbp");                                            // align the stack for the per-stack release calls
    emitter.instruction("mov rbp, rsp");                                        // anchor a frame for the loop's calls
    emitter.label("__rt_fiber_release_request_stacks_loop");
    abi::emit_symbol_address(emitter, "r9", "_fiber_live_stacks");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // r10 = number of stacks still tracked
    emitter.instruction("test r10, r10");                                       // any stack left?
    emitter.instruction("jz __rt_fiber_release_request_stacks_done");           // every stack is back: done
    emitter.instruction("sub r10, 1");                                          // r10 = index of the last entry
    emitter.instruction("mov QWORD PTR [r9], r10");                             // pop it before releasing, so the loop always advances
    emitter.instruction("shl r10, 4");                                          // r10 = 16 * index
    emitter.instruction("add r10, r9");                                         // r10 = that entry's slot
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // rdi = base
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // rsi = mapped length
    emitter.instruction("call __rt_fiber_free_stack_untracked");                // cache or unmap it
    emitter.instruction("jmp __rt_fiber_release_request_stacks_loop");          // continue with the next tracked stack
    emitter.label("__rt_fiber_release_request_stacks_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame
    emitter.instruction("ret");                                                 // back to the request reset
}

// `_fiber_stack_cache` layout: [count:8] then FIBER_STACK_CACHE_SLOTS entries of
// [base:8][mapped length:8], used as a LIFO stack. A cached stack keeps its PROT_NONE guard
// page, so reusing one needs no syscall at all. The cache belongs to the process, not to a
// request: a stack carries no PHP state once its fiber is freed.

/// Pops the most recently freed stack when it has the requested size, returning it exactly as
/// the `mmap` path would; falls through to `__rt_fiber_alloc_stack_cache_miss` otherwise.
/// Input `x0` = requested usable size, preserved on the miss path.
fn emit_stack_cache_pop_aarch64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "x9", "_fiber_stack_cache");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = number of cached stacks
    emitter.instruction("cbz x10, __rt_fiber_alloc_stack_cache_miss");          // nothing cached: map a fresh stack
    emitter.instruction(&format!("add x11, x0, #{}", FIBER_GUARD_PAGE_SIZE));   // x11 = mapped length a stack of this size has
    emitter.instruction("sub x12, x10, #1");                                    // x12 = index of the most recently freed stack
    emitter.instruction("add x13, x9, x12, lsl #4");                            // x13 = cache base + 16 * index (entry sits at +8)
    emitter.instruction("ldr x14, [x13, #16]");                                 // x14 = that stack's mapped length
    emitter.instruction("cmp x14, x11");                                        // was it mapped for the same usable size?
    emitter.instruction("b.ne __rt_fiber_alloc_stack_cache_miss");              // a different size cannot be reused as-is
    emitter.instruction("ldr x0, [x13, #8]");                                   // x0 = cached mapping base, guard page still installed
    emitter.instruction("str x12, [x9]");                                       // pop it off the cache
    emitter.instruction("add x1, x0, x14");                                     // x1 = end of the mapped region
    emitter.instruction("and x1, x1, #-16");                                    // stack_top rounded down to 16 bytes, as the mmap path does
    emitter.instruction("mov x2, x14");                                         // x2 = mapped length for the eventual free
    emitter.instruction("b __rt_fiber_alloc_stack_register");                   // track the reused stack before handing it back
    emitter.label("__rt_fiber_alloc_stack_cache_miss");
}

/// Parks a freed stack in the cache while it has room; falls through to
/// `__rt_fiber_free_stack_unmap` when it is full. Input `x0` = base, `x1` = mapped length.
fn emit_stack_cache_push_aarch64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "x9", "_fiber_stack_cache");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = number of cached stacks
    emitter.instruction(&format!("cmp x10, #{}", FIBER_STACK_CACHE_SLOTS));     // is the cache full?
    emitter.instruction("b.hs __rt_fiber_free_stack_unmap");                    // yes: give this one back to the kernel
    emitter.instruction("add x11, x9, x10, lsl #4");                            // x11 = cache base + 16 * count (entry sits at +8)
    emitter.instruction("stp x0, x1, [x11, #8]");                               // record the base and mapped length
    emitter.instruction("add x10, x10, #1");                                    // one more cached stack
    emitter.instruction("str x10, [x9]");                                       // publish the new count
    emitter.instruction("ret");                                                 // the stack stays mapped for the next fiber
    emitter.label("__rt_fiber_free_stack_unmap");
}

/// x86_64 twin of [`emit_stack_cache_pop_aarch64`]. Input `rdi` = requested usable size;
/// output `rax`/`rdx`/`rcx` = base/top/length as the `mmap` path returns them.
fn emit_stack_cache_pop_x86_64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "r9", "_fiber_stack_cache");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // r10 = number of cached stacks
    emitter.instruction("test r10, r10");                                       // is anything cached?
    emitter.instruction("jz __rt_fiber_alloc_stack_cache_miss");                // nothing cached: map a fresh stack
    emitter.instruction(&format!("lea r11, [rdi + {}]", FIBER_GUARD_PAGE_SIZE)); // r11 = mapped length a stack of this size has
    emitter.instruction("lea r8, [r10 - 1]");                                   // r8 = index of the most recently freed stack
    emitter.instruction("shl r8, 4");                                           // r8 = 16 * index
    emitter.instruction("add r8, r9");                                          // r8 = cache base + 16 * index (entry sits at +8)
    emitter.instruction("cmp QWORD PTR [r8 + 16], r11");                        // was it mapped for the same usable size?
    emitter.instruction("jne __rt_fiber_alloc_stack_cache_miss");               // a different size cannot be reused as-is
    emitter.instruction("mov rax, QWORD PTR [r8 + 8]");                         // rax = cached mapping base, guard page still installed
    emitter.instruction("sub r10, 1");                                          // pop it off the cache
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the new count
    emitter.instruction("mov rcx, r11");                                        // rcx = mapped length for the eventual free
    emitter.instruction("lea rdx, [rax + r11]");                                // rdx = end of the mapped region
    emitter.instruction("and rdx, -16");                                        // stack_top rounded down to 16 bytes, as the mmap path does
    emitter.instruction("jmp __rt_fiber_alloc_stack_register");                 // track the reused stack before handing it back
    emitter.label("__rt_fiber_alloc_stack_cache_miss");
}

/// x86_64 twin of [`emit_stack_cache_push_aarch64`]. Input `rdi` = base, `rsi` = mapped length.
fn emit_stack_cache_push_x86_64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "r9", "_fiber_stack_cache");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // r10 = number of cached stacks
    emitter.instruction(&format!("cmp r10, {}", FIBER_STACK_CACHE_SLOTS));      // is the cache full?
    emitter.instruction("jae __rt_fiber_free_stack_unmap");                     // yes: give this one back to the kernel
    emitter.instruction("mov r11, r10");                                        // r11 = count
    emitter.instruction("shl r11, 4");                                          // r11 = 16 * count
    emitter.instruction("add r11, r9");                                         // r11 = cache base + 16 * count (entry sits at +8)
    emitter.instruction("mov QWORD PTR [r11 + 8], rdi");                        // record the mapping base
    emitter.instruction("mov QWORD PTR [r11 + 16], rsi");                       // record the mapped length
    emitter.instruction("add r10, 1");                                          // one more cached stack
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the new count
    emitter.instruction("ret");                                                 // the stack stays mapped for the next fiber
    emitter.label("__rt_fiber_free_stack_unmap");
}
