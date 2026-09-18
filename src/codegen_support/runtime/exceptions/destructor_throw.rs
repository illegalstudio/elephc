//! Purpose:
//! Transfers an escaping eval-destructor Throwable from its owned box into native throw state.
//!
//! Called from:
//! - Native destructor dispatch after the Rust callback has returned status two.
//!
//! Key details:
//! - The input box is consumed and its raw object gains exactly one native exception owner.
//! - No longjmp occurs until every Rust callback frame has returned normally.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits ownership-balanced boxed-to-native Throwable propagation on every target.
pub fn emit_destructor_throw(emitter: &mut Emitter) {
    emitter.label_global("__rt_throw_boxed_destructor_exception");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- transfer the box owner to one independent native object owner --
            emitter.instruction("sub sp, sp, #32");                             // reserve box and object owners plus frame linkage
            emitter.instruction("stp x29, x30, [sp, #16]");                     // preserve the caller while ownership changes representation
            emitter.instruction("add x29, sp, #16");                            // establish the native transfer frame
            emitter.instruction("str x0, [sp]");                                // consume the owned Throwable box returned by eval
            emitter.instruction("bl __rt_mixed_unbox");                         // expose the validated Throwable object payload
            emitter.instruction("mov x0, x1");                                  // pass the raw object to the retain helper
            emitter.instruction("bl __rt_incref");                              // acquire the native exception owner before releasing the box
            emitter.instruction("str x0, [sp, #8]");                            // preserve that owner across boxed-cell release
            emitter.instruction("ldr x0, [sp]");                                // recover the input box being consumed
            emitter.instruction("bl __rt_decref_mixed");                        // release the box without destroying its now-retained object
            // -- publish the new exception before chaining and native unwinding --
            abi::emit_load_symbol_to_reg(emitter, "x1", "_exc_value", 0);
            emitter.instruction("ldr x0, [sp, #8]");                            // recover the raw owner acquired above
            abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
            emitter.instruction("bl __rt_exception_chain");                     // preserve any surrounding native exception chain
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore the caller after consuming every transfer-local owner
            emitter.instruction("add sp, sp, #32");                             // remove the transfer frame before propagating
            emitter.instruction("b __rt_throw_current");                        // the native collector boundary or PHP caller receives the throw
        }
        Arch::X86_64 => {
            // -- transfer the box owner to one independent native object owner --
            emitter.instruction("push rbp");                                    // preserve the caller and align helper calls
            emitter.instruction("mov rbp, rsp");                                // establish stable transfer-local slots
            emitter.instruction("sub rsp, 16");                                 // reserve the input box and new raw-object owner
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // consume the owned Throwable box returned by eval
            emitter.instruction("call __rt_mixed_unbox");                       // expose the validated raw Throwable payload
            emitter.instruction("mov rax, rdi");                                // pass that object to the internal retain helper
            emitter.instruction("call __rt_incref");                            // acquire one native exception owner
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // preserve the new owner across boxed-cell release
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // recover the owned input box
            emitter.instruction("call __rt_decref_mixed");                      // consume the box while the new raw owner keeps its object alive
            // -- publish the new exception before chaining and native unwinding --
            abi::emit_load_symbol_to_reg(emitter, "rdi", "_exc_value", 0);
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // recover the new raw exception owner
            abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
            emitter.instruction("call __rt_exception_chain");                   // preserve a surrounding native exception chain
            emitter.instruction("leave");                                       // discard transfer storage before propagation
            emitter.instruction("jmp __rt_throw_current");                      // native unwinding begins only after Rust returned
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target acquires a raw owner before consuming the box and throwing outside Rust.
    #[test]
    fn destructor_throw_transfers_one_boxed_owner_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_destructor_throw(&mut emitter);
            let asm = emitter.output();
            assert!(asm.find("__rt_incref").unwrap() < asm.find("__rt_decref_mixed").unwrap(), "{name}");
            assert!(asm.find("__rt_decref_mixed").unwrap() < asm.find("__rt_throw_current").unwrap(), "{name}");
            assert_eq!(asm.matches("__rt_incref").count(), 1, "{name}");
        }
    }
}
