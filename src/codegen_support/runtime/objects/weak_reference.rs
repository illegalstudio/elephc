//! Purpose:
//! Implements unique WeakReference wrappers and invalidates them before object storage reuse.
//!
//! Called from:
//! - Typed Core method calls and the allocator's object reclamation path.
//!
//! Key details:
//! - One non-owning wrapper pointer is indexed by the referent's heap granule.
//! - The wrapper stores a raw integer pointer, which object GC never traverses.
//! - Reclamation clears both directions before allocator or PHP handle reuse.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels};

/// Emits weak wrapper creation, nullable reads, and non-owning identity retirement.
pub(crate) fn emit_weak_reference(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Creates and reads weak wrappers under the AArch64 runtime convention.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_weak_reference_create");
    emitter.instruction("stp x29, x30, [sp, #-48]!");                           // preserve the caller across wrapper allocation
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the object or Closure runtime tag
    emitter.instruction("mov x0, x1");                                          // weak identity is the borrowed payload allocation
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the borrowed referent during allocation
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("sub x10, x0, x9");                                     // derive the referent's direct-mapped heap offset
    emitter.instruction("lsr x10, x10, #4");                                    // select the referent's unique allocation granule
    abi::emit_symbol_address(emitter, "x9", "_obj_weak_reference_index");
    emitter.instruction("add x9, x9, x10, lsl #3");                             // address the non-owning cached wrapper slot
    emitter.instruction("str x9, [sp, #24]");                                   // preserve the cache slot across heap allocation
    emitter.instruction("ldr x0, [x9]");                                        // reuse the unique live wrapper when one already exists
    emitter.instruction("cbz x0, __rt_weak_reference_create_fresh");            // allocate only when no wrapper is live
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("b __rt_weak_reference_create_done");                   // transfer the acquired wrapper owner to the caller
    emitter.label("__rt_weak_reference_create_fresh");
    emitter.instruction("mov x0, #24");                                         // allocate a class id and one integer property slot
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov x9, #4");                                          // classify the wrapper as an ordinary managed object
    emitter.instruction("str x9, [x0, #-8]");                                   // publish the object heap kind before handle binding
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_weak_reference_class_id", 0);
    emitter.instruction("str x9, [x0]");                                        // stamp the public WeakReference class identity
    emitter.instruction("ldr x9, [sp, #16]");                                   // recover the borrowed referent without retaining it
    emitter.instruction("str x9, [x0, #8]");                                    // store an integer pointer excluded from GC ownership edges
    emitter.instruction("ldr x9, [sp, #32]");                                   // recover the referent's boxed object or Closure tag
    emitter.instruction("str x9, [x0, #16]");                                   // retain type metadata without creating an ownership edge
    emitter.instruction("ldr x9, [sp, #24]");                                   // recover this referent's non-owning cache slot
    emitter.instruction("str x0, [x9]");                                        // publish the unique live wrapper
    emitter.label("__rt_weak_reference_create_done");
    emitter.instruction("ldp x29, x30, [sp], #48");                             // restore the caller after acquiring the result owner
    emitter.instruction("ret");                                                 // return the owned wrapper object

    emitter.label_global("__rt_weak_reference_get");
    emitter.instruction("ldr x1, [x0, #8]");                                    // borrow the referent until boxing acquires its owner
    emitter.instruction("mov x2, #0");                                          // object payloads do not use a high word
    emitter.instruction("ldr x0, [x0, #16]");                                   // restore the object, Closure, or cleared-null tag
    emitter.instruction("b __rt_mixed_from_value");                             // return an independently owned nullable object cell

    emitter.label_global("__rt_weak_reference_retire");
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("sub x10, x0, x9");                                     // derive the reclaimed object's allocation granule
    emitter.instruction("lsr x10, x10, #4");                                    // select its weak cache slot before storage reuse
    abi::emit_symbol_address(emitter, "x11", "_obj_weak_reference_index");
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // fetch the non-owning wrapper for this referent
    emitter.instruction("cbz x12, __rt_weak_reference_retire_wrapper");         // an unobserved object needs no wrapper invalidation
    emitter.instruction("str xzr, [x12, #8]");                                  // invalidate the referent before freeing its bytes
    emitter.instruction("mov x10, #8");                                         // a cleared observer returns boxed null
    emitter.instruction("str x10, [x12, #16]");                                 // retire Closure tags together with their payload
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("sub x10, x0, x9");                                     // restore the granule after preparing the null tag
    emitter.instruction("lsr x10, x10, #4");                                    // recover the cache index before unlinking
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // clear the cache before allocator granule reuse
    emitter.label("__rt_weak_reference_retire_wrapper");
    emitter.instruction("ldr x12, [x0, #-8]");                                  // Closure descriptors are not ordinary class instances
    emitter.instruction("and x12, x12, #0xff");                                 // isolate the allocation's heap kind
    emitter.instruction("cmp x12, #4");                                         // only ordinary objects can be WeakReference wrappers
    emitter.instruction("b.ne __rt_weak_reference_retire_done");                // skip reverse wrapper inspection for native Closures
    abi::emit_load_symbol_to_reg(emitter, "x10", "_weak_reference_class_id", 0);
    emitter.instruction("ldr x12, [x0]");                                       // determine whether the reclaimed object is itself a wrapper
    emitter.instruction("cmp x12, x10");                                        // only WeakReference carries the integer referent slot
    emitter.instruction("b.ne __rt_weak_reference_retire_done");                // other object layouts have no reverse cache edge
    emitter.instruction("ldr x12, [x0, #8]");                                   // inspect the wrapper's still-live referent
    emitter.instruction("cbz x12, __rt_weak_reference_retire_done");            // cleared wrappers have already lost their cache entry
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("sub x10, x12, x9");                                    // derive the surviving referent's allocation granule
    emitter.instruction("lsr x10, x10, #4");                                    // select the reverse non-owning cache slot
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // inspect the cached wrapper before unlinking it
    emitter.instruction("cmp x12, x0");                                         // a refused clone must not unlink the original wrapper
    emitter.instruction("b.ne __rt_weak_reference_retire_done");                // preserve another wrapper's cache identity
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // unlink the last live owner of this cached wrapper
    emitter.label("__rt_weak_reference_retire_done");
    emitter.instruction("ret");                                                 // preserve x0 for the allocator's remaining reclamation work
}

