//! Purpose:
//! Shares x86 collector edge enumeration between cache priming and exact fallback.
//!
//! Called from:
//! - `super::emit_gc_collect_cycles_linux_x86_64()` for each owned child layout.
//!
//! Key details:
//! - A zero candidate pointer means count every edge in its child's transient header.
//! - A real candidate retains the existing exact comparison and 64-bit accumulator.
//! - The walker keeps every register and stack alignment across a cached edge update.

use crate::codegen_support::emit::Emitter;

/// Records one cached child edge or one exact edge into the currently selected candidate.
pub(super) fn emit_edge(emitter: &mut Emitter, child: &str, next: &str, name: &str) {
    let exact = format!("__rt_gc_cache_{name}_exact");

    // -- a zero candidate primes all incoming counts in one outgoing-edge walk --
    emitter.instruction("test rsi, rsi");                                       // distinguish cache priming from the exact fallback
    emitter.instruction(&format!("jne {exact}"));                               // keep real candidate comparisons exact
    emitter.instruction("sub rsp, 16");                                         // preserve the current accumulator with aligned calls
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save a live property descriptor or container offset
    emitter.instruction(&format!("mov rax, {child}"));                          // pass the borrowed child through the runtime pointer ABI
    emitter.instruction("call __rt_gc_note_child_ref");                         // cache this edge without changing the child's owner count
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // restore the source walker's live accumulator
    emitter.instruction("add rsp, 16");                                         // release aligned edge-call scratch storage
    emitter.instruction(&format!("jmp {next}"));                                // continue enumerating the same source graph

    // -- saturated candidates use the previous full-precision recount unchanged --
    emitter.label(&exact);
    emitter.instruction(&format!("cmp {child}, rsi"));                          // compare the actual child with the fallback candidate
    emitter.instruction(&format!("jne {next}"));                                // another child contributes no edge to this candidate
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // retain the original 64-bit incoming-edge total
}
