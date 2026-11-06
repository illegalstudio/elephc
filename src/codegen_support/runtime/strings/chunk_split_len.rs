//! Purpose:
//! Emits `__rt_chunk_split_len`, the allocation-free length computation for
//! `strlen(chunk_split(...))`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - php-src appends the separator after every chunk, including the trailing partial
//!   chunk, and emits one separator for an empty subject. The result size is therefore
//!   `subject_len + max(1, ceil(subject_len / chunk_len)) * separator_len`.
//! - `$length < 1` is already rejected by the EIR lowering before this helper runs.
//! - Multiplication and addition overflow, and sizes above the configured heap bound,
//!   branch to PHP's allocation-overflow fatal instead of returning a wrapped length.
//! - ABI: AArch64 x1/x2 = subject, x3 = chunk length, x4/x5 = separator; x86_64
//!   rax/rdx = subject, rdi = chunk length, rcx/r8 = separator. Result in x0/rax.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_chunk_split_len` helper for the active target.
pub fn emit_chunk_split_len(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_chunk_split_len_x86_64(emitter);
        return;
    }
    emit_chunk_split_len_aarch64(emitter);
}

/// Emits the AArch64 length helper.
fn emit_chunk_split_len_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: chunk_split_len ---");
    e.label_global("__rt_chunk_split_len");
    e.instruction("udiv x9, x2, x3");                                            // whole chunks = subject length / chunk length
    e.instruction("msub x10, x9, x3, x2");                                       // trailing remainder
    e.instruction("cmp x10, #0");                                                // is there a partial trailing chunk?
    e.instruction("cinc x9, x9, ne");                                            // a partial chunk contributes another separator
    e.instruction("cmp x9, #0");                                                 // did an empty subject leave no chunks?
    e.instruction("csinc x9, x9, xzr, ne");                                      // php-src emits one separator for an empty subject
    e.instruction("umulh x11, x9, x5");                                          // capture the high half of parts * separator length
    e.instruction("cbnz x11, __rt_chunk_split_len_overflow");                    // reject a wrapped separator total
    e.instruction("mul x12, x9, x5");                                            // total separator bytes
    e.instruction("adds x0, x2, x12");                                           // result length = subject + separators
    e.instruction("b.cs __rt_chunk_split_len_overflow");                         // reject a wrapped total
    abi::emit_symbol_address(e, "x9", "_heap_max");
    e.instruction("ldr x9, [x9]");                                               // load the configured heap bound
    e.instruction("cmp x0, x9");                                                 // would chunk_split() exceed the heap?
    e.instruction("b.hi __rt_chunk_split_len_overflow");                         // report the same allocation fatal
    e.instruction("ret");                                                        // return the exact result length
    e.label("__rt_chunk_split_len_overflow");
    e.instruction("b __rt_alloc_overflow");                                      // keep the shared fatal trampoline cross-atom safe
}

/// Emits the x86_64 SysV length helper.
fn emit_chunk_split_len_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: chunk_split_len ---");
    e.label_global("__rt_chunk_split_len");
    e.instruction("test rdi, rdi");                                              // a zero chunk length cannot divide
    e.instruction("jz __rt_chunk_split_len_overflow_x86_64");                    // defensive fallback for an impossible call
    e.instruction("mov r10, rdx");                                               // preserve the subject length across the division
    e.instruction("mov rax, r10");                                               // dividend = subject length
    e.instruction("xor edx, edx");                                               // clear the dividend high half
    e.instruction("div rdi");                                                    // whole chunks in rax, trailing remainder in rdx
    e.instruction("test rdx, rdx");                                              // is there a partial trailing chunk?
    e.instruction("jz __rt_chunk_split_len_no_rest_x86_64");                     // an even split needs no extra separator
    e.instruction("add rax, 1");                                                 // a partial chunk contributes another separator
    e.label("__rt_chunk_split_len_no_rest_x86_64");
    e.instruction("test rax, rax");                                              // did an empty subject leave no chunks?
    e.instruction("jnz __rt_chunk_split_len_have_parts_x86_64");                 // a non-empty subject already has its separator count
    e.instruction("mov rax, 1");                                                 // php-src emits one separator for an empty subject
    e.label("__rt_chunk_split_len_have_parts_x86_64");
    e.instruction("mul r8");                                                     // total separator bytes = parts * separator length
    e.instruction("jo __rt_chunk_split_len_overflow_x86_64");                    // reject a wrapped separator total
    e.instruction("add rax, r10");                                               // result length = subject + separators
    e.instruction("jc __rt_chunk_split_len_overflow_x86_64");                    // reject a wrapped total
    abi::emit_symbol_address(e, "r9", "_heap_max");
    e.instruction("mov r9, QWORD PTR [r9]");                                     // load the configured heap bound
    e.instruction("cmp rax, r9");                                                // would chunk_split() exceed the heap?
    e.instruction("ja __rt_chunk_split_len_overflow_x86_64");                    // report the same allocation fatal
    e.instruction("ret");                                                        // return the exact result length
    e.label("__rt_chunk_split_len_overflow_x86_64");
    e.instruction("jmp __rt_alloc_overflow");                                    // keep the shared fatal trampoline reachable
}