/// Implements identical weak ownership and cache invalidation on Linux x86_64.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_weak_reference_create");
    emitter.instruction("push rbp");                                            // preserve the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // anchor referent and cache scratch across allocation
    emitter.instruction("sub rsp, 32");                                         // reserve aligned scratch for the borrowed referent
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the object or Closure runtime tag
    emitter.instruction("mov rax, rdi");                                        // index the borrowed payload rather than its Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the referent without acquiring an owner
    abi::emit_symbol_address(emitter, "r11", "_heap_buf");
    emitter.instruction("mov r10, rax");                                        // copy the referent before computing its granule
    emitter.instruction("sub r10, r11");                                        // derive its heap-relative byte offset
    emitter.instruction("shr r10, 4");                                          // select the unique allocation granule
    abi::emit_symbol_address(emitter, "r11", "_obj_weak_reference_index");
    emitter.instruction("lea r10, [r11 + r10 * 8]");                            // locate the non-owning wrapper cache entry
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the cache slot across allocation
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // reuse the unique live wrapper
    emitter.instruction("test rax, rax");                                       // determine whether allocation is necessary
    emitter.instruction("jz __rt_weak_reference_create_fresh");                 // allocate only when no wrapper survives
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("jmp __rt_weak_reference_create_done");                 // return the acquired existing wrapper owner
    emitter.label("__rt_weak_reference_create_fresh");
    emitter.instruction("mov rax, 24");                                         // allocate a class id and one integer property slot
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!("mov r10, 0x{:x}", sentinels::x86_64_heap_kind_word(4))); // classify the allocation as a managed object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish the object heap kind before binding its handle
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_weak_reference_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // stamp the public WeakReference class identity
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the borrowed referent
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store an integer pointer excluded from ownership tracing
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // recover the object or Closure runtime tag
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // retain non-owning type metadata for get()
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // recover the referent's cache slot
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the unique live wrapper without retaining it
    emitter.label("__rt_weak_reference_create_done");
    emitter.instruction("leave");                                               // restore the caller frame after acquiring the result owner
    emitter.instruction("ret");                                                 // return the owned wrapper

    emitter.label_global("__rt_weak_reference_get");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // borrow the current referent until boxing retains it
    emitter.instruction("xor esi, esi");                                        // object payloads have no high word
    emitter.instruction("mov rax, QWORD PTR [rax + 16]");                       // restore the object, Closure, or cleared-null tag
    emitter.instruction("jmp __rt_mixed_from_value");                           // return a newly owned nullable object cell

    emitter.label_global("__rt_weak_reference_retire");
    abi::emit_symbol_address(emitter, "r9", "_heap_buf");
    emitter.instruction("mov r10, rax");                                        // copy the object being reclaimed
    emitter.instruction("sub r10, r9");                                         // derive its heap-relative byte offset
    emitter.instruction("shr r10, 4");                                          // select its weak cache granule
    abi::emit_symbol_address(emitter, "r8", "_obj_weak_reference_index");
    emitter.instruction("mov r11, QWORD PTR [r8 + r10 * 8]");                   // inspect this referent's cached wrapper
    emitter.instruction("test r11, r11");                                       // unobserved objects have no weak wrapper
    emitter.instruction("jz __rt_weak_reference_retire_wrapper");               // skip absent wrapper invalidation
    emitter.instruction("mov QWORD PTR [r11 + 8], 0");                          // invalidate the referent before storage reuse
    emitter.instruction("mov QWORD PTR [r11 + 16], 8");                         // clear both object and Closure observers to boxed null
    emitter.instruction("mov QWORD PTR [r8 + r10 * 8], 0");                     // clear the non-owning cache entry
    emitter.label("__rt_weak_reference_retire_wrapper");
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // Closure descriptors have no ordinary class layout
    emitter.instruction("and r10d, 0xff");                                      // isolate the heap allocation kind
    emitter.instruction("cmp r10d, 4");                                         // reverse wrapper edges belong only to ordinary objects
    emitter.instruction("jne __rt_weak_reference_retire_done");                 // skip class-id inspection for native Closure storage
    abi::emit_load_symbol_to_reg(emitter, "r10", "_weak_reference_class_id", 0);
    emitter.instruction("cmp QWORD PTR [rax], r10");                            // only WeakReference owns a raw referent slot
    emitter.instruction("jne __rt_weak_reference_retire_done");                 // other objects need no reverse-cache cleanup
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // inspect this wrapper's surviving referent
    emitter.instruction("test r11, r11");                                       // a cleared wrapper has no cache edge
    emitter.instruction("jz __rt_weak_reference_retire_done");                  // leave already invalidated wrappers alone
    emitter.instruction("sub r11, r9");                                         // derive the surviving referent's heap offset
    emitter.instruction("shr r11, 4");                                          // select the referent's direct cache slot
    emitter.instruction("cmp QWORD PTR [r8 + r11 * 8], rax");                   // a refused clone cannot unlink the original wrapper
    emitter.instruction("jne __rt_weak_reference_retire_done");                 // preserve any independently cached wrapper
    emitter.instruction("mov QWORD PTR [r8 + r11 * 8], 0");                     // unlink this wrapper before its own storage is freed
    emitter.label("__rt_weak_reference_retire_done");
    emitter.instruction("ret");                                                 // preserve rax for the allocator's remaining work
}
