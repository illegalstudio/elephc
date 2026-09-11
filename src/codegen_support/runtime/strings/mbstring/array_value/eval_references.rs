//! Purpose:
//! Resolves eval-side array references before the native reader copies a stored entry.
//!
//! Called from:
//! - The protected mbstring array-value callback when eval support is emitted.
//!
//! Key details:
//! - The original array pointer is only an eval metadata lookup token.
//! - Rust receives explicit native copy, release, and pending-Throwable publication actions.
//! - A returned owner is published before status inspection so outer cleanup always sees it.

use super::*;

/// Reads AArch64 eval reference metadata using the original argument identity and current key.
pub(super) fn aarch64(emitter: &mut Emitter, graph: bool) {
    let name = if graph { "__rt_mbstring_graph_value" } else { "__rt_mbstring_array_value" };
    emitter.instruction("ldr x0, [sp, #320]");                                  // recover the optional active eval context
    emitter.instruction(&format!("cbz x0, {name}_eval_no_alias"));              // ordinary native calls read their retained physical array
    for (offset, symbol) in [(72, "__rt_mbstring_clone"), (80, "__rt_mbstring_release"), (88, "__rt_mbstring_publish_throw")] {
        abi::emit_symbol_address(emitter, "x9", symbol);
        emitter.instruction(&format!("str x9, [sp, #{offset}]"));               // provide one non-unwinding ownership action to the eval adapter
    }
    emitter.instruction("ldr x1, [sp, #328]");                                  // recover the retained/source identity descriptor
    emitter.instruction("ldr x1, [x1, #8]");                                    // pass only the original boxed-handle identity token
    emitter.instruction("add x2, sp, #24");                                     // borrow the current normalized integer or binary string key
    emitter.instruction("add x3, sp, #72");                                     // pass the complete native ownership-action table
    emitter.instruction("ldr x4, [sp, #344]");                                  // recover the caller-owned entry output
    emitter.instruction("add x4, x4, #8");                                      // publish directly into its native owner slot
    if graph { emitter.instruction("add x5, x4, #32"); }                        // publish a separate nested identity owner beside the copied value
    emitter.bl_c(if graph { "__elephc_eval_array_graph_reference_value" } else { "__elephc_eval_array_reference_value" });
    emitter.instruction(&format!("cbnz x0, {name}_eval_failed"));               // inspect failures only after any copied owner was published
    emitter.instruction("ldr x9, [sp, #344]");                                  // recover the copied-entry output
    emitter.instruction("ldr x9, [x9, #8]");                                    // distinguish a resolved reference from an ordinary physical entry
    emitter.instruction(&format!("cbnz x9, {name}_body_done"));                 // the resolved reference already has an independent native copy
    emitter.instruction(&format!("b {name}_eval_no_alias"));                    // retain ordinary raw-reader copying when no metadata matches
    emitter.label(&format!("{name}_eval_failed"));
    emitter.instruction("cmp x0, #2");                                          // pending PHP exceptions already own the native exception slot
    emitter.instruction(&format!("b.eq {name}_eval_pending"));                  // return pending status through this callback's protected boundary
    emitter.instruction("ldr x9, [sp, #344]");                                  // retain any failure-published owner for outer cleanup
    emitter.instruction("mov x10, #2");                                         // invalid entry metadata makes the coordinator fail closed
    emitter.instruction("str x10, [x9]");                                       // publish failure without inventing a successful PHP value
    emitter.instruction(&format!("b {name}_body_done"));                        // restore the callback frame normally after a fatal failure
    emitter.label(&format!("{name}_eval_pending"));
    emitter.instruction("bl __rt_throw_current");                               // unwind only native frames after the Rust adapter has returned
    emitter.label(&format!("{name}_eval_no_alias"));
}

/// Uses five SysV C arguments for list reads and a sixth identity-owner output for graph reads.
pub(super) fn x86_64(emitter: &mut Emitter, graph: bool) {
    let name = if graph { "__rt_mbstring_graph_value" } else { "__rt_mbstring_array_value" };
    emitter.instruction("mov rdi, QWORD PTR [rsp + 320]");                      // recover the optional active eval context
    emitter.instruction("test rdi, rdi");                                       // context-free native calls have no eval reference side table
    emitter.instruction(&format!("jz {name}_eval_no_alias"));                   // read the retained native array when no context exists
    for (offset, symbol) in [(72, "__rt_mbstring_clone"), (80, "__rt_mbstring_release"), (88, "__rt_mbstring_publish_throw")] {
        abi::emit_symbol_address(emitter, "r10", symbol);
        emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], r10"));   // supply a non-unwinding native ownership action
    }
    emitter.instruction("mov rsi, QWORD PTR [rsp + 328]");                      // recover the copied/original array source descriptor
    emitter.instruction("mov rsi, QWORD PTR [rsi + 8]");                        // pass the original boxed-handle identity without dereferencing it
    emitter.instruction("lea rdx, [rsp + 24]");                                 // borrow the exact current array key
    emitter.instruction("lea rcx, [rsp + 72]");                                 // pass the immutable native action table
    emitter.instruction("mov r8, QWORD PTR [rsp + 344]");                       // recover the caller-owned entry output
    emitter.instruction("add r8, 8");                                           // publish ownership directly into the coordinator's cleanup slot
    if graph { emitter.instruction("lea r9, [r8 + 32]"); }                      // preserve the original nested array box through the sixth C argument
    emitter.bl_c(if graph { "__elephc_eval_array_graph_reference_value" } else { "__elephc_eval_array_reference_value" });
    emitter.instruction("test eax, eax");                                       // failures may already have published a copied owner
    emitter.instruction(&format!("jnz {name}_eval_failed"));                    // preserve pending or fatal failure semantics
    emitter.instruction("mov r10, QWORD PTR [rsp + 344]");                      // recover the caller's entry output
    emitter.instruction("cmp QWORD PTR [r10 + 8], 0");                          // a nonnull owner means eval resolved the actual reference
    emitter.instruction(&format!("jne {name}_body_done"));                      // return the completed independent entry copy
    emitter.instruction(&format!("jmp {name}_eval_no_alias"));                  // absent metadata keeps the normal physical entry path
    emitter.label(&format!("{name}_eval_failed"));
    emitter.instruction("cmp eax, 2");                                          // pending exceptions have already been published by native actions
    emitter.instruction(&format!("je {name}_eval_pending"));                    // leave Rust before entering the native unwinder
    emitter.instruction("mov r10, QWORD PTR [rsp + 344]");                      // preserve the copied owner even on failure
    emitter.instruction("mov QWORD PTR [r10], 2");                              // report invalid metadata to make the coordinator fail closed
    emitter.instruction(&format!("jmp {name}_body_done"));                      // return fatal failure with balanced callback storage
    emitter.label(&format!("{name}_eval_pending"));
    emitter.instruction("call __rt_throw_current");                             // unwind to the surrounding native protected callback handler
    emitter.label(&format!("{name}_eval_no_alias"));
}
