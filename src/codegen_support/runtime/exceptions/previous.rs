//! Purpose:
//! Locates and reads previous-exception links without assuming a single Throwable layout.
//!
//! Called from:
//! - Throwable method lowering and exception-chain runtime helpers.
//!
//! Key details:
//! - Inputs and results are borrowed; these helpers neither allocate nor retain objects.
//! - Every Throwable finds its slot through class metadata (`_class_previous_slots`): this tree
//!   has no fixed-offset compact Throwable, the slot sits at `throwable_layout::PREVIOUS_OFFSET`.
//! - The slot holds a Mixed cell (typed property writes) or a raw object (native and eval
//!   writers); readers decide by the stored value's heap kind, as `getPrevious()` does.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Emits slot lookup and borrowed raw-object reads for every supported target.
pub fn emit_throwable_previous(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits ARM64 lookup returning the effective slot in x0 and its boxed flag in x1.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_throwable_previous_slot");
    // -- locate the physical property through the class metadata --
    emit_branch_if_null_container(emitter, "x0", "x9", "__rt_throwable_previous_missing");
    emitter.instruction("ldr x11, [x0]");                                       // preserve the runtime class id across address helpers
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_previous_slot_count", 0);
    emitter.instruction("cmp x11, x10");                                        // reject class ids outside the emitted descriptor table
    emitter.instruction("b.hs __rt_throwable_previous_missing");                // absent classes have no readable previous slot
    abi::emit_symbol_address(emitter, "x10", "_class_previous_slots");
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // load the offset and storage flags for this class
    emitter.instruction("cbz x10, __rt_throwable_previous_missing");            // preserve holes created by reachability pruning
    emitter.instruction("and x1, x10, #1");                                     // expose whether the effective slot owns a Mixed box
    emitter.instruction("and x11, x10, #-8");                                   // remove flags to recover the aligned byte offset
    emitter.instruction("add x0, x0, x11");                                     // locate the physical object-property slot
    emitter.instruction("tbz x10, #1, __rt_throwable_previous_slot_return");    // ordinary properties already expose their effective slot
    emitter.instruction("ldr x0, [x0]");                                        // reference properties point to stable external slot storage
    emitter.label("__rt_throwable_previous_slot_return");
    emitter.instruction("ret");                                                 // return a borrowed effective slot and its representation flag
    emitter.label("__rt_throwable_previous_missing");
    emitter.instruction("mov x0, #0");                                          // zero represents a missing effective slot
    emitter.instruction("mov x1, #0");                                          // missing slots have no storage flags
    emitter.instruction("ret");                                                 // never dereference an unknown layout

    emitter.label_global("__rt_throwable_previous");
    // -- read a borrowed link, unboxing only a stored Mixed cell --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller across slot lookup and Mixed unboxing
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("bl __rt_throwable_previous_slot");                     // resolve ordinary and reference property layouts
    emitter.instruction("cbz x0, __rt_throwable_previous_return");              // no slot means there is no previous exception
    emitter.instruction("ldr x0, [x0]");                                        // borrow the stored raw object or Mixed box
    emit_branch_if_null_container(emitter, "x0", "x9", "__rt_throwable_previous_null");
    emitter.instruction("ldr x10, [x0, #-8]");                                  // the stored value, not the declaration, decides its shape
    emitter.instruction("and x10, x10, #0xff");                                 // ignore collector and element flags
    emitter.instruction("cmp x10, #5");                                         // kind 5 is a Mixed cell from a typed property write
    emitter.instruction("b.ne __rt_throwable_previous_return");                 // a raw object stored by a native or eval writer is the link
    emitter.instruction("bl __rt_mixed_unbox");                                 // follow nested boxes and canonicalize nullable payloads
    emitter.instruction("cmp x0, #6");                                          // only an object contributes a previous-exception link
    emitter.instruction("csel x0, x1, xzr, eq");                                // return the borrowed object or canonical zero
    emitter.instruction("b __rt_throwable_previous_return");                    // skip raw-pointer null normalization after unboxing
    emitter.label("__rt_throwable_previous_null");
    emitter.instruction("mov x0, #0");                                          // canonicalize raw nullable sentinels for chain traversal
    emitter.label("__rt_throwable_previous_return");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller without changing ownership
    emitter.instruction("ret");                                                 // return the borrowed raw previous object or zero
}

