//! Purpose:
//! Emits `__rt_array_element_type_check`, the packed-array tag predicate used by fused
//! `is_int($a[$i])`, `is_string($a[$i])`, `is_array($a[$i])` and `is_null($a[$i])` calls.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - Input: array pointer, integer index, and a 16-bit runtime-tag mask.
//! - Output: raw PHP bool (0 or 1) in the integer result register.
//! - Packed arrays of boxed Mixed cells answer by reading the stored cell tag. Missing
//!   keys, hashes, typed slots, references and nested Mixed wrappers fall back to
//!   `__rt_array_get_mixed_key` + `__rt_mixed_unbox` + `__rt_decref_mixed`, so the
//!   warning and ownership behavior stays exactly the generic read's.

use crate::codegen_support::callable_invoker_args::INVOKER_ARG_REF_CELL_TAG;
use crate::codegen_support::sentinels::NULL_SENTINEL;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the fused packed-array tag predicate for the active target.
pub fn emit_array_type_check(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_type_check_x86_64(emitter);
        return;
    }
    emit_array_type_check_aarch64(emitter);
}

/// Emits `__rt_array_element_type_check` for AArch64.
fn emit_array_type_check_aarch64(emitter: &mut Emitter) {
    let fallback = "__rt_array_element_type_check_fallback";
    let miss_null = "__rt_array_element_type_check_null";
    let done = "__rt_array_element_type_check_done";

    emitter.blank();
    emitter.comment("--- runtime: array_element_type_check ---");
    emitter.label_global("__rt_array_element_type_check");
    emitter.instruction("sub sp, sp, #64");                                     // reserve the saved inputs, cell owner, and result
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable frame base
    emitter.instruction("str x0, [sp, #0]");                                    // save the array pointer for the generic fallback
    emitter.instruction("str x1, [sp, #8]");                                    // save the integer index for the generic fallback
    emitter.instruction("str x2, [sp, #16]");                                   // save the runtime-tag mask across helper calls

    // -- fast path: packed arrays of boxed Mixed cells answer from the stored tag --
    emitter.instruction("cbz x0, __rt_array_element_type_check_null_receiver"); // a null receiver needs the generic warning path
    abi::emit_load_int_immediate(emitter, "x9", NULL_SENTINEL);
    emitter.instruction("cmp x0, x9");                                          // does the receiver carry the in-band null-container sentinel?
    emitter.instruction(&format!("b.eq {fallback}"));                           // sentinel-null receivers need the generic warning path
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed array kind word from the heap header
    emitter.instruction("and x10, x9, #0xff");                                  // isolate the storage kind byte
    emitter.instruction("cmp x10, #2");                                         // is this packed indexed storage?
    emitter.instruction(&format!("b.ne {fallback}"));                           // hashes and unknown storage use the generic read
    emitter.instruction("ubfx x11, x9, #8, #7");                                // extract the runtime element value_type tag
    emitter.instruction("cmp x11, #7");                                         // are the slots boxed Mixed pointers?
    emitter.instruction(&format!("b.ne {fallback}"));                           // typed slots and inline tagged scalars use the generic read
    emitter.instruction("ldr x12, [x0]");                                       // load the packed element count
    emitter.instruction("cmp x1, #0");                                          // is the integer index negative?
    emitter.instruction(&format!("b.lt {fallback}"));                           // a negative key follows the generic missing-key path
    emitter.instruction("cmp x1, x12");                                         // is the integer index past the packed payload?
    emitter.instruction(&format!("b.ge {fallback}"));                           // an out-of-bounds key follows the generic warning path
    emitter.instruction("add x10, x0, #24");                                    // skip the indexed-array header to the payload base
    emitter.instruction("ldr x13, [x10, x1, lsl #3]");                          // load the stored boxed Mixed cell pointer
    emitter.instruction(&format!("cbz x13, {miss_null}"));                      // an empty slot reads as PHP null
    emitter.instruction("ldr x14, [x13]");                                      // load the stored runtime tag
    emitter.instruction("cmp x14, #7");                                         // is the slot a nested Mixed wrapper?
    emitter.instruction(&format!("b.eq {fallback}"));                           // nested wrappers need the unboxing helper
    emitter.instruction(&format!("cmp x14, #{INVOKER_ARG_REF_CELL_TAG}"));      // is the slot a by-reference invoker marker?
    emitter.instruction(&format!("b.eq {fallback}"));                           // references need the generic dereference path
    emitter.instruction("lsr x14, x2, x14");                                    // move the selected mask bit into the low bit
    emitter.instruction("and x0, x14, #1");                                     // materialize the PHP bool result
    emitter.instruction(&format!("b {done}"));                                  // return the fast-path predicate

    emitter.label("__rt_array_element_type_check_null_receiver");
    emitter.instruction(&format!("b {fallback}"));                              // route an ordinary null receiver through the generic read

    emitter.label(miss_null);
    emitter.instruction("lsr x14, x2, #8");                                     // select the null bit in the caller's mask
    emitter.instruction("and x0, x14, #1");                                     // materialize the PHP bool result
    emitter.instruction(&format!("b {done}"));                                  // return the missing-slot predicate

    emitter.label(fallback);
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the integer index
    emitter.instruction("mov x2, #-1");                                         // int-key sentinel in the high key word
    emitter.instruction("mov x3, #1");                                          // read context warns on a missing key like PHP
    emitter.instruction("bl __rt_array_get_mixed_key");                         // obtain the owned boxed element through the generic path
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the owned cell for release
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel nested Mixed wrappers and return the concrete tag
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the runtime-tag mask clobbered by the helpers
    emitter.instruction("lsr x14, x2, x0");                                     // move the selected mask bit into the low bit
    emitter.instruction("and x14, x14, #1");                                    // materialize the PHP bool result before releasing the cell
    emitter.instruction("str x14, [sp, #32]");                                  // preserve the predicate across the cell release
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the owned cell for release
    emitter.instruction("bl __rt_decref_mixed");                                // release the copied cell and its payload references
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the completed predicate

    emitter.label(done);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the predicate frame
    emitter.instruction("ret");                                                 // return the PHP bool in x0
}

