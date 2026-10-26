//! Purpose:
//! Emits `__rt_php_compare_slots` / `__rt_php_compare_slots_desc`: comparator
//! callbacks that order two boxed `Mixed` array slots by PHP's own rules.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Uses `(a, b)` in the first two argument registers and ordering in the result
//!   register. The stable scalar-slot sorter consumes this adapter without
//!   duplicating the shared PHP ordering table.
//! - A validation pass rejects non-scalar runtime tags before sorting. The shared
//!   comparator does not yet implement PHP ordering for containers or objects,
//!   so accepting those values would silently report them as equal.
//! - The ordering itself is `__rt_php_compare`, which is what `<` and `<=>`
//!   already use for runtime-tagged operands. Reimplementing PHP's cross-type
//!   comparison here would be a second answer to a question the runtime already
//!   answers, and the two would drift.
//! - The descending variant negates the result rather than swapping the operands.
//!   NAN comparisons are not antisymmetric, so swapping operands would give a
//!   different result from reversing the PHP comparison result.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::data::MIXED_SORT_NON_SCALAR_MSG;

/// Emits the Mixed-sort scalar guard and both comparator callbacks for the current target.
pub fn emit_php_compare_slots(emitter: &mut Emitter) {
    emit_mixed_sort_scalar_guard(emitter);
    emit_one(emitter, "__rt_php_compare_slots", false);
    emit_one(emitter, "__rt_php_compare_slots_desc", true);
}

/// Emits a target-specific guard that rejects non-scalar Mixed array elements.
///
/// The helper receives an indexed-array pointer in the first integer argument
/// register and returns the same pointer in the integer result register. Tags
/// 0 through 3 and tag 8 are the supported scalar and null values. Any other
/// tag reports the unsupported runtime value and terminates before sorting.
fn emit_mixed_sort_scalar_guard(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_sort_require_scalars ---");
    emitter.label_global("__rt_mixed_sort_require_scalars");

    match emitter.target.arch {
        Arch::AArch64 => {
            // -- scan every boxed Mixed slot before the sorter can mutate the array --
            emitter.instruction("ldr x9, [x0]");                                // load the indexed-array element count
            emitter.instruction("mov x10, #0");                                 // start at the first Mixed slot
            emitter.instruction("add x11, x0, #24");                            // point past the indexed-array header to its slots
            emitter.label("__rt_mixed_sort_scalar_scan");
            emitter.instruction("cmp x10, x9");                                 // have all live Mixed slots been validated?
            emitter.instruction("b.hs __rt_mixed_sort_scalar_done");            // return once every runtime tag is supported
            emitter.instruction("ldr x12, [x11, x10, lsl #3]");                 // load the boxed Mixed cell pointer for this slot
            emitter.instruction("ldr x12, [x12]");                              // read the cell's runtime value tag
            emitter.instruction("cmp x12, #3");                                 // are int, string, float, or bool semantics sufficient?
            emitter.instruction("b.ls __rt_mixed_sort_scalar_next");            // accept runtime scalar tags 0 through 3
            emitter.instruction("cmp x12, #8");                                 // is this value PHP null?
            emitter.instruction("b.ne __rt_mixed_sort_non_scalar");             // reject containers, objects, resources, and callables
            emitter.label("__rt_mixed_sort_scalar_next");
            emitter.instruction("add x10, x10, #1");                            // advance to the next Mixed slot
            emitter.instruction("b __rt_mixed_sort_scalar_scan");               // continue validating runtime tags
            emitter.label("__rt_mixed_sort_scalar_done");
            emitter.instruction("ret");                                         // return the unchanged array pointer in x0

            // -- report an unsupported runtime value before terminating --
            emitter.label("__rt_mixed_sort_non_scalar");
            emitter.instruction("mov x0, #2");                                  // select stderr for the explicit Mixed-sort diagnostic
            abi::emit_symbol_address(emitter, "x1", "_mixed_sort_non_scalar_msg");
            emitter.instruction(&format!("mov x2, #{}", MIXED_SORT_NON_SCALAR_MSG.len())); // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            // -- scan every boxed Mixed slot before the sorter can mutate the array --
            emitter.instruction("sub rsp, 8");                                  // align the stack for the fatal path's output-buffer flush
            emitter.instruction("mov rax, rdi");                                // preserve the array pointer as the helper result
            emitter.instruction("mov rcx, QWORD PTR [rax]");                    // load the indexed-array element count
            emitter.instruction("xor edx, edx");                                // start at the first Mixed slot
            emitter.instruction("lea r8, [rax + 24]");                          // point past the indexed-array header to its slots
            emitter.label("__rt_mixed_sort_scalar_scan_x86");
            emitter.instruction("cmp rdx, rcx");                                // have all live Mixed slots been validated?
            emitter.instruction("jae __rt_mixed_sort_scalar_done_x86");         // return once every runtime tag is supported
            emitter.instruction("mov r9, QWORD PTR [r8 + rdx * 8]");            // load the boxed Mixed cell pointer for this slot
            emitter.instruction("mov r9, QWORD PTR [r9]");                      // read the cell's runtime value tag
            emitter.instruction("cmp r9, 3");                                   // are int, string, float, or bool semantics sufficient?
            emitter.instruction("jbe __rt_mixed_sort_scalar_next_x86");         // accept runtime scalar tags 0 through 3
            emitter.instruction("cmp r9, 8");                                   // is this value PHP null?
            emitter.instruction("jne __rt_mixed_sort_non_scalar_x86");          // reject containers, objects, resources, and callables
            emitter.label("__rt_mixed_sort_scalar_next_x86");
            emitter.instruction("add rdx, 1");                                  // advance to the next Mixed slot
            emitter.instruction("jmp __rt_mixed_sort_scalar_scan_x86");         // continue validating runtime tags
            emitter.label("__rt_mixed_sort_scalar_done_x86");
            emitter.instruction("add rsp, 8");                                  // release the alignment slot before returning
            emitter.instruction("ret");                                         // return the unchanged array pointer in rax

            // -- report an unsupported runtime value before terminating --
            emitter.label("__rt_mixed_sort_non_scalar_x86");
            emitter.instruction("mov edi, 2");                                  // select stderr for the explicit Mixed-sort diagnostic
            abi::emit_symbol_address(emitter, "rsi", "_mixed_sort_non_scalar_msg");
            emitter.instruction(&format!("mov edx, {}", MIXED_SORT_NON_SCALAR_MSG.len())); // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // select the Linux x86_64 write syscall
            emitter.instruction("syscall");                                     // write the explicit Mixed-sort diagnostic
            abi::emit_cdylib_exit_escape(emitter);
            emitter.instruction("call __rt_ob_flush_all");                      // flush buffered output from the aligned helper frame
            emitter.instruction("mov edi, 1");                                  // select process failure status
            emitter.instruction("mov eax, 231");                                // Linux x86_64 syscall 231 is exit_group
            emitter.instruction("syscall");                                     // terminate after reporting the unsupported value
        }
    }
}

