//! Purpose:
//! Emits `__rt_count_chars_distinct`, the allocation-free count of distinct byte
//! values for `count(count_chars($s, 1))`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - AArch64 keeps a 256-bit presence bitmap and reduces it with `cnt`; x86_64 uses a
//!   256-byte presence table. No hash or 2 KiB tally is materialized.
//! - ABI: AArch64 x1/x2 = subject; x86_64 rax/rdx = subject. Result in x0/rax.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the distinct-byte-count helper for the active target.
pub fn emit_count_chars_distinct(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_count_chars_distinct_x86_64(emitter);
        return;
    }
    emit_count_chars_distinct_aarch64(emitter);
}

/// Emits the AArch64 presence-bitmap helper.
fn emit_count_chars_distinct_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: count_chars_distinct ---");
    e.label_global("__rt_count_chars_distinct");
    e.instruction("sub sp, sp, #32");                                           // reserve the 256-bit presence bitmap
    e.instruction("stp xzr, xzr, [sp]");                                        // clear the first half of the bitmap
    e.instruction("stp xzr, xzr, [sp, #16]");                                   // clear the second half of the bitmap
    e.label("__rt_count_chars_distinct_scan");
    e.instruction("cbz x2, __rt_count_chars_distinct_done");                    // stop at the end of the subject
    e.instruction("ldrb w4, [x1], #1");                                         // load the next subject byte
    e.instruction("sub x2, x2, #1");                                            // consume one subject byte
    e.instruction("lsr w5, w4, #3");                                            // select the bitmap byte
    e.instruction("and w6, w4, #7");                                            // select the bit inside that byte
    e.instruction("add x9, sp, x5");                                            // address the selected bitmap byte
    e.instruction("ldrb w7, [x9]");                                             // load the presence bits
    e.instruction("mov w8, #1");                                                // build the one-bit mask
    e.instruction("lsl w8, w8, w6");                                            // move the mask to the selected bit
    e.instruction("orr w7, w7, w8");                                            // mark the byte as present
    e.instruction("strb w7, [x9]");                                             // publish the updated presence bits
    e.instruction("b __rt_count_chars_distinct_scan");                          // scan the next subject byte
    e.label("__rt_count_chars_distinct_done");
    e.instruction("ldr q0, [sp]");                                              // load the first 16 bitmap bytes
    e.instruction("ldr q1, [sp, #16]");                                         // load the second 16 bitmap bytes
    e.instruction("cnt v0.16b, v0.16b");                                        // count set bits in each first-half byte
    e.instruction("cnt v1.16b, v1.16b");                                        // count set bits in each second-half byte
    e.instruction("add v0.16b, v0.16b, v1.16b");                                // combine both bitmap halves
    e.instruction("uaddlv h0, v0.16b");                                         // sum the per-byte counts
    e.instruction("umov w0, v0.h[0]");                                          // move the total into the result register
    e.instruction("add sp, sp, #32");                                           // release the presence bitmap
    e.instruction("ret");                                                       // return the distinct byte count
}

/// Emits the x86_64 presence-table helper.
fn emit_count_chars_distinct_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: count_chars_distinct ---");
    e.label_global("__rt_count_chars_distinct");
    e.instruction("sub rsp, 256");                                              // reserve the 256-byte presence table
    e.instruction("mov r8, rax");                                               // r8 = subject pointer
    e.instruction("mov r9, rdx");                                               // r9 = subject length
    e.instruction("mov rdi, rsp");                                              // rdi = presence table base
    e.instruction("xor eax, eax");                                              // clear the table with `rep stosq`
    e.instruction("mov rcx, 32");                                               // 32 quadwords cover all 256 byte values
    e.instruction("rep stosq");                                                 // zero the presence table
    e.label("__rt_count_chars_distinct_scan_x86_64");
    e.instruction("test r9, r9");                                               // is the subject exhausted?
    e.instruction("jz __rt_count_chars_distinct_done_x86_64");                  // count the present bytes
    e.instruction("movzx eax, BYTE PTR [r8]");                                  // load the next subject byte
    e.instruction("mov BYTE PTR [rsp + rax], 1");                               // mark it present
    e.instruction("inc r8");                                                    // advance the subject cursor
    e.instruction("dec r9");                                                    // consume one subject byte
    e.instruction("jmp __rt_count_chars_distinct_scan_x86_64");                 // scan the next subject byte
    e.label("__rt_count_chars_distinct_done_x86_64");
    e.instruction("xor eax, eax");                                              // distinct count starts at zero
    e.instruction("xor ecx, ecx");                                              // byte-value index starts at zero
    e.label("__rt_count_chars_distinct_count_x86_64");
    e.instruction("cmp BYTE PTR [rsp + rcx], 0");                               // is this byte value present?
    e.instruction("je __rt_count_chars_distinct_next_x86_64");                  // absent values do not count
    e.instruction("add rax, 1");                                                // count one distinct byte value
    e.label("__rt_count_chars_distinct_next_x86_64");
    e.instruction("inc rcx");                                                   // advance the byte-value index
    e.instruction("cmp rcx, 256");                                              // have all byte values been checked?
    e.instruction("jb __rt_count_chars_distinct_count_x86_64");                 // keep counting distinct values
    e.instruction("add rsp, 256");                                              // release the presence table
    e.instruction("ret");                                                       // return the distinct byte count
}
