//! Purpose:
//! Copies ordered array elements for deferred encoding lists and recursive graph snapshots.
//!
//! Called from:
//! - The version-two and version-three native/eval mbstring callback tables.
//!
//! Key details:
//! - The shared reader preserves concrete native tags; copying retains arrays and object identity.
//! - The protected callback publishes ownership before returning to Rust and performs no string cast.
//! - Cursor advancement and end/error kinds use the same insertion-order reader as snapshots.

use super::*;

mod eval_references;
mod publish_throw;

/// Emits the protected four-argument callback that returns an owned boxed entry.
pub(super) fn emit(emitter: &mut Emitter, eval_bridge: bool) {
    crate::codegen_support::runtime::exceptions::emit_protected(
        emitter, "__rt_mbstring_array_value", if eval_bridge { body_with_eval } else { body },
    );
    crate::codegen_support::runtime::exceptions::emit_protected(
        emitter, "__rt_mbstring_graph_value", if eval_bridge { graph_with_eval } else { graph },
    );
    if eval_bridge { publish_throw::emit(emitter); }
}

/// Stages borrowed descriptors below the protected frame before publishing a copied entry owner.
fn body(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter, false, false); }
    else { x86_64(emitter, false, false); }
}

/// Includes original-handle reference lookup when the runtime links the eval bridge.
fn body_with_eval(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter, true, false); }
    else { x86_64(emitter, true, false); }
}

/// Reads exact keys and pins nested source identities for ordinary native graph snapshots.
fn graph(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter, false, true); }
    else { x86_64(emitter, false, true); }
}

/// Adds eval reference resolution to the same protected graph reader.
fn graph_with_eval(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter, true, true); }
    else { x86_64(emitter, true, true); }
}

/// Copies one AArch64 entry while leaving string conversion and diagnostics to the coordinator.
fn aarch64(emitter: &mut Emitter, eval_bridge: bool, graph: bool) {
    let name = if graph { "__rt_mbstring_graph_value" } else { "__rt_mbstring_array_value" };
    emitter.instruction("sub sp, sp, #96");                                     // reserve root, key, and value descriptors below the protected callback frame
    emitter.instruction("ldr x9, [sp, #344]");                                  // recover the caller-owned entry result pointer
    if graph { emitter.instruction("str xzr, [x9, #40]"); }                     // initialize the nested original owner before any callback can fail
    emitter.instruction("stp xzr, xzr, [x9]");                                  // initialize the iteration kind and owner before any allocation
    emitter.instruction("ldr x0, [sp, #328]");                                  // recover the borrowed boxed array argument
    emitter.instruction("ldr x0, [x0]");                                        // borrow the retained copy from the array source descriptor
    emitter.instruction("bl __rt_mixed_unbox");                                 // obtain the concrete root array representation
    emitter.instruction("stp x0, x1, [sp]");                                    // stage the root tag and opaque native array identity
    emitter.instruction("str x2, [sp, #16]");                                   // retain its complete root descriptor
    emitter.instruction("mov x0, #0");                                          // the native reader needs no host context
    emitter.instruction("mov x1, sp");                                          // pass the root descriptor through the C argument convention
    emitter.instruction("ldr x2, [sp, #336]");                                  // resume the caller-owned insertion-order cursor
    emitter.instruction("add x3, sp, #24");                                     // reserve borrowed key metadata even though list parsing ignores keys
    emitter.instruction("add x4, sp, #48");                                     // receive every concrete value word before copying it
    emitter.instruction("bl __rt_mbstring_array_next_raw");                     // borrow one entry without discarding object or resource identity
    emitter.instruction("ldr x9, [sp, #344]");                                  // recover the caller-owned entry result
    emitter.instruction("str x0, [x9]");                                        // publish the exact end, entry, or invalid-metadata kind
    emitter.instruction("cmp x0, #1");                                          // only an entry has a value to copy
    emitter.instruction(&format!("b.ne {name}_body_done"));                     // return end or invalid metadata without fabricated ownership
    if graph {
        emitter.instruction("ldp x10, x11, [sp, #24]");                         // recover the exact key tag and low payload
        emitter.instruction("stp x10, x11, [x9, #16]");                         // publish borrowed key metadata beside the value owner
        emitter.instruction("ldr x10, [sp, #40]");                              // preserve the binary key length
        emitter.instruction("str x10, [x9, #32]");                              // complete the exact key descriptor
    }
    if eval_bridge { eval_references::aarch64(emitter, graph); }
    emitter.instruction("ldp x0, x1, [sp, #48]");                               // recover the original tag and low payload for value copying
    emitter.instruction("ldr x2, [sp, #64]");                                   // preserve string lengths and high payload words
    emitter.instruction("cmp x0, #7");                                          // recognize an existing Mixed cell whose resource identity must be retained
    emitter.instruction(&format!("b.ne {name}_copy_concrete"));                 // ordinary typed slots use concrete value boxing
    if graph {
        emitter.instruction("mov x0, #0");                                      // native identity pinning does not require an eval context
        emitter.instruction("ldr x2, [sp, #344]");                              // recover the caller-owned graph entry
        emitter.instruction("add x2, x2, #40");                                 // publish the original boxed identity into its cleanup slot
        emitter.instruction("bl __rt_mbstring_pin");                            // keep nested reference metadata identities alive through traversal
        emitter.instruction("ldr x1, [sp, #56]");                               // restore the borrowed original Mixed cell for value copying
    }
    emitter.instruction("mov x0, #0");                                          // the value-copy callback needs no eval context
    emitter.instruction("ldr x2, [sp, #344]");                                  // recover the caller-owned entry result
    emitter.instruction("add x2, x2, #8");                                      // publish a copied owner directly into the explicit cleanup arena
    emitter.instruction("bl __rt_mbstring_clone");                              // dereference markers and retain shared resource cells correctly
    emitter.instruction(&format!("b {name}_body_done"));                        // the shared clone callback already transferred the result owner
    emitter.label(&format!("{name}_copy_concrete"));
    emitter.instruction("bl __rt_mixed_from_value");                            // copy the element by value with native retain and string persistence rules
    emitter.instruction("ldr x9, [sp, #344]");                                  // recover the owner destination after allocation
    emitter.instruction("str x0, [x9, #8]");                                    // transfer the copied entry owner to the explicit Rust cleanup arena
    emitter.label(&format!("{name}_body_done"));
    emitter.instruction("add sp, sp, #96");                                     // restore the protected frame before its shared handler teardown
}

