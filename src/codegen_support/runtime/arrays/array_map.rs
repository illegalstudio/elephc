//! Purpose:
//! Emits the `__rt_array_map` runtime helper assembly for indexed array maps.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The result array is owned by the helper until the callback loop completes. A callback
//!   throw is caught by a bounded cleanup guard that releases the partial result before the
//!   original exception is rethrown.
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Main frame: context slots below the frame footer and the cleanup-guard spill area.
const FRAME: usize = 96;
/// Distance from the current stack pointer to the frame footer, used to make the context
/// pointer addressable with positive offsets from the body.
const CONTEXT_BASE: usize = FRAME - 16;
const CALLBACK: usize = 8;
const SOURCE: usize = 16;
const ENV: usize = 24;
const RESULT: usize = 32;
const LENGTH: usize = 40;
const WIDTH: usize = 48;
const PENDING: usize = 56;

/// Converts a main-frame slot offset into a positive offset from the context pointer.
fn context_offset(slot: usize) -> usize {
    CONTEXT_BASE - slot
}

/// Emits the `__rt_array_map` runtime helper for both supported architectures.
///
/// # Input registers
/// - `x0`/`rdi`: callback function address
/// - `x1`/`rsi`: source array pointer
/// - `x2`/`rdx`: optional callback environment pointer (capture closure)
///
/// # Output registers
/// - `x0`/`rax`: pointer to the new mapped array
pub fn emit_array_map(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(emitter.target, 2);
    emitter.blank();
    emitter.comment("--- runtime: array_map ---");
    emitter.label_global("__rt_array_map");

    // -- preserve the callback contract and source metadata in the context frame --
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [CALLBACK, SOURCE, ENV].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::store_at_offset(emitter, result, PENDING);
    abi::load_at_offset(emitter, result, SOURCE);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::store_at_offset(emitter, result, LENGTH);
    abi::load_at_offset(emitter, result, SOURCE);
    abi::emit_load_from_address(emitter, result, result, 16);
    abi::store_at_offset(emitter, result, WIDTH);

    // -- allocate the destination array before the callback boundary --
    abi::load_at_offset(emitter, arg0, LENGTH);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    abi::store_at_offset(emitter, result, RESULT);

    // -- protect the partial result across callback throws --
    abi::emit_temporary_stack_address(emitter, arg1, 0);
    abi::emit_symbol_address(emitter, arg0, "__rt_array_map_body");
    abi::emit_frame_slot_address(emitter, arg2, PENDING);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_map_throw");

    // -- publish the mapped length and return the completed array --
    abi::load_at_offset(emitter, result, RESULT);
    abi::load_at_offset(emitter, abi::secondary_scratch_reg(emitter), LENGTH);
    abi::emit_store_to_address(emitter, abi::secondary_scratch_reg(emitter), result, 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    // -- release the partial result, then rethrow the callback exception --
    emitter.label("__rt_array_map_throw");
    abi::load_at_offset(emitter, result, RESULT);
    abi::emit_reg_move(emitter, arg1, result);
    abi::emit_symbol_address(emitter, arg0, "__rt_decref_any");
    abi::emit_frame_slot_address(emitter, arg2, PENDING);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::load_at_offset(emitter, result, PENDING);
    let older = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    abi::emit_load_symbol_to_reg(emitter, older, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");

    emit_body(emitter);
}

/// Emits the guarded callback loop. The cleanup invoker passes the context pointer in
/// `x0`/`rax`; a callback throw longjmps back to the invoker, which releases the partial
/// result and rethrows from the caller.
fn emit_body(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_array_map_body");
    match emitter.target.arch {
        Arch::AArch64 => emit_body_aarch64(emitter),
        Arch::X86_64 => emit_body_x86_64(emitter),
    }
}

/// Emits the AArch64 callback loop with the context in x19 and the index in x20.
fn emit_body_aarch64(emitter: &mut Emitter) {
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve body linkage
    emitter.instruction("mov x29, sp");                                         // establish a stable body frame
    emitter.instruction("stp x19, x20, [sp, #-16]!");                           // preserve the context and index registers
    emitter.instruction("mov x19, x0");                                         // x19 = caller-owned context pointer
    emitter.instruction("mov x20, xzr");                                        // x20 = current source index
    emitter.label("__rt_array_map_body_loop");
    emitter.instruction(&format!("ldr x9, [x19, #{}]", context_offset(LENGTH)));// load the source element count
    emitter.instruction("cmp x20, x9");                                         // has every source element been mapped?
    emitter.instruction("b.ge __rt_array_map_body_done");                       // finish the loop
    emitter.instruction(&format!("ldr x1, [x19, #{}]", context_offset(SOURCE)));// load the source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip the indexed-array header to the payload base
    emitter.instruction(&format!("ldr x10, [x19, #{}]", context_offset(WIDTH)));// load the source element width
    emitter.instruction("cmp x10, #16");                                        // does the source hold string pointer/length pairs?
    emitter.instruction("b.eq __rt_array_map_body_str");                        // use the string callback ABI
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // load the scalar source element
    emitter.instruction(&format!("ldr x9, [x19, #{}]", context_offset(ENV)));   // load the optional capture environment
    emitter.instruction("cbz x9, __rt_array_map_body_call");                    // keep the one-argument callback ABI without captures
    emitter.instruction("mov x1, x9");                                          // pass the capture environment as the second argument
    emitter.instruction("b __rt_array_map_body_call");                          // invoke the callback
    emitter.label("__rt_array_map_body_str");
    emitter.instruction("lsl x11, x20, #4");                                    // convert the logical index into a 16-byte string-slot offset
    emitter.instruction("add x11, x1, x11");                                    // address the current source string slot
    emitter.instruction("ldr x0, [x11]");                                       // load the source string pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // load the source string length
    emitter.instruction(&format!("ldr x9, [x19, #{}]", context_offset(ENV)));   // load the optional capture environment
    emitter.instruction("cbz x9, __rt_array_map_body_call");                    // keep the string callback ABI without captures
    emitter.instruction("mov x2, x9");                                          // pass the capture environment after ptr/len
    emitter.label("__rt_array_map_body_call");
    emitter.instruction(&format!("ldr x9, [x19, #{}]", context_offset(CALLBACK))); // load the callback address
    emitter.instruction("blr x9");                                              // invoke the user callback
    emitter.instruction(&format!("ldr x1, [x19, #{}]", context_offset(RESULT)));// reload the destination array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip the destination header to the payload base
    emitter.instruction("str x0, [x1, x20, lsl #3]");                           // store the mapped value at the matching index
    emitter.instruction("add x20, x20, #1");                                    // advance the mapping index
    emitter.instruction("b __rt_array_map_body_loop");                          // continue until every element is mapped
    emitter.label("__rt_array_map_body_done");
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the context and index registers
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore body linkage
    emitter.instruction("ret");                                                 // return to the cleanup invoker
}

/// Emits the x86_64 callback loop with the context in r12 and the index in r13.
fn emit_body_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve body linkage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable body frame
    emitter.instruction("push r12");                                            // preserve the context register
    emitter.instruction("push r13");                                            // preserve the index register
    emitter.instruction("mov r12, rax");                                        // r12 = caller-owned context pointer
    emitter.instruction("xor r13d, r13d");                                      // r13 = current source index
    emitter.label("__rt_array_map_body_loop");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", context_offset(LENGTH))); // load the source element count
    emitter.instruction("cmp r13, r10");                                        // has every source element been mapped?
    emitter.instruction("jge __rt_array_map_body_done");                        // finish the loop
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", context_offset(SOURCE))); // load the source array pointer
    emitter.instruction("add r10, 24");                                         // skip the indexed-array header to the payload base
    emitter.instruction(&format!("mov r11, QWORD PTR [r12 + {}]", context_offset(WIDTH))); // load the source element width
    emitter.instruction("cmp r11, 16");                                         // does the source hold string pointer/length pairs?
    emitter.instruction("je __rt_array_map_body_str");                          // use the string callback ABI
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8]");                  // load the scalar source element
    emitter.instruction(&format!("mov r11, QWORD PTR [r12 + {}]", context_offset(ENV))); // load the optional capture environment
    emitter.instruction("test r11, r11");                                       // is a capture environment present?
    emitter.instruction("jz __rt_array_map_body_call");                         // keep the one-argument callback ABI without captures
    emitter.instruction("mov rsi, r11");                                        // pass the capture environment as the second argument
    emitter.instruction("jmp __rt_array_map_body_call");                        // invoke the callback
    emitter.label("__rt_array_map_body_str");
    emitter.instruction("mov rcx, r13");                                        // copy the logical source index
    emitter.instruction("shl rcx, 4");                                          // convert it into a 16-byte string-slot offset
    emitter.instruction("lea rcx, [r10 + rcx]");                                // address the current source string slot
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load the source string pointer
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // load the source string length
    emitter.instruction(&format!("mov r11, QWORD PTR [r12 + {}]", context_offset(ENV))); // load the optional capture environment
    emitter.instruction("test r11, r11");                                       // is a capture environment present?
    emitter.instruction("jz __rt_array_map_body_call");                         // keep the string callback ABI without captures
    emitter.instruction("mov rdx, r11");                                        // pass the capture environment after ptr/len
    emitter.label("__rt_array_map_body_call");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", context_offset(CALLBACK))); // load the callback address
    emitter.instruction("call r10");                                            // invoke the user callback
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", context_offset(RESULT))); // reload the destination array pointer
    emitter.instruction("mov QWORD PTR [r10 + r13 * 8 + 24], rax");             // store the mapped value at the matching index
    emitter.instruction("inc r13");                                             // advance the mapping index
    emitter.instruction("jmp __rt_array_map_body_loop");                        // continue until every element is mapped
    emitter.label("__rt_array_map_body_done");
    emitter.instruction("pop r13");                                             // restore the index register
    emitter.instruction("pop r12");                                             // restore the context register
    emitter.instruction("pop rbp");                                             // restore body linkage
    emitter.instruction("ret");                                                 // return to the cleanup invoker
}