/// Emits `__rt_array_element_type_check` for Linux x86_64.
fn emit_array_type_check_x86_64(emitter: &mut Emitter) {
    let fallback = "__rt_array_element_type_check_fallback_x86";
    let miss_null = "__rt_array_element_type_check_null_x86";
    let done = "__rt_array_element_type_check_done_x86";

    emitter.blank();
    emitter.comment("--- runtime: array_element_type_check ---");
    emitter.label_global("__rt_array_element_type_check");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve the saved inputs, cell owner, and result
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the array pointer for the generic fallback
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the integer index for the generic fallback
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the runtime-tag mask across helper calls

    // -- fast path: packed arrays of boxed Mixed cells answer from the stored tag --
    emitter.instruction("test rdi, rdi");                                       // a null receiver needs the generic warning path
    emitter.instruction(&format!("jz {fallback}"));                             // follow the generic read for a null container
    abi::emit_load_int_immediate(emitter, "r10", NULL_SENTINEL);
    emitter.instruction("cmp rdi, r10");                                        // does the receiver carry the in-band null-container sentinel?
    emitter.instruction(&format!("je {fallback}"));                             // sentinel-null receivers need the generic warning path
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the packed array kind word from the heap header
    emitter.instruction("mov r11, r10");                                        // copy the kind word before isolating the storage byte
    emitter.instruction("and r11d, 0xff");                                      // isolate the storage kind byte
    emitter.instruction("cmp r11, 2");                                          // is this packed indexed storage?
    emitter.instruction(&format!("jne {fallback}"));                            // hashes and unknown storage use the generic read
    emitter.instruction("shr r10, 8");                                          // move the runtime element value_type tag into the low byte
    emitter.instruction("and r10d, 0x7f");                                      // isolate the element value_type tag
    emitter.instruction("cmp r10, 7");                                          // are the slots boxed Mixed pointers?
    emitter.instruction(&format!("jne {fallback}"));                            // typed slots and inline tagged scalars use the generic read
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the packed element count
    emitter.instruction("cmp rsi, 0");                                          // is the integer index negative?
    emitter.instruction(&format!("jl {fallback}"));                             // a negative key follows the generic missing-key path
    emitter.instruction("cmp rsi, r10");                                        // is the integer index past the packed payload?
    emitter.instruction(&format!("jge {fallback}"));                            // an out-of-bounds key follows the generic warning path
    emitter.instruction("lea r10, [rdi + 24]");                                 // skip the indexed-array header to the payload base
    emitter.instruction("mov r11, QWORD PTR [r10 + rsi * 8]");                  // load the stored boxed Mixed cell pointer
    emitter.instruction(&format!("test r11, r11"));                             // an empty slot reads as PHP null
    emitter.instruction(&format!("jz {miss_null}"));                            // answer from the null mask bit
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the stored runtime tag
    emitter.instruction("cmp rax, 7");                                          // is the slot a nested Mixed wrapper?
    emitter.instruction(&format!("je {fallback}"));                             // nested wrappers need the unboxing helper
    emitter.instruction(&format!("cmp rax, {INVOKER_ARG_REF_CELL_TAG}"));       // is the slot a by-reference invoker marker?
    emitter.instruction(&format!("je {fallback}"));                             // references need the generic dereference path
    emitter.instruction("mov rcx, rax");                                        // place the tag in the shift-count register
    emitter.instruction("mov rax, rdx");                                        // load the runtime-tag mask
    emitter.instruction("shr rax, cl");                                         // move the selected mask bit into the low bit
    emitter.instruction("and eax, 1");                                          // materialize the PHP bool result
    emitter.instruction(&format!("jmp {done}"));                                // return the fast-path predicate

    emitter.label(miss_null);
    emitter.instruction("mov rax, rdx");                                        // load the runtime-tag mask
    emitter.instruction("shr rax, 8");                                          // select the null bit in the caller's mask
    emitter.instruction("and eax, 1");                                          // materialize the PHP bool result
    emitter.instruction(&format!("jmp {done}"));                                // return the missing-slot predicate

    emitter.label(fallback);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the integer index
    emitter.instruction("mov rdx, -1");                                         // int-key sentinel in the high key word
    emitter.instruction("mov rcx, 1");                                          // read context warns on a missing key like PHP
    emitter.instruction("call __rt_array_get_mixed_key");                       // obtain the owned boxed element through the generic path
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the owned cell for release
    emitter.instruction("call __rt_mixed_unbox");                               // peel nested Mixed wrappers and return the concrete tag
    emitter.instruction("mov rcx, rax");                                        // place the concrete tag in the shift-count register
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the runtime-tag mask clobbered by the helpers
    emitter.instruction("shr rax, cl");                                         // move the selected mask bit into the low bit
    emitter.instruction("and eax, 1");                                          // materialize the PHP bool result before releasing the cell
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the predicate across the cell release
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the owned cell for release
    emitter.instruction("call __rt_decref_mixed");                              // release the copied cell and its payload references
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the completed predicate

    emitter.label(done);
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the PHP bool in rax
}
