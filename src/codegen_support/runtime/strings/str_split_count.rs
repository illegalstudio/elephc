//! Purpose:
//! Emits `__rt_str_split_count`, the allocation-free count of `str_split()` chunks.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The chunk count is `ceil(source_len / chunk_len)`; an empty source has zero chunks.
//! - The `$length >= 1` guard lives in the EIR lowering, so this helper can divide
//!   directly without a zero check.
//! - ABI matches `__rt_str_split`: AArch64 x1/x2 = source, x3 = chunk length; x86_64
//!   rax/rdx = source, rdi = chunk length. Result in x0/rax.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_str_split_count` helper for the active target.
pub fn emit_str_split_count(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_split_count_x86_64(emitter);
        return;
    }
    emit_str_split_count_aarch64(emitter);
}

/// Emits the AArch64 chunk-count helper.
fn emit_str_split_count_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: str_split_count ---");
    e.label_global("__rt_str_split_count");
    e.instruction("udiv x9, x2, x3");                                             // whole chunks = source length / chunk length
    e.instruction("msub x10, x9, x3, x2");                                        // trailing remainder
    e.instruction("cmp x10, #0");                                                 // is there a partial trailing chunk?
    e.instruction("cinc x0, x9, ne");                                             // a partial chunk contributes one more element
    e.instruction("ret");                                                        // return the chunk count
}

/// Emits the x86_64 SysV chunk-count helper.
fn emit_str_split_count_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: str_split_count ---");
    e.label_global("__rt_str_split_count");
    e.instruction("mov rax, rdx");                                                // dividend = source length
    e.instruction("xor edx, edx");                                                // clear the dividend high half
    e.instruction("div rdi");                                                     // whole chunks in rax, remainder in rdx
    e.instruction("test rdx, rdx");                                               // is there a partial trailing chunk?
    e.instruction("jz __rt_str_split_count_done_x86_64");                        // an even split needs no extra element
    e.instruction("add rax, 1");                                                  // a partial chunk contributes one more element
    e.label("__rt_str_split_count_done_x86_64");
    e.instruction("ret");                                                        // return the chunk count
}
