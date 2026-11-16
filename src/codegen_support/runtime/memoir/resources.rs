//! Purpose:
//! Preserves PHP resource identity and weak inventory publication during Mixed boxing.
//!
//! Called from:
//! - Compiler runtime emission and memory integration tests.
//!
//! Key details:
//! - Called inside the fixed Mixed frame: ARM payload/node at sp+8/+16/+24; x86 at rbp-16/-24/-32.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits resource subtype exclusions, shared resource ids and canonical cell reuse.
pub(super) fn emit_identity(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.label("__rt_mixed_from_value_resource");
            emitter.instruction("cmp x2, #2");                                  // resource kind 2 = the raw incremental-hash context
            emitter.instruction("b.eq __rt_mixed_from_value_alloc");            // a HashContext is an OBJECT in PHP and must consume no resource id
            emitter.instruction("cmp x2, #5");                                  // resource kind 5 = eval's inert handle on the same HashContext
            emitter.instruction("b.eq __rt_mixed_from_value_alloc");            // eval hashing must not shift the ids the host program's fopen() reports
            emitter.instruction("cmp x2, #6");                                  // resource kind 6 = a libcurl easy handle
            emitter.instruction("b.eq __rt_mixed_from_value_alloc");            // a CurlHandle is an OBJECT in PHP and must consume no resource id
            emitter.instruction("cmp x2, #7");                                  // resource kind 7 = a libcurl multi handle
            emitter.instruction("b.eq __rt_mixed_from_value_alloc");            // a CurlMultiHandle is an OBJECT in PHP and must consume no resource id
            emitter.instruction("cmp x2, #8");                                  // resource kind 8 = a libcurl share handle
            emitter.instruction("b.eq __rt_mixed_from_value_alloc");            // a CurlShareHandle/CurlSharePersistentHandle is an OBJECT in PHP and must consume no resource id
            emitter.instruction("mov x0, x1");                                  // move the native resource payload into the registry argument
            emitter.instruction("bl __rt_resource_id_of");                      // bind a display id if this payload does not already have one
            emitter.instruction("ldr x1, [sp, #8]");                            // reload native payload for incarnation tracking
            emitter.instruction("ldr x2, [sp, #16]");                           // reload resource subtype for introspection filtering
            emitter.instruction("bl __rt_resource_inventory_register");         // record this PHP id exactly once for get_resources
            emitter.instruction("str x0, [sp, #24]");                           // keep the weak cell slot across allocation
            emitter.instruction("cbz x0, __rt_mixed_from_value_alloc");         // synthesized resources need no shared node
            emitter.instruction("ldr x0, [x0, #40]");                           // resolve any already-boxed owner of this incarnation
            emitter.instruction("cbz x0, __rt_mixed_from_value_alloc");         // the first owner creates the canonical cell
            emitter.instruction("bl __rt_incref");                              // each alias owns one reference to the same cell
            emitter.instruction("b __rt_mixed_from_value_done");                // never allocate an independently destructible alias
        }
        Arch::X86_64 => {
            emitter.label("__rt_mixed_from_value_resource");
            emitter.instruction("cmp QWORD PTR [rbp - 24], 2");                 // resource kind 2 = the raw incremental-hash context
            emitter.instruction("je __rt_mixed_from_value_alloc");              // a HashContext is an OBJECT in PHP and must consume no resource id
            emitter.instruction("cmp QWORD PTR [rbp - 24], 5");                 // resource kind 5 = eval's inert handle on the same HashContext
            emitter.instruction("je __rt_mixed_from_value_alloc");              // eval hashing must not shift the ids the host program's fopen() reports
            emitter.instruction("cmp QWORD PTR [rbp - 24], 6");                 // resource kind 6 = a libcurl easy handle
            emitter.instruction("je __rt_mixed_from_value_alloc");              // a CurlHandle is an OBJECT in PHP and must consume no resource id
            emitter.instruction("cmp QWORD PTR [rbp - 24], 7");                 // resource kind 7 = a libcurl multi handle
            emitter.instruction("je __rt_mixed_from_value_alloc");              // a CurlMultiHandle is an OBJECT in PHP and must consume no resource id
            emitter.instruction("cmp QWORD PTR [rbp - 24], 8");                 // resource kind 8 = a libcurl share handle
            emitter.instruction("je __rt_mixed_from_value_alloc");              // a CurlShareHandle/CurlSharePersistentHandle is an OBJECT in PHP and must consume no resource id
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // move the native resource payload into the registry argument
            emitter.instruction("call __rt_resource_id_of");                    // bind a display id if this payload does not already have one
            emitter.instruction("mov rdi, rax");                                // pass the resolved PHP id to incarnation tracking
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // reload native payload for the inventory node
            emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");               // reload resource subtype for filtering
            emitter.instruction("call __rt_resource_inventory_register");       // record this PHP id exactly once for get_resources
            emitter.instruction("mov QWORD PTR [rbp - 32], rax");               // keep the weak cell slot across allocation
            emitter.instruction("test rax, rax");                               // synthesized resources have no inventory node
            emitter.instruction("jz __rt_mixed_from_value_alloc");              // box synthesized resources normally
            emitter.instruction("mov rax, QWORD PTR [rax + 40]");               // resolve any already-boxed owner of this incarnation
            emitter.instruction("test rax, rax");                               // the first owner creates the canonical cell
            emitter.instruction("jz __rt_mixed_from_value_alloc");              // allocate only when no owner exists yet
            emitter.instruction("call __rt_incref");                            // each alias owns one reference to the same cell
            emitter.instruction("jmp __rt_mixed_from_value_done");              // never allocate an independently destructible alias
        }
    }
}

/// Publishes a completed box weakly in its resource inventory node.
pub(super) fn emit_publication(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #24]");                           // load the optional inventory node
            emitter.instruction("cbz x9, __rt_mixed_from_value_done");          // ordinary values have no inventory identity
            emitter.instruction("str x0, [x9, #40]");                           // publish a weak pointer without retaining an immortal owner
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rbp - 32]");               // load the optional inventory node
            emitter.instruction("test r10, r10");                               // ordinary values have no inventory identity
            emitter.instruction("jz __rt_mixed_from_value_done");               // return ordinary boxes without registration
            emitter.instruction("mov QWORD PTR [r10 + 40], rax");               // publish a weak pointer without retaining an immortal owner
        }
    }
}