/// Emits one target-specific Mixed-slot comparator in the requested direction.
fn emit_one(emitter: &mut Emitter, label: &str, descending: bool) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {label} ---"));
    emitter.label_global(label);

    match emitter.target.arch {
        Arch::AArch64 => {
            // Frame: [sp,#0] b's cell, [sp,#8] a.tag, [sp,#16] a.lo, [sp,#24] a.hi.
            emitter.instruction("sub sp, sp, #48");                             // reserve comparator locals and the saved frame record
            emitter.instruction("stp x29, x30, [sp, #32]");                     // save the frame record across two nested calls
            emitter.instruction("add x29, sp, #32");                            // establish a stable comparator frame pointer
            emitter.instruction("str x1, [sp, #0]");                            // keep b's cell: unboxing a clobbers the argument registers
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // a becomes x0=tag, x1=lo, x2=hi
            emitter.instruction("stp x0, x1, [sp, #8]");                        // stash a's tag and low word
            emitter.instruction("str x2, [sp, #24]");                           // stash a's high word
            emitter.instruction("ldr x0, [sp, #0]");                            // reload b's boxed Mixed cell for unboxing
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // b becomes x0=tag, x1=lo, x2=hi
            emitter.instruction("mov x3, x0");                                  // right tag
            emitter.instruction("mov x4, x1");                                  // right low word
            emitter.instruction("mov x5, x2");                                  // right high word
            emitter.instruction("ldp x0, x1, [sp, #8]");                        // left tag and low word
            emitter.instruction("ldr x2, [sp, #24]");                           // left high word
            abi::emit_call_label(emitter, "__rt_php_compare");                  // return ordering as x0 = -1, 0, or 1
            if descending {
                emitter.instruction("neg x0, x0");                              // reverse the order without reversing equal elements
            }
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore the caller frame pointer and return address
            emitter.instruction("add sp, sp, #48");                             // release comparator locals
            emitter.instruction("ret");                                         // return the PHP ordering result
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish a stable comparator frame
            emitter.instruction("sub rsp, 48");                                 // locals, and rsp stays 16-byte aligned for the nested calls
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // keep b's cell before unboxing a clobbers it
            emitter.instruction("mov rax, rdi");                                // __rt_mixed_unbox reads its cell from rax
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // a becomes rax=tag, rdi=lo, rdx=hi
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // save a's runtime value tag
            emitter.instruction("mov QWORD PTR [rbp - 24], rdi");               // save a's low payload word
            emitter.instruction("mov QWORD PTR [rbp - 32], rdx");               // save a's high payload word
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // reload b's boxed Mixed cell for unboxing
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // b becomes rax=tag, rdi=lo, rdx=hi
            emitter.instruction("mov rcx, rax");                                // right tag
            emitter.instruction("mov r8, rdi");                                 // right low word
            emitter.instruction("mov r9, rdx");                                 // right high word
            emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");               // left tag
            emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");               // left low word
            emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");               // left high word
            abi::emit_call_label(emitter, "__rt_php_compare");                  // return ordering as rax = -1, 0, or 1
            if descending {
                emitter.instruction("neg rax");                                 // reverse the order without reversing equal elements
            }
            emitter.instruction("add rsp, 48");                                 // release comparator locals
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the PHP ordering result
        }
    }
}
