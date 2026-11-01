//! Purpose:
//! Converts boxed associative object results into the native typed hash layout.
//!
//! Called from:
//! - The EIR eval-result conversion shared by method and property boundaries.
//!
//! Key details:
//! - Clone before changing representation so aliases keep their boxed entries.
//! - Retain each raw object before releasing the cloned entry's Mixed-cell owner.

use super::*;
use crate::codegen_support::emit::Emitter;

/// Produces an owned object-valued hash from a borrowed boxed eval result.
pub(super) fn emit_eval_owned_object_hash(ctx: &mut FunctionContext<'_>) {
    let loop_label = ctx.next_label("eval_object_hash_loop");
    let done_label = ctx.next_label("eval_object_hash_done");
    emit_object_hash_conversion(ctx.emitter, &loop_label, &done_label);
}

/// Emits the target-specific conversion while keeping call-site labels unique.
fn emit_object_hash_conversion(emitter: &mut Emitter, loop_label: &str, done_label: &str) {
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                   // pass the borrowed hash payload to the clone helper
            abi::emit_call_label(emitter, "__rt_hash_clone_shallow");
            emitter.instruction("sub sp, sp, #32");                              // reserve hash, cursor, entry address, and old-cell slots
            emitter.instruction("str x0, [sp]");                                 // preserve the owned hash across runtime calls
            emitter.instruction("str xzr, [sp, #8]");                            // begin at the first insertion-order entry
            emitter.label(&loop_label);
            emitter.instruction("ldr x0, [sp]");                                 // reload the cloned hash
            emitter.instruction("ldr x1, [sp, #8]");                             // reload the insertion-order cursor
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmn x0, #1");                                   // the negative-one cursor marks iteration completion
            emitter.instruction(&format!("b.eq {done_label}"));                  // publish the completed object representation
            emitter.instruction("str x0, [sp, #8]");                             // preserve the next cursor before conversion calls
            emitter.instruction("cmp x5, #6");                                   // entries may already contain retained raw objects
            emitter.instruction(&format!("b.eq {loop_label}"));                  // preserve an already-native object entry
            emitter.instruction("str x6, [sp, #16]");                            // preserve the mutable entry value address
            emitter.instruction("str x3, [sp, #24]");                            // preserve the cloned Mixed-cell owner for release
            emitter.instruction("mov x0, x3");                                   // pass the boxed object cell to the unbox helper
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("mov x0, x1");                                   // retain the object's raw payload for the new slot owner
            abi::emit_call_label(emitter, "__rt_incref");
            emitter.instruction("ldr x6, [sp, #16]");                            // recover the entry after the retain call
            emitter.instruction("str x0, [x6]");                                 // publish the raw object pointer instead of a Mixed-cell pointer
            emitter.instruction("str xzr, [x6, #8]");                            // object entries have no high payload word
            emitter.instruction("mov x9, #6");                                   // runtime value tag six denotes a raw object
            emitter.instruction("str x9, [x6, #16]");                            // update per-entry ownership metadata before releasing the old cell
            emitter.instruction("ldr x0, [sp, #24]");                            // release only the cloned entry's old Mixed-cell owner
            abi::emit_call_label(emitter, "__rt_decref_mixed");
            emitter.instruction(&format!("b {loop_label}"));                     // convert the next insertion-order entry
            emitter.label(&done_label);
            emitter.instruction("ldr x0, [sp]");                                 // return the owned normalized hash
            emitter.instruction("mov x9, #6");                                   // publish the uniform raw-object value representation
            emitter.instruction("str x9, [x0, #16]");                            // update the hash-wide value type, including empty maps
            emitter.instruction("add sp, sp, #32");                              // release the conversion scratch frame
        }
        Arch::X86_64 => {
            abi::emit_call_label(emitter, "__rt_hash_clone_shallow");
            emitter.instruction("sub rsp, 32");                                  // reserve hash, cursor, entry address, and old-cell slots
            emitter.instruction("mov QWORD PTR [rsp], rax");                     // preserve the owned hash across runtime calls
            emitter.instruction("mov QWORD PTR [rsp + 8], 0");                   // begin at the first insertion-order entry
            emitter.label(&loop_label);
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                     // reload the cloned hash
            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                 // reload the insertion-order cursor
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmp rax, -1");                                  // the negative-one cursor marks iteration completion
            emitter.instruction(&format!("je {done_label}"));                    // publish the completed object representation
            emitter.instruction("mov QWORD PTR [rsp + 8], rax");                 // preserve the next cursor before conversion calls
            emitter.instruction("cmp r9, 6");                                    // entries may already contain retained raw objects
            emitter.instruction(&format!("je {loop_label}"));                    // preserve an already-native object entry
            emitter.instruction("mov QWORD PTR [rsp + 16], r10");                // preserve the mutable entry value address
            emitter.instruction("mov QWORD PTR [rsp + 24], rcx");                // preserve the cloned Mixed-cell owner for release
            emitter.instruction("mov rax, rcx");                                 // pass the boxed object cell to the unbox helper
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("mov rax, rdi");                                 // retain the object's raw payload for the new slot owner
            abi::emit_call_label(emitter, "__rt_incref");
            emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                // recover the entry after the retain call
            emitter.instruction("mov QWORD PTR [r10], rax");                     // publish the raw object pointer instead of a Mixed-cell pointer
            emitter.instruction("mov QWORD PTR [r10 + 8], 0");                   // object entries have no high payload word
            emitter.instruction("mov QWORD PTR [r10 + 16], 6");                  // update per-entry ownership metadata before releasing the old cell
            emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                // release only the cloned entry's old Mixed-cell owner
            abi::emit_call_label(emitter, "__rt_decref_mixed");
            emitter.instruction(&format!("jmp {loop_label}"));                   // convert the next insertion-order entry
            emitter.label(&done_label);
            emitter.instruction("mov rax, QWORD PTR [rsp]");                     // return the owned normalized hash
            emitter.instruction("mov QWORD PTR [rax + 16], 6");                  // update the hash-wide value type, including empty maps
            emitter.instruction("add rsp, 32");                                  // release the conversion scratch frame
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    #[test]
    fn associative_object_results_preserve_ownership_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_object_hash_conversion(&mut emitter, "convert_next", "convert_done");
            let asm = emitter.output();
            let clone = asm.find("__rt_hash_clone_shallow").expect("clone borrowed source");
            let iterate = asm.find("__rt_hash_iter_next").expect("preserve insertion order");
            let retain = asm.find("__rt_incref").expect("retain the raw object");
            let release = asm.find("__rt_decref_mixed").expect("release the replaced cell");
            let (publish, metadata, reserve, restore) = match target.arch {
                Arch::AArch64 => ("str x0, [x6]", "str x9, [x6, #16]", "sub sp, sp, #32", "add sp, sp, #32"),
                Arch::X86_64 => ("mov QWORD PTR [r10], rax", "mov QWORD PTR [r10 + 16], 6", "sub rsp, 32", "add rsp, 32"),
            };
            let publish = asm.find(publish).expect("publish raw object");
            let metadata = asm.find(metadata).expect("publish entry ownership tag");
            assert!(clone < iterate && iterate < retain && retain < publish && publish < metadata && metadata < release, "{name}: {asm}");
            assert!(asm.contains(reserve) && asm.contains(restore), "{name}: aligned scratch frame");
        }
    }
}
