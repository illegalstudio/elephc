//! Purpose:
//! Drains active Buffer descriptors after fatal control flow abandons scalar handles.
//!
//! Called from:
//! - The Parallel worker fatal trampoline after ordinary frame and descriptor cleanup.
//!
//! Key details:
//! - Descriptor activity is cleared before payload release to make re-entry fail closed.
//! - Only active descriptors release payloads; inactive and retired slots are skipped.

use super::{BUFFER_DESCRIPTOR_SIZE, BUFFER_REGISTRY_CAPACITY};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_buffer_registry_drain` for the active target.
pub fn emit_buffer_registry_drain(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_buffer_registry_drain_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: buffer_registry_drain ---");
    emitter.label_global("__rt_buffer_registry_drain");
    emitter.instruction("sub sp, sp, #48");                                     // reserve an aligned scan frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame pointer and return address
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve descriptor index and address
    emitter.instruction("add x29, sp, #32");                                    // establish the drain frame pointer
    emitter.instruction("mov x19, #1");                                         // descriptor index zero is permanently invalid
    emitter.label("__rt_buffer_registry_drain_loop");
    emitter.instruction(&format!("cmp x19, #{}", BUFFER_REGISTRY_CAPACITY));    // reached or passed the last usable descriptor?
    emitter.instruction("b.hi __rt_buffer_registry_drain_done");                // finish after scanning the final slot inclusively
    abi::emit_symbol_address(emitter, "x20", "_buffer_registry");
    emitter.instruction(&format!("mov x9, #{}", BUFFER_DESCRIPTOR_SIZE));       // materialize the descriptor stride
    emitter.instruction("madd x20, x19, x9, x20");                              // address this descriptor
    emitter.instruction("ldr x9, [x20, #32]");                                  // load the active marker
    emitter.instruction("cbz x9, __rt_buffer_registry_drain_next");             // inactive descriptors own no payload
    emitter.instruction("str xzr, [x20, #32]");                                 // detach activity before any nested cleanup can re-enter
    emitter.instruction("ldr x0, [x20]");                                       // load the owned payload pointer
    emitter.instruction("str xzr, [x20]");                                      // clear the payload owner before release
    emitter.instruction("str xzr, [x20, #8]");                                  // clear logical length metadata
    emitter.instruction("str xzr, [x20, #16]");                                 // clear element stride metadata
    emitter.instruction("bl __rt_heap_free");                                   // return the abandoned payload to the worker arena
    emitter.label("__rt_buffer_registry_drain_next");
    emitter.instruction("add x19, x19, #1");                                    // advance to the next descriptor
    emitter.instruction("b __rt_buffer_registry_drain_loop");                   // continue the bounded registry scan
    emitter.label("__rt_buffer_registry_drain_done");
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved scan registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore caller frame state
    emitter.instruction("add sp, sp, #48");                                     // release the scan frame
    emitter.instruction("ret");                                                 // return with no active Buffer payload owners
}

/// Scans x86_64 Buffer descriptors, clearing each active owner before releasing its payload.
fn emit_buffer_registry_drain_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_registry_drain ---");
    emitter.label_global("__rt_buffer_registry_drain");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the drain frame
    emitter.instruction("push r12");                                            // preserve descriptor index
    emitter.instruction("push r13");                                            // preserve descriptor address
    emitter.instruction("mov r12, 1");                                          // descriptor index zero is permanently invalid
    emitter.label("__rt_buffer_registry_drain_loop_x86");
    emitter.instruction(&format!("cmp r12, {}", BUFFER_REGISTRY_CAPACITY));     // reached or passed the last usable descriptor?
    emitter.instruction("ja __rt_buffer_registry_drain_done_x86");              // finish after scanning the final slot inclusively
    abi::emit_symbol_address(emitter, "r13", "_buffer_registry");
    emitter.instruction(&format!("imul r10, r12, {}", BUFFER_DESCRIPTOR_SIZE)); // scale the descriptor index
    emitter.instruction("add r13, r10");                                        // address this descriptor
    emitter.instruction("cmp QWORD PTR [r13 + 32], 0");                         // inspect the active marker
    emitter.instruction("je __rt_buffer_registry_drain_next_x86");              // inactive descriptors own no payload
    emitter.instruction("mov QWORD PTR [r13 + 32], 0");                         // detach activity before any nested cleanup can re-enter
    emitter.instruction("mov rax, QWORD PTR [r13]");                            // load the owned payload pointer
    emitter.instruction("mov QWORD PTR [r13], 0");                              // clear the payload owner before release
    emitter.instruction("mov QWORD PTR [r13 + 8], 0");                          // clear logical length metadata
    emitter.instruction("mov QWORD PTR [r13 + 16], 0");                         // clear element stride metadata
    emitter.instruction("call __rt_heap_free");                                 // return the abandoned payload to the worker arena
    emitter.label("__rt_buffer_registry_drain_next_x86");
    emitter.instruction("inc r12");                                             // advance to the next descriptor
    emitter.instruction("jmp __rt_buffer_registry_drain_loop_x86");             // continue the bounded registry scan
    emitter.label("__rt_buffer_registry_drain_done_x86");
    emitter.instruction("pop r13");                                             // restore descriptor address register
    emitter.instruction("pop r12");                                             // restore descriptor index
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return with no active Buffer payload owners
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Pins detach-before-free ordering and bounded registry traversal on both runtime ABIs.
    #[test]
    fn fatal_buffer_drain_detaches_before_free_on_both_abis() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emitter.ctx_register = true;
            emit_buffer_registry_drain(&mut emitter);
            let asm = emitter.output();
            let detach = if target.arch == Arch::AArch64 {
                asm.find("str xzr, [x20, #32]")
            } else {
                asm.find("mov QWORD PTR [r13 + 32], 0")
            }
            .expect("descriptor detach");
            let release = asm.find("__rt_heap_free").expect("payload release");
            assert!(detach < release, "{target:?}:\n{asm}");
            assert!(asm.contains(&BUFFER_REGISTRY_CAPACITY.to_string()), "{asm}");
        }
    }
}
