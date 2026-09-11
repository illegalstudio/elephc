//! Purpose:
//! Visits and releases object-owned cells stored by eval outside PHP properties.
//!
//! Called from:
//! - Native GC object counting/marking and the final object deep-free path.
//!
//! Key details:
//! - Optional callbacks use the C ABI and return borrowed child cells.
//! - Counting exposes hidden receiver edges to cycle detection.
//! - Final release propagates a protected child-destructor exception after Rust returns.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits optional eval object-edge traversal and final-release C-ABI helpers.
pub fn emit_gc_eval_object_children(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits AAPCS64 traversal with x0 = object, x1 = unused candidate, x2 = mark flag.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_eval_object_children");
    emitter.instruction("sub sp, sp, #64");                                     // reserve aligned traversal state and linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable frame for nested calls
    emitter.instruction("str x0, [sp]");                                        // save the raw owning object identity
    emitter.instruction("str x2, [sp, #8]");                                    // preserve count-versus-mark mode
    emitter.instruction("str xzr, [sp, #16]");                                  // enumerate from the first child
    emitter.label("__rt_gc_eval_object_children_loop");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_elephc_eval_object_gc_child_fn", 0);
    emitter.instruction("cbz x10, __rt_gc_eval_object_children_done");          // programs without eval have no hidden object edges
    emitter.instruction("ldr x0, [sp]");                                        // pass the owner identity through the C ABI
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the current child index
    emitter.instruction("blr x10");                                             // borrow one retained child without changing ownership
    emitter.instruction("cbz x0, __rt_gc_eval_object_children_done");           // zero terminates this object's child sequence
    emitter.instruction("ldr x9, [sp, #8]");                                    // select the requested collector phase
    emitter.instruction("cbnz x9, __rt_gc_eval_object_children_mark");          // recursively mark only during reachability traversal
    emitter.instruction("bl __rt_gc_note_child_ref");                           // count the actual incoming edge to this child
    emitter.instruction("b __rt_gc_eval_object_children_next");                 // continue after recording the edge
    emitter.label("__rt_gc_eval_object_children_mark");
    emitter.instruction("bl __rt_gc_mark_reachable");                           // mark the child and its transitive graph
    emitter.label("__rt_gc_eval_object_children_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // recover the index after nested calls
    emitter.instruction("add x9, x9, #1");                                      // advance to the next retained child
    emitter.instruction("str x9, [sp, #16]");                                   // persist the updated index
    emitter.instruction("b __rt_gc_eval_object_children_loop");                 // continue until the callback returns zero
    emitter.label("__rt_gc_eval_object_children_done");
    emitter.instruction("mov x0, xzr");                                         // ARM64 stores incoming counts in child headers
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // discard traversal state
    emitter.instruction("ret");                                                 // resume ordinary object-property traversal

    emitter.label_global("__rt_eval_object_release_children");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_elephc_eval_object_release_fn", 0);
    emitter.instruction("cbz x10, __rt_eval_object_release_children_done");     // skip objects when eval installed no owner callbacks
    emitter.instruction("sub sp, sp, #16");                                     // preserve linkage across the Rust C callback
    emitter.instruction("str x30, [sp, #8]");                                   // save the native caller return address
    emitter.instruction("blr x10");                                             // detach and release the receiver behind a protected boundary
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore linkage before propagating a returned exception status
    emitter.instruction("add sp, sp, #16");                                     // discard the callback frame
    emitter.instruction("cbz x0, __rt_eval_object_release_children_done");      // zero means every receiver owner was released cleanly
    emitter.instruction("b __rt_throw_current");                                // propagate the pending Throwable after Rust has returned
    emitter.label("__rt_eval_object_release_children_done");
    emitter.instruction("ret");                                                 // objects without eval owners need no extra cleanup
}

/// Emits System V traversal with rdi = object, rsi = candidate, rdx = mark flag.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_eval_object_children");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame across nested calls
    emitter.instruction("sub rsp, 48");                                         // reserve aligned enumeration state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the raw owning object identity
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the candidate whose incoming edges are counted
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve count-versus-mark mode
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // enumerate from the first child
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start with no matching incoming edges
    emitter.label("__rt_gc_eval_object_children_loop");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_eval_object_gc_child_fn", 0);
    emitter.instruction("test r10, r10");                                       // check whether eval installed an edge enumerator
    emitter.instruction("jz __rt_gc_eval_object_children_done");                // programs without eval have no hidden edges
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the owner identity through the C ABI
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the current child index
    emitter.instruction("call r10");                                            // borrow one retained child without changing ownership
    emitter.instruction("test rax, rax");                                       // check for the end of the sequence
    emitter.instruction("jz __rt_gc_eval_object_children_done");                // zero terminates enumeration
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // select the requested collector phase
    emitter.instruction("jne __rt_gc_eval_object_children_mark");               // recursively mark during reachability traversal
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // compare this child with the recounted candidate
    emitter.instruction("jne __rt_gc_eval_object_children_next");               // other children do not contribute to this candidate
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // count one actual owning edge
    emitter.instruction("jmp __rt_gc_eval_object_children_next");               // continue after recording the edge
    emitter.label("__rt_gc_eval_object_children_mark");
    emitter.instruction("call __rt_gc_mark_reachable");                         // mark the child and its transitive graph
    emitter.label("__rt_gc_eval_object_children_next");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next retained child
    emitter.instruction("jmp __rt_gc_eval_object_children_loop");               // continue until the callback returns zero
    emitter.label("__rt_gc_eval_object_children_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return matching incoming edges
    emitter.instruction("leave");                                               // discard traversal state and restore the frame
    emitter.instruction("ret");                                                 // resume ordinary object-property traversal

    emitter.label_global("__rt_eval_object_release_children");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_eval_object_release_fn", 0);
    emitter.instruction("test r10, r10");                                       // check whether eval installed a release callback
    emitter.instruction("jz __rt_eval_object_release_children_done");           // programs without eval owners need no extra cleanup
    emitter.instruction("push rbp");                                            // align the stack and preserve the native caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish linkage across the Rust C callback
    emitter.instruction("call r10");                                            // detach and release the receiver behind a protected boundary
    emitter.instruction("leave");                                               // restore the native caller frame before propagation
    emitter.instruction("test rax, rax");                                       // inspect the protected cleanup status
    emitter.instruction("jnz __rt_throw_current");                              // propagate after every Rust frame has returned
    emitter.label("__rt_eval_object_release_children_done");
    emitter.instruction("ret");                                                 // return after clean or absent owner cleanup
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{platform::Target, RuntimeFeatures};

    /// All targets traverse hidden edges and release them only on the final path.
    #[test]
    fn eval_object_edges_cover_all_supported_collectors_and_release_paths() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gc_eval_object_children(&mut emitter);
            let helper = emitter.output();
            assert!(helper.contains("_elephc_eval_object_gc_child_fn"), "{name}");
            assert!(helper.contains("_elephc_eval_object_release_fn"), "{name}");
            assert!(helper.contains("__rt_gc_mark_reachable"), "{name}");

            let mut emitter = Emitter::new(target);
            super::super::emit_object_free_deep(&mut emitter, RuntimeFeatures::none());
            let free = emitter.output();
            let final_release = free.find("__rt_object_free_deep_release:").unwrap();
            let receiver_release = free.find("__rt_eval_object_release_children").unwrap();
            assert!(final_release < receiver_release, "{name}");
        }
    }
}
