//! Purpose:
//! Visits and releases object-owned cells stored by eval outside PHP property storage.
//!
//! Called from:
//! - Native GC object counting/marking and the final object deep-free path.
//!
//! Key details:
//! - Optional callbacks use the C ABI on every target and return borrowed child cells.
//! - Counting includes these edges so retained closure receivers are not false GC roots.
//! - Marking happens after the parent mark; cycles cannot recurse indefinitely.

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
    // -- preserve enumeration state across Rust callbacks and recursive marking --
    emitter.instruction("sub sp, sp, #64");                                     // reserve aligned state and saved frame registers
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable frame for nested calls
    emitter.instruction("str x0, [sp]");                                        // save the raw owning object identity
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the count-versus-mark mode
    emitter.instruction("str xzr, [sp, #16]");                                  // enumerate from the first child
    emitter.label("__rt_gc_eval_object_children_loop");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_elephc_eval_object_gc_child_fn", 0);
    emitter.instruction("cbz x10, __rt_gc_eval_object_children_done");          // programs without eval have no external object edges
    emitter.instruction("ldr x0, [sp]");                                        // pass the owner identity to the C callback
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the current child index
    emitter.instruction("blr x10");                                             // borrow one retained child without allocating or changing ownership
    emitter.instruction("cbz x0, __rt_gc_eval_object_children_done");           // zero terminates this object's child sequence
    emitter.instruction("ldr x9, [sp, #8]");                                    // select the requested collector phase
    emitter.instruction("cbnz x9, __rt_gc_eval_object_children_mark");          // recursively mark children only during reachability traversal
    emitter.instruction("bl __rt_gc_note_child_ref");                           // count the real incoming edge to this child cell
    emitter.instruction("b __rt_gc_eval_object_children_next");                 // continue enumeration after recording the edge
    emitter.label("__rt_gc_eval_object_children_mark");
    emitter.instruction("bl __rt_gc_mark_reachable");                           // mark the child cell and its transitive payload graph
    emitter.label("__rt_gc_eval_object_children_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // recover the index after callback or recursive clobbers
    emitter.instruction("add x9, x9, #1");                                      // advance to the next retained child
    emitter.instruction("str x9, [sp, #16]");                                   // persist the next enumeration index
    emitter.instruction("b __rt_gc_eval_object_children_loop");                 // continue until the callback reports no child
    emitter.label("__rt_gc_eval_object_children_done");
    emitter.instruction("mov x0, xzr");                                         // ARM64 stores incoming counts in child headers
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // discard the traversal state
    emitter.instruction("ret");                                                 // resume the ordinary object-property walk

    // -- tail-call final ownership release while the object identity is still valid --
    emitter.label_global("__rt_eval_object_release_children");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_elephc_eval_object_release_fn", 0);
    emitter.instruction("cbz x10, __rt_eval_object_release_children_done");     // skip release when eval has never installed callbacks
    emitter.instruction("br x10");                                              // let the C callback detach and release the owner's cells
    emitter.label("__rt_eval_object_release_children_done");
    emitter.instruction("ret");                                                 // objects without eval owners require no extra cleanup
}

/// Emits System V traversal with rdi = object, rsi = candidate, rdx = mark flag.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_eval_object_children");
    // -- preserve enumeration state across Rust callbacks and recursive marking --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame across nested calls
    emitter.instruction("sub rsp, 48");                                         // reserve six words while maintaining C-call stack alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the raw owning object identity
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the candidate whose incoming edges are counted
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the count-versus-mark mode
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // enumerate from the first child
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start with no matching incoming edges
    emitter.label("__rt_gc_eval_object_children_loop");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_eval_object_gc_child_fn", 0);
    emitter.instruction("test r10, r10");                                       // check whether eval installed the optional edge enumerator
    emitter.instruction("jz __rt_gc_eval_object_children_done");                // programs without eval have no external object edges
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the owner identity using the C ABI
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the current child index
    emitter.instruction("call r10");                                            // borrow one retained child without changing ownership
    emitter.instruction("test rax, rax");                                       // check for the end of this object's child sequence
    emitter.instruction("jz __rt_gc_eval_object_children_done");                // zero terminates enumeration
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // select the requested collector phase
    emitter.instruction("jne __rt_gc_eval_object_children_mark");               // recursively mark children during reachability traversal
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // compare this child with the candidate being recounted
    emitter.instruction("jne __rt_gc_eval_object_children_next");               // other children do not contribute to this candidate
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // count one actual owning edge to the candidate
    emitter.instruction("jmp __rt_gc_eval_object_children_next");               // continue enumeration after recording the edge
    emitter.label("__rt_gc_eval_object_children_mark");
    emitter.instruction("call __rt_gc_mark_reachable");                         // pass the child in the runtime's rax convention
    emitter.label("__rt_gc_eval_object_children_next");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance the index preserved across recursive calls
    emitter.instruction("jmp __rt_gc_eval_object_children_loop");               // continue until the callback reports no child
    emitter.label("__rt_gc_eval_object_children_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the candidate's incoming edge count
    emitter.instruction("mov rsp, rbp");                                        // discard enumeration locals
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // resume the ordinary object-property walk

    // -- tail-call final ownership release using the incoming C object argument --
    emitter.label_global("__rt_eval_object_release_children");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_eval_object_release_fn", 0);
    emitter.instruction("test r10, r10");                                       // check whether eval installed the optional release callback
    emitter.instruction("jz __rt_eval_object_release_children_done");           // skip objects when no eval ownership callback exists
    emitter.instruction("jmp r10");                                             // let the C callback detach and release the owner's cells
    emitter.label("__rt_eval_object_release_children_done");
    emitter.instruction("ret");                                                 // objects without eval owners require no extra cleanup
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{platform::Target, RuntimeFeatures};

    /// Every target traverses external edges in both GC phases and releases them after PHP destructors.
    #[test]
    fn eval_object_edges_cover_all_supported_collectors_and_release_paths() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gc_eval_object_children(&mut emitter);
            let helper = emitter.output();
            assert!(helper.contains("_elephc_eval_object_gc_child_fn"), "{name}");
            assert!(helper.contains("_elephc_eval_object_release_fn"), "{name}");
            assert!(helper.contains("__rt_gc_mark_reachable"), "{name}");
            let indirect = if target.arch == Arch::AArch64 { "blr x10" } else { "call r10" };
            assert!(helper.contains(indirect), "{name}");

            let mut emitter = Emitter::new(target);
            super::super::emit_gc_collect_cycles(&mut emitter);
            assert!(emitter.output().contains("__rt_gc_eval_object_children"), "{name}");
            let mut emitter = Emitter::new(target);
            super::super::emit_gc_mark_reachable(&mut emitter);
            assert!(emitter.output().contains("__rt_gc_eval_object_children"), "{name}");
            let mut emitter = Emitter::new(target);
            super::super::emit_object_free_deep(&mut emitter, RuntimeFeatures::none());
            let free = emitter.output();
            let destructor = free.find("__rt_call_object_destructor").unwrap();
            let release = free.find("__rt_eval_object_release_children").unwrap();
            assert!(destructor < release, "{name}");
        }
    }
}
