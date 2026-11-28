//! Purpose:
//! Emits generation-safe Buffer release by invalidating descriptors before heap release.
//! Stale aliases fail descriptor resolution instead of observing recycled payload storage.
//!
//! Called from:
//! - `elephc::codegen_support::runtime::emitters::emit_runtime()` via `buffers`.
//!
//! Key details:
//! - The public handle supplies the free-list index before resolution changes the result register.
//! - A descriptor at generation `u32::MAX` is retired and never linked into the free list.

use crate::abi;
use crate::Emitter;
use crate::Arch;

/// Emits `__rt_buffer_free` for the active target.
/// Resolves a non-zero scalar handle, invalidates and recycles its descriptor when
/// possible, then tail-calls `__rt_heap_free` with the detached payload pointer.
pub fn emit_buffer_free(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_buffer_free_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_free ---");
    emitter.label_global("__rt_buffer_free");

    // -- preserve return state, handle index, and payload --
    emitter.instruction("sub sp, sp, #32");                                     // reserve an aligned scratch frame across descriptor resolution
    emitter.instruction("str x0, [sp]");                                        // retain scalar handle so its low u32 index needs no descriptor division
    emitter.instruction("str x30, [sp, #24]");                                  // preserve caller return address across the resolver call
    abi::emit_call_label(emitter, "__rt_buffer_resolve");
    emitter.instruction("ldr x9, [x0]");                                        // load payload pointer before clearing descriptor field zero
    emitter.instruction("str x9, [sp, #16]");                                   // retain detached payload across static registry updates

    // -- invalidate descriptor before recycling or releasing payload --
    emitter.instruction("str xzr, [x0, #32]");                                  // publish inactive state before mutating any other descriptor metadata
    emitter.instruction("str xzr, [x0]");                                       // clear payload pointer only after future resolution is fail-closed
    emitter.instruction("str xzr, [x0, #8]");                                   // clear logical length after descriptor activity is already disabled
    emitter.instruction("str xzr, [x0, #16]");                                  // clear element stride after descriptor activity is already disabled
    emitter.instruction("ldr w10, [x0, #24]");                                  // inspect lifetime generation without widening past u32
    emitter.instruction("cmn w10, #1");                                         // detect terminal u32::MAX generation without allowing wraparound
    emitter.instruction("b.eq __rt_buffer_free_retire");                        // permanently detach saturated descriptors from future allocation
    abi::emit_symbol_address(emitter, "x11", "_buffer_registry_free");
    emitter.instruction("ldr x12, [x11]");                                      // load the previous free-list head index
    emitter.instruction("str x12, [x0, #40]");                                  // link the inactive descriptor to the current free-list head
    emitter.instruction("ldr w12, [sp]");                                       // recover this descriptor index from the original scalar handle
    emitter.instruction("str x12, [x11]");                                      // publish recycled descriptor index before heap reuse can occur
    emitter.instruction("b __rt_buffer_free_release");                          // converge on payload release after successful free-list insertion
    emitter.label("__rt_buffer_free_retire");
    emitter.instruction("str xzr, [x0, #40]");                                  // ensure permanently retired descriptors have no stale list successor

    // -- restore caller return state and release only the detached payload --
    emitter.label("__rt_buffer_free_release");
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass detached payload pointer to the shared heap free helper
    emitter.instruction("ldr x30, [sp, #24]");                                  // restore caller return address before tail-calling heap_free back to the original caller
    emitter.instruction("add sp, sp, #32");                                     // release the aligned scratch frame before heap helper entry
    emitter.instruction("b __rt_heap_free");                                    // free payload only after descriptor invalidation and list publication
}

/// Emits the Linux x86_64 variant of `__rt_buffer_free`.
/// The scalar handle arrives in `rax`; its low u32 is saved before the resolver
/// returns the descriptor in the same register.
fn emit_buffer_free_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_free ---");
    emitter.label_global("__rt_buffer_free");

    // -- preserve handle index and payload across resolution --
    emitter.instruction("push rbp");                                            // preserve caller frame state while creating an aligned helper frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable spill slots for handle, descriptor, and payload
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill storage before the resolver call
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // retain scalar handle so index recovery avoids descriptor division
    abi::emit_call_label(emitter, "__rt_buffer_resolve");
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load payload pointer before clearing descriptor field zero
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // retain detached payload across static registry updates

    // -- invalidate descriptor before recycling or releasing payload --
    emitter.instruction("mov QWORD PTR [rax + 32], 0");                         // publish inactive state before mutating any other descriptor metadata
    emitter.instruction("mov QWORD PTR [rax], 0");                              // clear payload pointer only after future resolution is fail-closed
    emitter.instruction("mov QWORD PTR [rax + 8], 0");                          // clear logical length after descriptor activity is already disabled
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear element stride after descriptor activity is already disabled
    emitter.instruction("cmp DWORD PTR [rax + 24], -1");                        // detect terminal u32::MAX generation without allowing wraparound
    emitter.instruction("je __rt_buffer_free_retire_x");                        // permanently detach saturated descriptors from future allocation
    abi::emit_symbol_address(emitter, "r11", "_buffer_registry_free");
    emitter.instruction("mov rcx, QWORD PTR [r11]");                            // load the previous free-list head index
    emitter.instruction("mov QWORD PTR [rax + 40], rcx");                       // link the inactive descriptor to the current free-list head
    emitter.instruction("mov r10d, DWORD PTR [rbp - 8]");                       // recover this descriptor index from the original scalar handle
    emitter.instruction("mov QWORD PTR [r11], r10");                            // publish recycled descriptor index before heap reuse can occur
    emitter.instruction("jmp __rt_buffer_free_release_x");                      // converge on payload release after successful free-list insertion
    emitter.label("__rt_buffer_free_retire_x");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // ensure permanently retired descriptors have no stale list successor

    // -- restore caller frame state and release only the detached payload --
    emitter.label("__rt_buffer_free_release_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // pass detached payload pointer to the shared heap free helper
    emitter.instruction("add rsp, 32");                                         // release helper spill storage before tail transfer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before heap helper returns to generated code
    emitter.instruction("jmp __rt_heap_free");                                  // free payload only after descriptor invalidation and list publication
}
