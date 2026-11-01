//! Purpose:
//! Emits ownership helpers for request-global reference cells shared with PHP arrays.
//!
//! Called from:
//! - global load/store lowering, web reset, and tag-12 Mixed marker destruction.
//!
//! Key details:
//! - A cell owns a hash or boxed Mixed payload and uses the allocator header refcount for sharing.
//! - The final release decrefs the payload before freeing the cell allocation.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits global reference-cell incref/decref helpers for the current target.
pub fn emit_global_ref_cell(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_global_ref_cell_aarch64(emitter),
        Arch::X86_64 => emit_global_ref_cell_x86_64(emitter),
    }
}

/// Emits the AArch64 request-global reference-cell ownership helpers.
fn emit_global_ref_cell_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: request-global reference cell ownership ---");
    emitter.label_global("__rt_global_ref_cell_incref");
    emitter.instruction("cbz x0, __rt_global_ref_cell_incref_done");            // a missing cell has no ownership share to retain
    emitter.instruction("ldr w9, [x0, #-12]");                                 // load the allocator-header refcount
    emitter.instruction("add w9, w9, #1");                                     // retain one array-marker share
    emitter.instruction("str w9, [x0, #-12]");                                 // publish the incremented cell refcount
    emitter.label("__rt_global_ref_cell_incref_done");
    emitter.instruction("ret");                                                // return the unchanged cell pointer in x0

    emitter.blank();
    emitter.label_global("__rt_global_ref_cell_decref");
    emitter.instruction("cbz x0, __rt_global_ref_cell_decref_done");            // null globals need no release
    emitter.instruction("ldr w9, [x0, #-12]");                                 // load the shared cell refcount
    emitter.instruction("subs w9, w9, #1");                                    // release one global or marker owner
    emitter.instruction("str w9, [x0, #-12]");                                 // publish the remaining owner count
    emitter.instruction("b.ne __rt_global_ref_cell_decref_done");              // another owner keeps both cell and payload alive
    emitter.instruction("str x30, [sp, #-16]!");                               // preserve the return address across nested releases
    emitter.instruction("str x0, [sp, #8]");                                   // preserve the final cell pointer for heap_free
    emitter.instruction("ldr x9, [x0, #8]");                                  // read the stored payload representation tag
    emitter.instruction("ldr x0, [x0]");                                       // load the payload owned by the cell
    emitter.instruction("cmp x9, #7");                                        // representation tag 7 denotes a boxed Mixed payload
    emitter.instruction("b.eq __rt_global_ref_cell_release_mixed");            // dispatch ordinary PHP globals to boxed-value cleanup
    emitter.instruction("bl __rt_decref_hash");                                // release the final payload owner
    emitter.instruction("b __rt_global_ref_cell_release_storage");              // join after hash payload destruction
    emitter.label("__rt_global_ref_cell_release_mixed");
    emitter.instruction("bl __rt_decref_mixed");                               // release the ordinary global's boxed value
    emitter.label("__rt_global_ref_cell_release_storage");
    emitter.instruction("ldr x0, [sp, #8]");                                   // restore the cell pointer
    emitter.instruction("bl __rt_heap_free");                                  // free the cell allocation itself
    emitter.instruction("ldr x30, [sp], #16");                                 // restore the caller return address and stack
    emitter.label("__rt_global_ref_cell_decref_done");
    emitter.instruction("ret");                                                // finish the ownership release

    // -- hash view of a superglobal cell: x0 = cell or null -> x0 = borrowed hash or null --
    emitter.blank();
    emitter.label_global("__rt_shared_cell_hash_load");
    emitter.instruction("cbz x0, __rt_shared_cell_hash_load_done");            // no cell yet: the superglobal is still unset
    emitter.instruction("ldr x9, [x0, #8]");                                   // read the cell's payload representation tag
    emitter.instruction("ldr x0, [x0]");                                       // load the payload the cell owns
    emitter.instruction("cmp x9, #7");                                         // a boxed Mixed payload needs its hash unwrapped
    emitter.instruction("b.ne __rt_shared_cell_hash_load_done");               // a bare hash payload is already the answer
    emitter.label("__rt_shared_cell_hash_load_peel");
    emitter.instruction("cbz x0, __rt_shared_cell_hash_load_done");            // an empty box reads as an unset superglobal
    emitter.instruction("ldr x9, [x0]");                                       // inspect the boxed value's runtime tag
    emitter.instruction("cmp x9, #7");                                         // nested wrappers (references) hold the value one level down
    emitter.instruction("b.ne __rt_shared_cell_hash_load_value");              // reached the concrete boxed value
    emitter.instruction("ldr x0, [x0, #8]");                                   // follow the nested wrapper
    emitter.instruction("b __rt_shared_cell_hash_load_peel");                  // peel the next wrapper level
    emitter.label("__rt_shared_cell_hash_load_value");
    emitter.instruction("cmp x9, #5");                                         // only an associative array is a superglobal hash
    emitter.instruction("b.ne __rt_shared_cell_hash_load_none");               // anything else stored through an alias reads as unset
    emitter.instruction("ldr x0, [x0, #8]");                                   // borrow the boxed hash
    emitter.instruction("ret");                                                // the box keeps owning it
    emitter.label("__rt_shared_cell_hash_load_none");
    emitter.instruction("mov x0, #0");                                         // a non-array value has no hash view
    emitter.label("__rt_shared_cell_hash_load_done");
    emitter.instruction("ret");                                                // return the borrowed hash or null

    // -- store a hash into a superglobal cell: x0 = cell, x1 = owned hash -> x0 = previous hash owner or 0 --
    emitter.blank();
    emitter.label_global("__rt_shared_cell_hash_store");
    emitter.instruction("ldr x9, [x0, #8]");                                   // read the cell's payload representation tag
    emitter.instruction("cmp x9, #7");                                         // is the payload already a box the aliases share?
    emitter.instruction("b.ne __rt_shared_cell_hash_store_rebox");             // a bare or empty cell gets a fresh box
    emitter.instruction("ldr x10, [x0]");                                      // load the shared box
    emitter.label("__rt_shared_cell_hash_store_peel");
    emitter.instruction("cbz x10, __rt_shared_cell_hash_store_rebox");         // an empty box is replaced
    emitter.instruction("ldr x11, [x10]");                                     // inspect the boxed value's runtime tag
    emitter.instruction("cmp x11, #7");                                        // nested wrappers (references) hold the value one level down
    emitter.instruction("b.ne __rt_shared_cell_hash_store_value");             // reached the concrete boxed value
    emitter.instruction("ldr x10, [x10, #8]");                                 // follow the nested wrapper
    emitter.instruction("b __rt_shared_cell_hash_store_peel");                 // peel the next wrapper level
    emitter.label("__rt_shared_cell_hash_store_value");
    emitter.instruction("cmp x11, #5");                                        // swap in place only over an existing hash
    emitter.instruction("b.ne __rt_shared_cell_hash_store_rebox");             // any other value is replaced by a hash box
    emitter.instruction("ldr x0, [x10, #8]");                                  // hand the previous hash owner back to the caller
    emitter.instruction("str x1, [x10, #8]");                                  // every alias of the box now sees the new hash
    emitter.instruction("ret");                                                // ownership of the new hash moved into the box
    emitter.label("__rt_shared_cell_hash_store_rebox");
    emitter.instruction("sub sp, sp, #48");                                    // reserve cell, hash, replaced payload and its kind
    emitter.instruction("stp x29, x30, [sp, #32]");                            // preserve linkage across boxing and release
    emitter.instruction("add x29, sp, #32");                                   // establish the helper frame
    emitter.instruction("stp x0, x1, [sp]");                                   // keep the cell and the transferred hash
    emitter.instruction("ldr x10, [x0]");                                      // the payload this store replaces
    emitter.instruction("stp x10, x9, [sp, #16]");                             // keep it with its representation tag
    emitter.instruction("mov x0, #5");                                         // runtime tag 5 boxes an associative array
    emitter.instruction("mov x2, #0");                                         // hashes have no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                           // box the hash (the box retains its own owner)
    emitter.instruction("ldr x9, [sp]");                                       // recover the cell
    emitter.instruction("str x0, [x9]");                                       // publish the box before releasing anything
    emitter.instruction("mov x10, #7");                                        // the cell now holds a boxed Mixed payload
    emitter.instruction("str x10, [x9, #8]");                                  // record the representation every reader dispatches on
    emitter.instruction("ldr x0, [sp, #8]");                                   // the caller's hash owner moved into the box
    emitter.instruction("bl __rt_decref_hash");                                // drop it, leaving the box as the only owner
    emitter.instruction("ldp x0, x9, [sp, #16]");                              // recover the replaced payload and its tag
    emitter.instruction("cmp x9, #7");                                         // was it a box rather than a bare hash?
    emitter.instruction("b.ne __rt_shared_cell_hash_store_return");            // a bare hash goes back to the caller as the previous owner
    emitter.instruction("bl __rt_decref_mixed");                               // release the replaced box (aliases keep their own shares)
    emitter.instruction("mov x0, #0");                                         // no previous hash owner remains for the caller
    emitter.label("__rt_shared_cell_hash_store_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore linkage
    emitter.instruction("add sp, sp, #48");                                    // release the helper frame
    emitter.instruction("ret");                                                // return the previous hash owner or 0
}

