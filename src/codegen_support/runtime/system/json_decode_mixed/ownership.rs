//! Purpose:
//! Emits ownership boundaries for recursive JSON construction and error unwinding.
//!
//! Called from:
//! - `super::emit_json_decode_mixed()` while emitting the shared runtime.
//!
//! Key details:
//! - Recursive parsers return failures normally so each frame releases partial owners.
//! - Only the outer entry restores JSON_THROW_ON_ERROR and publishes the exception.
//! - Container boxing retires the fresh owner after Mixed acquires its own reference.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the public decode boundary and the fresh-container boxing adapter on every target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Defers AArch64 JSON exceptions until recursive parser owners have been released.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed ownership boundary ---");
    emitter.label_global("__rt_json_decode_mixed");

    // -- save flags without disturbing the borrowed source in x1/x2 --
    emitter.instruction("sub sp, sp, #32");                                     // reserve flags, result, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the outer caller frame
    emitter.instruction("add x29, sp, #16");                                    // establish the boundary frame
    abi::emit_load_symbol_to_reg(emitter, "x9", "_json_active_flags", 0);
    emitter.instruction("str x9, [sp]");                                        // preserve every original decoding flag
    emitter.instruction("mov x10, #4194304");                                   // isolate JSON_THROW_ON_ERROR
    emitter.instruction("bic x9, x9, x10");                                     // let recursive failures unwind normally
    abi::emit_store_reg_to_symbol(emitter, "x9", "_json_active_flags", 0);

    // -- parse, then restore throwing semantics after all partial owners retire --
    emitter.instruction("bl __rt_json_decode_mixed_value");                     // recursively build a fresh Mixed graph or return zero
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the result while restoring parser state
    emitter.instruction("ldr x9, [sp]");                                        // reload the caller's complete flag mask
    abi::emit_store_reg_to_symbol(emitter, "x9", "_json_active_flags", 0);
    emitter.instruction("cbnz x0, __rt_json_decode_owned_done");                // successful values need no error publication
    abi::emit_store_zero_to_symbol(emitter, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(emitter, "_json_error_source_ptr", 0);
    abi::emit_load_symbol_to_reg(emitter, "x0", "_json_last_error", 0);
    emitter.instruction("bl __rt_json_throw_error");                            // throw only after recursive cleanup has completed
    emitter.label("__rt_json_decode_owned_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the fresh value or the normal failure signal
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame after parsing
    emitter.instruction("add sp, sp, #32");                                     // release the boundary frame
    emitter.instruction("ret");                                                 // return to the PHP-visible decode call

    emitter.blank();
    emitter.comment("--- runtime: json_box_owned_container ---");
    emitter.label_global("__rt_json_box_owned_container");

    // -- tags 4/5/6 keep the borrowed boxing ABI while consuming the fresh payload --
    emitter.instruction("sub sp, sp, #32");                                     // reserve payload, result, and saved linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the container builder frame
    emitter.instruction("add x29, sp, #16");                                    // establish a frame across retain and release
    emitter.instruction("str x1, [sp]");                                        // save the fresh array, hash, or object owner
    emitter.instruction("bl __rt_mixed_from_value");                            // let the new Mixed cell retain the container
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the new boxed owner across release
    emitter.instruction("ldr x0, [sp]");                                        // recover the builder's original container reference
    emitter.instruction("bl __rt_decref_any");                                  // retire that reference after Mixed has acquired its own
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the independently owned Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the container builder frame
    emitter.instruction("add sp, sp, #32");                                     // release boxing scratch storage
    emitter.instruction("ret");                                                 // transfer the boxed graph to the parser caller
}

/// Mirrors deferred exceptions and consuming container boxing for the x86_64 runtime ABI.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode_mixed ownership boundary ---");
    emitter.label_global("__rt_json_decode_mixed");

    // -- save flags without disturbing the borrowed source in rax/rdx --
    emitter.instruction("push rbp");                                            // preserve the outer caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for flags and result
    emitter.instruction("sub rsp, 16");                                         // reserve aligned boundary scratch storage
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_active_flags", 0);
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // preserve every original decoding flag
    emitter.instruction("and r10, -4194305");                                   // clear only JSON_THROW_ON_ERROR during recursion
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_active_flags", 0);

    // -- restore throwing semantics only after the recursive parser has unwound --
    emitter.instruction("call __rt_json_decode_mixed_value");                   // build the fresh graph or release partial owners and return zero
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the completed parser result
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the original complete flag mask
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_active_flags", 0);
    emitter.instruction("test rax, rax");                                       // distinguish a value from the normal failure signal
    emitter.instruction("jne __rt_json_decode_owned_done_x");                   // successful parses require no error publication
    abi::emit_store_zero_to_symbol(emitter, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(emitter, "_json_error_source_ptr", 0);
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_last_error", 0);
    emitter.instruction("call __rt_json_throw_error");                          // throw only after every partial parser owner has retired
    emitter.label("__rt_json_decode_owned_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the fresh value or normal failure result
    emitter.instruction("mov rsp, rbp");                                        // release boundary scratch storage
    emitter.instruction("pop rbp");                                             // restore the outer caller frame pointer
    emitter.instruction("ret");                                                 // return to the PHP-visible decode call

    emitter.blank();
    emitter.comment("--- runtime: json_box_owned_container ---");
    emitter.label_global("__rt_json_box_owned_container");

    // -- tags 4/5/6 consume the fresh payload after the ordinary boxer retains it --
    emitter.instruction("push rbp");                                            // preserve the container builder frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable payload and result spill slots
    emitter.instruction("sub rsp, 16");                                         // keep calls aligned while saving both owners
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the fresh array, hash, or object reference
    emitter.instruction("call __rt_mixed_from_value");                          // acquire the container reference owned by the Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the boxed result across uniform release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the original builder reference
    emitter.instruction("call __rt_decref_any");                                // retire the fresh owner now retained by Mixed
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the independently owned Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // release the boxing scratch frame
    emitter.instruction("pop rbp");                                             // restore the container builder frame pointer
    emitter.instruction("ret");                                                 // transfer the boxed graph to the parser caller
}