/// Emits the same lookup on System V, returning the effective slot in rax and boxed flag in rdx.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_throwable_previous_slot");
    // -- locate the physical property through the class metadata --
    emit_branch_if_null_container(emitter, "rax", "r10", "__rt_throwable_previous_missing");
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // preserve the runtime class id across address helpers
    abi::emit_load_symbol_to_reg(emitter, "r10", "_class_previous_slot_count", 0);
    emitter.instruction("cmp r11, r10");                                        // bound the class-id-indexed metadata read
    emitter.instruction("jae __rt_throwable_previous_missing");                 // unknown classes have no readable previous slot
    abi::emit_symbol_address(emitter, "r10", "_class_previous_slots");
    emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");                  // recover this class's offset and storage flags
    emitter.instruction("test r10, r10");                                       // inspect the reachability-pruned descriptor
    emitter.instruction("jz __rt_throwable_previous_missing");                  // preserve holes in the class-id table
    emitter.instruction("mov rdx, r10");                                        // copy descriptor bits before discarding the offset
    emitter.instruction("and edx, 1");                                          // expose whether the effective slot owns a Mixed box
    emitter.instruction("mov r11, r10");                                        // preserve reference indirection while extracting the offset
    emitter.instruction("and r11, -8");                                         // remove flags from the aligned byte offset
    emitter.instruction("add rax, r11");                                        // locate the physical object-property slot
    emitter.instruction("test r10b, 2");                                        // does the property store a reference-slot pointer?
    emitter.instruction("jz __rt_throwable_previous_slot_return");              // non-reference properties already expose the effective slot
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // follow the reference without replacing its identity
    emitter.label("__rt_throwable_previous_slot_return");
    emitter.instruction("ret");                                                 // return the borrowed slot and its representation flag
    emitter.label("__rt_throwable_previous_missing");
    emitter.instruction("xor eax, eax");                                        // zero represents a missing slot
    emitter.instruction("xor edx, edx");                                        // missing slots have no storage flags
    emitter.instruction("ret");                                                 // avoid dereferencing unknown class layouts

    emitter.label_global("__rt_throwable_previous");
    // -- read a borrowed link, unboxing only a stored Mixed cell --
    emitter.instruction("push rbp");                                            // preserve the caller and align the stack for helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("call __rt_throwable_previous_slot");                   // resolve ordinary and reference layouts
    emitter.instruction("test rax, rax");                                       // check whether a previous slot exists
    emitter.instruction("jz __rt_throwable_previous_return");                   // no slot means no previous exception
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // borrow the stored raw object or Mixed box
    emit_branch_if_null_container(emitter, "rax", "r10", "__rt_throwable_previous_null");
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // the stored value, not the declaration, decides its shape
    emitter.instruction("and r10d, 0xff");                                      // ignore collector and element flags
    emitter.instruction("cmp r10d, 5");                                         // kind 5 is a Mixed cell from a typed property write
    emitter.instruction("jne __rt_throwable_previous_return");                  // a raw object stored by a native or eval writer is the link
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested Mixed cells and canonicalize nullable payloads
    emitter.instruction("cmp rax, 6");                                          // only an object supplies a previous-exception link
    emitter.instruction("mov eax, 0");                                          // default to null without changing the comparison flags
    emitter.instruction("cmove rax, rdi");                                      // select the borrowed object payload when its tag matches
    emitter.instruction("jmp __rt_throwable_previous_return");                  // skip raw-pointer normalization after unboxing
    emitter.label("__rt_throwable_previous_null");
    emitter.instruction("xor eax, eax");                                        // normalize raw nullable sentinels for chain traversal
    emitter.label("__rt_throwable_previous_return");
    emitter.instruction("pop rbp");                                             // restore the caller without changing ownership
    emitter.instruction("ret");                                                 // return the borrowed raw previous object or zero
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported ABI bounds metadata, has no fixed-offset layout, and unboxes without retaining.
    #[test]
    fn previous_readers_cover_boxed_raw_and_reference_layouts_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_throwable_previous(&mut emitter);
            let asm = emitter.output();
            assert!(asm.find("_class_previous_slot_count").unwrap() < asm.find("_class_previous_slots").unwrap(), "{name}");
            assert!(!asm.contains("__rt_throwable_previous_compact") && asm.contains("__rt_mixed_unbox"), "{name}");
            assert!(!asm.contains("__rt_incref") && !asm.contains("__rt_heap_alloc"), "{name}");
        }
    }
}
