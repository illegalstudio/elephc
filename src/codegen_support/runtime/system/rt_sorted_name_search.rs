//! Purpose:
//! Emits `__rt_sorted_name_search`, the shared binary-search helper over a
//! name-sorted runtime table used by the constant/enum registry lookups.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (gated by the
//!   `const_introspection` feature) and indirectly by `__rt_defined`,
//!   `__rt_enum_exists`, and `__rt_constant`.
//!
//! Key details:
//! - Input:  x0/rdi=name_ptr, x1/rsi=name_len, x2/rdx=table_base,
//!   x3/rcx=entry_count, x4/r8=entry_size (bytes).
//! - Output: x0/rax=pointer to the matching entry, or 0 when absent.
//! - Each entry begins with `{name_ptr, name_len}`; the search strips a single
//!   leading `\` from the query and orders entries with `__rt_strcmp`, matching
//!   the byte-lexicographic sort applied when the table is emitted.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_sorted_name_search` binary-search helper.
pub fn emit_rt_sorted_name_search(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation of `__rt_sorted_name_search`.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sorted_name_search ---");
    emitter.label_global("__rt_sorted_name_search");

    // -- strip a single leading backslash from the query name --
    emitter.instruction("cbz x1, __rt_sns_after_strip");                        // empty name has nothing to strip
    emitter.instruction("ldrb w11, [x0]");                                      // load first query byte
    emitter.instruction("cmp w11, #0x5C");                                      // is it a leading backslash?
    emitter.instruction("b.ne __rt_sns_after_strip");                           // keep the name unchanged otherwise
    emitter.instruction("add x0, x0, #1");                                      // skip the leading backslash byte
    emitter.instruction("sub x1, x1, #1");                                      // shorten the query length accordingly
    emitter.label("__rt_sns_after_strip");

    // -- set up the search frame and spill the immutable inputs --
    emitter.instruction("sub sp, sp, #80");                                     // reserve locals plus saved registers
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // spill normalized query name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // spill normalized query name length
    emitter.instruction("str x2, [sp, #16]");                                   // spill table base pointer
    emitter.instruction("str x4, [sp, #24]");                                   // spill entry size in bytes
    emitter.instruction("str xzr, [sp, #32]");                                  // lo bound starts at 0
    emitter.instruction("str x3, [sp, #40]");                                   // hi bound starts at entry_count

    emitter.label("__rt_sns_loop");
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload lo bound
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload hi bound
    emitter.instruction("cmp x5, x6");                                          // has the window collapsed?
    emitter.instruction("b.hs __rt_sns_not_found");                             // lo >= hi means the name is absent
    emitter.instruction("add x7, x5, x6");                                      // sum the bounds
    emitter.instruction("lsr x7, x7, #1");                                      // mid = (lo + hi) / 2
    emitter.instruction("str x7, [sp, #48]");                                   // save mid for the comparison branch
    emitter.instruction("ldr x8, [sp, #16]");                                   // reload table base
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload entry size
    emitter.instruction("mul x10, x7, x9");                                     // byte offset = mid * entry_size
    emitter.instruction("add x10, x8, x10");                                    // entry pointer = base + offset
    emitter.instruction("ldr x3, [x10]");                                       // candidate name pointer (strcmp arg b)
    emitter.instruction("ldr x4, [x10, #8]");                                   // candidate name length (strcmp arg b)
    emitter.instruction("ldr x1, [sp, #0]");                                    // query name pointer (strcmp arg a)
    emitter.instruction("ldr x2, [sp, #8]");                                    // query name length (strcmp arg a)
    emitter.instruction("bl __rt_strcmp");                                      // compare query against the candidate
    emitter.instruction("cmp x0, #0");                                          // inspect the ordering result
    emitter.instruction("b.eq __rt_sns_found");                                 // zero means an exact match
    emitter.instruction("ldr x7, [sp, #48]");                                   // reload mid after the call
    emitter.instruction("b.lt __rt_sns_set_hi");                                // query < candidate searches the lower half
    emitter.instruction("add x7, x7, #1");                                      // query > candidate searches the upper half
    emitter.instruction("str x7, [sp, #32]");                                   // lo = mid + 1
    emitter.instruction("b __rt_sns_loop");                                     // continue the search

    emitter.label("__rt_sns_set_hi");
    emitter.instruction("str x7, [sp, #40]");                                   // hi = mid
    emitter.instruction("b __rt_sns_loop");                                     // continue the search

    emitter.label("__rt_sns_found");
    emitter.instruction("ldr x7, [sp, #48]");                                   // reload mid for the matched entry
    emitter.instruction("ldr x8, [sp, #16]");                                   // reload table base
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload entry size
    emitter.instruction("mul x10, x7, x9");                                     // recompute the matched byte offset
    emitter.instruction("add x0, x8, x10");                                     // return pointer = base + offset
    emitter.instruction("b __rt_sns_done");                                     // share the epilogue

    emitter.label("__rt_sns_not_found");
    emitter.instruction("mov x0, #0");                                          // report a miss with a null pointer

    emitter.label("__rt_sns_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the search frame
    emitter.instruction("ret");                                                 // return entry pointer in x0
}

/// Emits the x86_64 implementation of `__rt_sorted_name_search`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sorted_name_search ---");
    emitter.label_global("__rt_sorted_name_search");

    // -- strip a single leading backslash from the query name --
    emitter.instruction("test rsi, rsi");                                       // empty name has nothing to strip
    emitter.instruction("je __rt_sns_after_strip");                             // keep an empty name unchanged
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load first query byte
    emitter.instruction("cmp al, 0x5C");                                        // is it a leading backslash?
    emitter.instruction("jne __rt_sns_after_strip");                            // keep the name unchanged otherwise
    emitter.instruction("add rdi, 1");                                          // skip the leading backslash byte
    emitter.instruction("sub rsi, 1");                                          // shorten the query length accordingly
    emitter.label("__rt_sns_after_strip");

    // -- set up the search frame and spill the immutable inputs --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve locals for the search bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // spill normalized query name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // spill normalized query name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // spill table base pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // spill entry size in bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // lo bound starts at 0
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // hi bound starts at entry_count

    emitter.label("__rt_sns_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload lo bound
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload hi bound
    emitter.instruction("cmp rax, r9");                                         // has the window collapsed?
    emitter.instruction("jae __rt_sns_not_found");                              // lo >= hi means the name is absent
    emitter.instruction("add rax, r9");                                         // sum the bounds
    emitter.instruction("shr rax, 1");                                          // mid = (lo + hi) / 2
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save mid for the comparison branch
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload table base
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload entry size
    emitter.instruction("imul rax, r11");                                       // byte offset = mid * entry_size
    emitter.instruction("add rax, r10");                                        // entry pointer = base + offset
    emitter.instruction("mov rdx, QWORD PTR [rax]");                            // candidate name pointer (strcmp arg b)
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // candidate name length (strcmp arg b)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // query name pointer (strcmp arg a)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // query name length (strcmp arg a)
    emitter.instruction("call __rt_strcmp");                                    // compare query against the candidate
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload mid after the call
    emitter.instruction("test rax, rax");                                       // inspect the ordering result
    emitter.instruction("je __rt_sns_found");                                   // zero means an exact match
    emitter.instruction("js __rt_sns_set_hi");                                  // query < candidate searches the lower half
    emitter.instruction("add r9, 1");                                           // query > candidate searches the upper half
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // lo = mid + 1
    emitter.instruction("jmp __rt_sns_loop");                                   // continue the search

    emitter.label("__rt_sns_set_hi");
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // hi = mid
    emitter.instruction("jmp __rt_sns_loop");                                   // continue the search

    emitter.label("__rt_sns_found");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload mid for the matched entry
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload table base
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload entry size
    emitter.instruction("imul rax, r11");                                       // recompute the matched byte offset
    emitter.instruction("add rax, r10");                                        // return pointer = base + offset
    emitter.instruction("jmp __rt_sns_done");                                   // share the epilogue

    emitter.label("__rt_sns_not_found");
    emitter.instruction("xor eax, eax");                                        // report a miss with a null pointer

    emitter.label("__rt_sns_done");
    emitter.instruction("mov rsp, rbp");                                        // discard the search frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return entry pointer in rax
}

