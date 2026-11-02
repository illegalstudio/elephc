//! Purpose:
//! Emits the byte-oriented `strcspn()` and `strspn()` runtime scanners.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the string runtime facade.
//!
//! Key details:
//! - Callers normalize PHP offset and nullable-length semantics before entering these helpers.
//! - The 256-byte membership table matches php-src for arbitrary bytes, including embedded NULs.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the initial segment scanner for bytes absent from the character mask.
pub fn emit_strcspn(emitter: &mut Emitter) {
    emit_string_span(emitter, "strcspn", false);
}

/// Emits the initial segment scanner for bytes present in the character mask.
pub fn emit_strspn(emitter: &mut Emitter) {
    emit_string_span(emitter, "strspn", true);
}

/// Emits one target-specific byte membership scanner.
fn emit_string_span(emitter: &mut Emitter, name: &str, must_match: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_string_span_x86_64(emitter, name, must_match);
        return;
    }

    let symbol = format!("__rt_{name}");
    let zero = format!("{symbol}_zero_table");
    let fill = format!("{symbol}_fill_table");
    let scan = format!("{symbol}_scan");
    let advance = format!("{symbol}_advance");
    let done = format!("{symbol}_done");

    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(&symbol);
    emitter.instruction("sub sp, sp, #256");                                    // reserve one membership byte for every possible input byte
    emitter.instruction("mov x5, #0");                                          // start clearing the 32 table words at index zero
    emitter.label(&zero);
    emitter.instruction("str xzr, [sp, x5, lsl #3]");                           // clear eight membership bytes at once
    emitter.instruction("add x5, x5, #1");                                      // advance to the next table word
    emitter.instruction("cmp x5, #32");                                         // have all 256 membership bytes been cleared?
    emitter.instruction(&format!("b.lt {zero}"));                               // continue until the whole table is zeroed
    emitter.instruction("mov x5, #0");                                          // start consuming the character mask
    emitter.label(&fill);
    emitter.instruction("cmp x5, x4");                                          // have all mask bytes been recorded?
    emitter.instruction(&format!("b.ge {scan}"));                               // begin scanning the subject once the table is complete
    emitter.instruction("ldrb w6, [x3, x5]");                                   // load one unsigned mask byte
    emitter.instruction("mov w7, #1");                                          // membership entries use one as their true value
    emitter.instruction("strb w7, [sp, x6]");                                   // mark this byte as present in the mask
    emitter.instruction("add x5, x5, #1");                                      // advance to the next mask byte
    emitter.instruction(&format!("b {fill}"));                                  // continue building the membership table
    emitter.label(&scan);
    emitter.instruction("mov x5, #0");                                          // the result is the number of accepted leading bytes
    let loop_label = format!("{symbol}_scan_loop");
    emitter.label(&loop_label);
    emitter.instruction("cmp x5, x2");                                          // did the scan reach the normalized window end?
    emitter.instruction(&format!("b.ge {done}"));                               // the whole window satisfied the requested predicate
    emitter.instruction("ldrb w6, [x1, x5]");                                   // load the current subject byte
    emitter.instruction("ldrb w7, [sp, x6]");                                   // read whether that byte belongs to the mask
    if must_match {
        emitter.instruction(&format!("cbnz w7, {advance}"));                    // strspn continues while each byte belongs to the mask
    } else {
        emitter.instruction(&format!("cbz w7, {advance}"));                     // strcspn continues while each byte is absent from the mask
    }
    emitter.instruction(&format!("b {done}"));                                  // the first predicate mismatch terminates the span
    emitter.label(&advance);
    emitter.instruction("add x5, x5, #1");                                      // count one accepted leading byte
    emitter.instruction(&format!("b {loop_label}"));                            // inspect the next subject byte
    emitter.label(&done);
    emitter.instruction("mov x0, x5");                                          // return the accepted byte count as a PHP integer
    emitter.instruction("add sp, sp, #256");                                    // release the membership table
    emitter.instruction("ret");                                                 // return to generated user code
}

/// Emits the System V x86_64 byte membership scanner.
fn emit_string_span_x86_64(emitter: &mut Emitter, name: &str, must_match: bool) {
    let symbol = format!("__rt_{name}");
    let zero = format!("{symbol}_zero_table_x86_64");
    let fill = format!("{symbol}_fill_table_x86_64");
    let scan = format!("{symbol}_scan_x86_64");
    let loop_label = format!("{symbol}_scan_loop_x86_64");
    let advance = format!("{symbol}_advance_x86_64");
    let done = format!("{symbol}_done_x86_64");

    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(&symbol);
    emitter.instruction("sub rsp, 256");                                        // reserve one membership byte for every possible input byte
    emitter.instruction("xor r8d, r8d");                                        // start clearing the 32 table words at index zero
    emitter.label(&zero);
    emitter.instruction("mov QWORD PTR [rsp + r8*8], 0");                       // clear eight membership bytes at once
    emitter.instruction("add r8, 1");                                           // advance to the next table word
    emitter.instruction("cmp r8, 32");                                          // have all 256 membership bytes been cleared?
    emitter.instruction(&format!("jl {zero}"));                                 // continue until the whole table is zeroed
    emitter.instruction("xor r8d, r8d");                                        // start consuming the character mask
    emitter.label(&fill);
    emitter.instruction("cmp r8, rcx");                                         // have all mask bytes been recorded?
    emitter.instruction(&format!("jge {scan}"));                                // begin scanning the subject once the table is complete
    emitter.instruction("movzx r10d, BYTE PTR [rdx + r8]");                     // load one unsigned mask byte
    emitter.instruction("mov BYTE PTR [rsp + r10], 1");                         // mark this byte as present in the mask
    emitter.instruction("add r8, 1");                                           // advance to the next mask byte
    emitter.instruction(&format!("jmp {fill}"));                                // continue building the membership table
    emitter.label(&scan);
    emitter.instruction("xor r8d, r8d");                                        // the result is the number of accepted leading bytes
    emitter.label(&loop_label);
    emitter.instruction("cmp r8, rsi");                                         // did the scan reach the normalized window end?
    emitter.instruction(&format!("jge {done}"));                                // the whole window satisfied the requested predicate
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r8]");                     // load the current subject byte
    emitter.instruction("movzx r11d, BYTE PTR [rsp + r10]");                    // read whether that byte belongs to the mask
    emitter.instruction("test r11d, r11d");                                     // turn membership into branch flags
    if must_match {
        emitter.instruction(&format!("jnz {advance}"));                         // strspn continues while each byte belongs to the mask
    } else {
        emitter.instruction(&format!("jz {advance}"));                          // strcspn continues while each byte is absent from the mask
    }
    emitter.instruction(&format!("jmp {done}"));                                // the first predicate mismatch terminates the span
    emitter.label(&advance);
    emitter.instruction("add r8, 1");                                           // count one accepted leading byte
    emitter.instruction(&format!("jmp {loop_label}"));                          // inspect the next subject byte
    emitter.label(&done);
    emitter.instruction("mov rax, r8");                                         // return the accepted byte count as a PHP integer
    emitter.instruction("add rsp, 256");                                        // release the membership table
    emitter.instruction("ret");                                                 // return to generated user code
}