/// Applies the identical borrowed-read and copied-owner protocol under the SysV ABI.
fn x86_64(emitter: &mut Emitter, eval_bridge: bool, graph: bool) {
    let name = if graph { "__rt_mbstring_graph_value" } else { "__rt_mbstring_array_value" };
    emitter.instruction("sub rsp, 96");                                         // reserve root, key, and value descriptors with aligned native calls
    emitter.instruction("mov r10, QWORD PTR [rsp + 344]");                      // recover the caller entry result pointer
    if graph { emitter.instruction("mov QWORD PTR [r10 + 40], 0"); }            // initialize the nested identity owner before reading an entry
    emitter.instruction("mov QWORD PTR [r10], 0");                              // initialize the result kind before reading the array
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // initialize copied ownership before allocation can fail
    emitter.instruction("mov rax, QWORD PTR [rsp + 328]");                      // recover the borrowed boxed array
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // borrow the retained array copy without dereferencing its original identity
    emitter.instruction("call __rt_mixed_unbox");                               // obtain the original root tag and native array identity
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // stage the root tag for the C reader
    emitter.instruction("mov QWORD PTR [rsp + 8], rdi");                        // stage the opaque root array identity
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // preserve the complete root descriptor
    emitter.instruction("xor edi, edi");                                        // the native reader does not require host context
    emitter.instruction("mov rsi, rsp");                                        // pass the root descriptor through the second C argument
    emitter.instruction("mov rdx, QWORD PTR [rsp + 336]");                      // resume the caller insertion-order cursor
    emitter.instruction("lea rcx, [rsp + 24]");                                 // provide unused borrowed key metadata storage
    emitter.instruction("lea r8, [rsp + 48]");                                  // receive the complete concrete entry descriptor
    emitter.instruction("call __rt_mbstring_array_next_raw");                   // borrow an entry without replacing unsupported graph values
    emitter.instruction("mov r10, QWORD PTR [rsp + 344]");                      // recover caller result storage after iteration
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish end, entry, or invalid-metadata status
    emitter.instruction("cmp rax, 1");                                          // only a complete entry contains a value to copy
    emitter.instruction(&format!("jne {name}_body_done"));                      // leave ownership empty for end or invalid metadata
    if graph {
        for offset in [0, 8, 16] {
            emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", 24 + offset)); // recover one exact key descriptor word
            emitter.instruction(&format!("mov QWORD PTR [r10 + {}], r11", 16 + offset)); // publish borrowed key metadata before reference callbacks
        }
    }
    if eval_bridge { eval_references::x86_64(emitter, graph); }
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // recover the original native value tag
    emitter.instruction("mov rdi, QWORD PTR [rsp + 56]");                       // recover its low payload before boxing
    emitter.instruction("mov rsi, QWORD PTR [rsp + 64]");                       // preserve the high payload or binary string length
    emitter.instruction("cmp rax, 7");                                          // recognize an existing Mixed cell before detaching resource ownership
    emitter.instruction(&format!("jne {name}_copy_concrete"));                  // typed non-Mixed slots use concrete value boxing
    if graph {
        emitter.instruction("mov rsi, rdi");                                    // pass the original Mixed cell to the identity pin callback
        emitter.instruction("xor edi, edi");                                    // native identity pinning needs no eval context
        emitter.instruction("mov rdx, QWORD PTR [rsp + 344]");                  // recover caller graph entry storage
        emitter.instruction("add rdx, 40");                                     // publish its separate original identity owner
        emitter.instruction("call __rt_mbstring_pin");                          // retain nested reference lookup identity until traversal finishes
        emitter.instruction("mov rdi, QWORD PTR [rsp + 56]");                   // restore the borrowed original cell before copying
    }
    emitter.instruction("mov rsi, rdi");                                        // pass the borrowed original cell through the C copy callback
    emitter.instruction("xor edi, edi");                                        // the native copy callback needs no eval context
    emitter.instruction("mov rdx, QWORD PTR [rsp + 344]");                      // recover the caller entry result
    emitter.instruction("add rdx, 8");                                          // publish directly into its owned-value slot
    emitter.instruction("call __rt_mbstring_clone");                            // preserve resource identity and dereference argument markers before copying
    emitter.instruction(&format!("jmp {name}_body_done"));                      // owner publication is complete after the shared copy callback
    emitter.label(&format!("{name}_copy_concrete"));
    emitter.instruction("call __rt_mixed_from_value");                          // create an independent entry owner before PHP string conversion
    emitter.instruction("mov r10, QWORD PTR [rsp + 344]");                      // recover the owner output after allocation
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // transfer the copied entry to the coordinator cleanup arena
    emitter.label(&format!("{name}_body_done"));
    emitter.instruction("add rsp, 96");                                         // restore the protected frame for common handler cleanup
}