/// Emits the Linux x86_64 request-global reference-cell ownership helpers.
fn emit_global_ref_cell_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: request-global reference cell ownership ---");
    emitter.label_global("__rt_global_ref_cell_incref");
    emitter.instruction("test rax, rax");                                      // a missing cell has no ownership share to retain
    emitter.instruction("jz __rt_global_ref_cell_incref_done");
    emitter.instruction("add DWORD PTR [rax - 12], 1");                        // retain one array-marker share in the allocator header
    emitter.label("__rt_global_ref_cell_incref_done");
    emitter.instruction("ret");                                                // return the unchanged cell pointer in rax

    emitter.blank();
    emitter.label_global("__rt_global_ref_cell_decref");
    emitter.instruction("test rax, rax");                                      // null globals need no release
    emitter.instruction("jz __rt_global_ref_cell_decref_done");
    emitter.instruction("sub DWORD PTR [rax - 12], 1");                        // release one global or marker owner
    emitter.instruction("jnz __rt_global_ref_cell_decref_done");               // another owner keeps both cell and payload alive
    emitter.instruction("push rbp");                                           // establish an aligned frame for nested helper calls
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                       // preserve the final cell pointer for heap_free
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                       // read the stored payload representation tag
    emitter.instruction("mov rax, QWORD PTR [rax]");                           // load the payload owned by the cell
    emitter.instruction("cmp r10, 7");                                         // representation tag 7 denotes a boxed Mixed payload
    emitter.instruction("je __rt_global_ref_cell_release_mixed");               // dispatch ordinary PHP globals to boxed-value cleanup
    emitter.instruction("call __rt_decref_hash");                              // release the final payload owner
    emitter.instruction("jmp __rt_global_ref_cell_release_storage");            // join after hash payload destruction
    emitter.label("__rt_global_ref_cell_release_mixed");
    emitter.instruction("call __rt_decref_mixed");                              // release the ordinary global's boxed value
    emitter.label("__rt_global_ref_cell_release_storage");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                       // restore the cell pointer
    emitter.instruction("call __rt_heap_free");                                // free the cell allocation itself
    emitter.instruction("leave");                                              // restore the caller frame and stack
    emitter.label("__rt_global_ref_cell_decref_done");
    emitter.instruction("ret");                                                // finish the ownership release

    // -- hash view of a superglobal cell: rax = cell or null -> rax = borrowed hash or null --
    emitter.blank();
    emitter.label_global("__rt_shared_cell_hash_load");
    emitter.instruction("test rax, rax");                                      // no cell yet: the superglobal is still unset
    emitter.instruction("jz __rt_shared_cell_hash_load_done");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                       // read the cell's payload representation tag
    emitter.instruction("mov rax, QWORD PTR [rax]");                           // load the payload the cell owns
    emitter.instruction("cmp r10, 7");                                         // a boxed Mixed payload needs its hash unwrapped
    emitter.instruction("jne __rt_shared_cell_hash_load_done");                // a bare hash payload is already the answer
    emitter.label("__rt_shared_cell_hash_load_peel");
    emitter.instruction("test rax, rax");                                      // an empty box reads as an unset superglobal
    emitter.instruction("jz __rt_shared_cell_hash_load_done");
    emitter.instruction("mov r10, QWORD PTR [rax]");                           // inspect the boxed value's runtime tag
    emitter.instruction("cmp r10, 7");                                         // nested wrappers (references) hold the value one level down
    emitter.instruction("jne __rt_shared_cell_hash_load_value");               // reached the concrete boxed value
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                       // follow the nested wrapper
    emitter.instruction("jmp __rt_shared_cell_hash_load_peel");                // peel the next wrapper level
    emitter.label("__rt_shared_cell_hash_load_value");
    emitter.instruction("cmp r10, 5");                                         // only an associative array is a superglobal hash
    emitter.instruction("jne __rt_shared_cell_hash_load_none");                // anything else stored through an alias reads as unset
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                       // borrow the boxed hash
    emitter.instruction("ret");                                                // the box keeps owning it
    emitter.label("__rt_shared_cell_hash_load_none");
    emitter.instruction("xor eax, eax");                                       // a non-array value has no hash view
    emitter.label("__rt_shared_cell_hash_load_done");
    emitter.instruction("ret");                                                // return the borrowed hash or null

    // -- store a hash into a superglobal cell: rax = cell, rdi = owned hash -> rax = previous hash owner or 0 --
    emitter.blank();
    emitter.label_global("__rt_shared_cell_hash_store");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                       // read the cell's payload representation tag
    emitter.instruction("cmp r10, 7");                                         // is the payload already a box the aliases share?
    emitter.instruction("jne __rt_shared_cell_hash_store_rebox");              // a bare or empty cell gets a fresh box
    emitter.instruction("mov r11, QWORD PTR [rax]");                           // load the shared box
    emitter.label("__rt_shared_cell_hash_store_peel");
    emitter.instruction("test r11, r11");                                      // an empty box is replaced
    emitter.instruction("jz __rt_shared_cell_hash_store_rebox");
    emitter.instruction("mov rcx, QWORD PTR [r11]");                           // inspect the boxed value's runtime tag
    emitter.instruction("cmp rcx, 7");                                         // nested wrappers (references) hold the value one level down
    emitter.instruction("jne __rt_shared_cell_hash_store_value");              // reached the concrete boxed value
    emitter.instruction("mov r11, QWORD PTR [r11 + 8]");                       // follow the nested wrapper
    emitter.instruction("jmp __rt_shared_cell_hash_store_peel");               // peel the next wrapper level
    emitter.label("__rt_shared_cell_hash_store_value");
    emitter.instruction("cmp rcx, 5");                                         // swap in place only over an existing hash
    emitter.instruction("jne __rt_shared_cell_hash_store_rebox");              // any other value is replaced by a hash box
    emitter.instruction("mov rax, QWORD PTR [r11 + 8]");                       // hand the previous hash owner back to the caller
    emitter.instruction("mov QWORD PTR [r11 + 8], rdi");                       // every alias of the box now sees the new hash
    emitter.instruction("ret");                                                // ownership of the new hash moved into the box
    emitter.label("__rt_shared_cell_hash_store_rebox");
    emitter.instruction("push rbp");                                           // establish an aligned frame for nested helper calls
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 32");                                        // reserve cell, hash, replaced payload and its kind
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                       // keep the cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                      // keep the transferred hash
    emitter.instruction("mov r11, QWORD PTR [rax]");                           // the payload this store replaces
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                      // keep it
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                      // with its representation tag
    emitter.instruction("mov eax, 5");                                         // runtime tag 5 boxes an associative array
    emitter.instruction("xor esi, esi");                                       // hashes have no high payload word
    emitter.instruction("call __rt_mixed_from_value");                         // box the hash (the box retains its own owner)
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                       // recover the cell
    emitter.instruction("mov QWORD PTR [r10], rax");                           // publish the box before releasing anything
    emitter.instruction("mov QWORD PTR [r10 + 8], 7");                         // record the representation every reader dispatches on
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                      // the caller's hash owner moved into the box
    emitter.instruction("call __rt_decref_hash");                              // drop it, leaving the box as the only owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                      // recover the replaced payload
    emitter.instruction("cmp QWORD PTR [rbp - 32], 7");                        // was it a box rather than a bare hash?
    emitter.instruction("jne __rt_shared_cell_hash_store_return");             // a bare hash goes back to the caller as the previous owner
    emitter.instruction("call __rt_decref_mixed");                             // release the replaced box (aliases keep their own shares)
    emitter.instruction("xor eax, eax");                                       // no previous hash owner remains for the caller
    emitter.label("__rt_shared_cell_hash_store_return");
    emitter.instruction("leave");                                              // restore the caller frame and stack
    emitter.instruction("ret");                                                // return the previous hash owner or 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    #[test]
    fn shared_global_ref_cells_release_mixed_and_hash_payloads_on_all_targets() {
        let targets = [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target { platform: Platform::MacOS, arch: Arch::AArch64, apple_variant: AppleVariant::IOS },
            Target { platform: Platform::MacOS, arch: Arch::AArch64, apple_variant: AppleVariant::IOSSimulator },
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ];
        for target in targets {
            let mut emitter = Emitter::new(target);
            emit_global_ref_cell(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_decref_mixed"), "{target:?}: missing boxed payload release");
            assert!(asm.contains("__rt_decref_hash"), "{target:?}: missing existing hash release");
            assert!(asm.contains("__rt_global_ref_cell_release_mixed"));
        }
    }
}
