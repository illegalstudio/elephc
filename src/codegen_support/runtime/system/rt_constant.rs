//! Purpose:
//! Emits `__rt_constant`, the runtime helper backing a non-literal
//! `constant($name)` call: it resolves a registered scalar constant, or a
//! `Class::NAME` class constant or enum case, into an owned boxed Mixed, or
//! throws a catchable `\Error` on a miss.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (gated by the
//!   `const_introspection` feature) and the EIR `constant` lowering.
//!
//! Key details:
//! - Input:  x0/rdi=name_ptr, x1/rsi=name_len.
//! - Output: x0/rax = boxed Mixed pointer on a hit (does not return on a miss).
//! - A name containing `::` is split there and read through the program's
//!   `__elephc_eval_value_class_constant_get(class, constant, scope)` getter, which the
//!   user assembly emits whenever this helper exists: public class constants and enum
//!   cases (Monolog's `constant(Level::class . '::' . $name)`). A miss is php's
//!   `Undefined constant C::X`, or `Class "C" not found` when no class has that name.
//! - On a global hit, the entry's pre-boxed `(tag, lo, hi)` triple is handed to
//!   `__rt_mixed_from_value`, which owns the payload (string persist / refcount).
//! - On a global miss, the helper builds the PHP 8 message `Undefined constant "NAME"`
//!   with `__rt_concat`/`__rt_str_persist`, stamps a `\Error` object
//!   (`_constant_error_class_id`), publishes it to `_exc_value`, and branches to
//!   `__rt_throw_current`.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Byte length of the `Undefined constant "` message prefix.
const MSG_PREFIX_LEN: i64 = 20;
/// Byte length of `Undefined constant `: the same prefix without its quote, for `C::X`.
const CLASS_CONSTANT_MSG_PREFIX_LEN: i64 = 19;
/// Byte length of the `Class "` prefix of a missing class.
const CLASS_MSG_PREFIX_LEN: i64 = 7;
/// Byte length of the `" not found` suffix of a missing class.
const CLASS_MSG_SUFFIX_LEN: i64 = 11;
/// The user-assembly getter that reads one class constant by name.
const CLASS_CONSTANT_GETTER: &str = "__elephc_eval_value_class_constant_get";

/// Emits the target-specific `__rt_constant` constant-resolution helper.
pub fn emit_rt_constant(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation of `__rt_constant`.
///
/// Frame (64 bytes): [0] name pointer, [8] name length, [16] `::` offset,
/// [24] message suffix pointer, [32] message suffix length, [48] saved fp/lr.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: constant ---");
    emitter.label_global("__rt_constant");

    // -- frame and query spill (slots reused for the owned message on a miss) --
    emitter.instruction("sub sp, sp, #64");                                     // reserve the helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // spill the constant name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // spill the constant name length

    // -- a `Class::NAME` name is a class constant: find the `::` --
    emitter.instruction("mov x9, #0");                                          // scan offset = 0
    emitter.instruction("sub x10, x1, #1");                                     // last offset a `::` can start at
    emitter.label("__rt_constant_scan");
    emitter.instruction("cmp x9, x10");                                         // scanned every candidate offset?
    emitter.instruction("b.ge __rt_constant_global");                           // no `::`: a global constant
    emitter.instruction("ldrb w11, [x0, x9]");                                  // load the byte at the scan offset
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_constant_scan_next");                        // keep scanning
    emitter.instruction("add x12, x9, #1");                                     // offset of the following byte
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the following byte
    emitter.instruction("cmp w11, #58");                                        // is it ':' too?
    emitter.instruction("b.eq __rt_constant_class");                            // `::` separates class and constant
    emitter.label("__rt_constant_scan_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan offset
    emitter.instruction("b __rt_constant_scan");                                // test the next offset

    // -- search the constant registry --
    emitter.label("__rt_constant_global");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the constant name pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the constant name length
    abi::emit_symbol_address(emitter, "x2", "_const_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_const_table_count", 0);
    emitter.instruction("mov x4, #40");                                         // constant entries are 40 bytes wide
    emitter.instruction("bl __rt_sorted_name_search");                          // locate the constant entry
    emitter.instruction("cbz x0, __rt_constant_miss");                          // a null entry means the constant is undefined

    // -- hit: reconstruct an owned boxed Mixed from the stored payload --
    emitter.instruction("mov x9, x0");                                          // keep the matched entry pointer
    emitter.instruction("ldr x0, [x9, #16]");                                   // load the boxed Mixed tag
    emitter.instruction("ldr x1, [x9, #24]");                                   // load the payload low word
    emitter.instruction("ldr x2, [x9, #32]");                                   // load the payload high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the stored scalar into an owned Mixed
    emitter.instruction("b __rt_constant_done");                                // return the boxed value

    // -- global miss: `Undefined constant "NAME"` --
    emitter.label("__rt_constant_miss");
    abi::emit_symbol_address(emitter, "x9", "_constant_undefined_msg_suffix");
    emitter.instruction("str x9, [sp, #24]");                                   // closing-quote suffix pointer
    emitter.instruction("mov x9, #1");                                          // closing-quote suffix byte length
    emitter.instruction("str x9, [sp, #32]");                                   // park the suffix length
    abi::emit_symbol_address(emitter, "x1", "_constant_undefined_msg_prefix");
    emitter.instruction(&format!("mov x2, #{}", MSG_PREFIX_LEN));               // message prefix byte length
    emitter.instruction("b __rt_constant_throw");                               // build and throw the \Error

    // -- class constant: read `Class::NAME` through the compiled getter --
    emitter.label("__rt_constant_class");
    emitter.instruction("str x9, [sp, #16]");                                   // keep the `::` offset (the class-name length)
    emitter.instruction("ldr x0, [sp, #0]");                                    // class-name pointer = the name pointer
    emitter.instruction("mov x1, x9");                                          // class-name length = the `::` offset
    emitter.instruction("add x2, x0, x9");                                      // point at the `::`
    emitter.instruction("add x2, x2, #2");                                      // constant-name pointer, past the `::`
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the whole name length
    emitter.instruction("sub x3, x3, x9");                                      // drop the class name
    emitter.instruction("sub x3, x3, #2");                                      // and the `::`: the constant-name length
    emitter.instruction("mov x4, xzr");                                         // no class scope: public constants only
    emitter.instruction("mov x5, xzr");                                         // empty scope name length
    let getter = emitter.target.extern_symbol(CLASS_CONSTANT_GETTER);
    abi::emit_call_label_long_range(emitter, &getter);
    emitter.instruction("cbnz x0, __rt_constant_done");                         // the getter boxed an owned value
    emitter.instruction("ldr x0, [sp, #0]");                                    // class-name pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // class-name length
    emitter.instruction("bl __rt_class_id_by_name");                            // does any class have that name?
    emitter.instruction("cmp x0, #0");                                          // a negative id names no class
    emitter.instruction("b.lt __rt_constant_class_missing");                    // report the missing class
    abi::emit_symbol_address(emitter, "x1", "_constant_undefined_msg_prefix");
    emitter.instruction("str x1, [sp, #24]");                                   // an empty suffix: any valid pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // no suffix after `C::X`
    emitter.instruction(&format!("mov x2, #{}", CLASS_CONSTANT_MSG_PREFIX_LEN)); // `Undefined constant ` without the quote
    emitter.instruction("b __rt_constant_throw");                               // build and throw the \Error
    emitter.label("__rt_constant_class_missing");
    emitter.instruction("ldr x9, [sp, #16]");                                   // the class-name length
    emitter.instruction("str x9, [sp, #8]");                                    // the message names the class only
    abi::emit_symbol_address(emitter, "x9", "_constant_class_msg_suffix");
    emitter.instruction("str x9, [sp, #24]");                                   // `" not found` suffix pointer
    emitter.instruction(&format!("mov x9, #{}", CLASS_MSG_SUFFIX_LEN));         // `" not found` byte length
    emitter.instruction("str x9, [sp, #32]");                                   // park the suffix length
    abi::emit_symbol_address(emitter, "x1", "_constant_class_msg_prefix");
    emitter.instruction(&format!("mov x2, #{}", CLASS_MSG_PREFIX_LEN));         // `Class "` byte length

    // -- throw: prefix (x1/x2) + name ([sp,0]/[sp,8]) + suffix ([sp,24]/[sp,32]) --
    emitter.label("__rt_constant_throw");
    emitter.instruction("ldr x3, [sp, #0]");                                    // reload the name pointer
    emitter.instruction("ldr x4, [sp, #8]");                                    // reload the name length
    emitter.instruction("bl __rt_concat");                                      // prefix concatenated with the name
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the suffix pointer
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload the suffix length
    emitter.instruction("bl __rt_concat");                                      // append the suffix to the message
    emitter.instruction("bl __rt_str_persist");                                 // own a heap copy of the message bytes
    emitter.instruction("str x1, [sp, #0]");                                    // park the owned message pointer
    emitter.instruction("str x2, [sp, #8]");                                    // park the owned message length
    // The standard Throwable payload, as every runtime-raised \Error is built: a 32-byte block
    // left the object without a handle, a creation line or a `previous` slot, and the unzeroed
    // bytes read as `previous` were released when a later `catch` rebound the variable.
    crate::codegen_support::throwable_layout::emit_allocate(emitter, crate::codegen_support::throwable_layout::PAYLOAD_SIZE);
    emitter.instruction("mov x9, #6");                                          // heap kind 6 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a runtime object
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the \Error to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "x9", "_constant_error_class_id", 0);
    emitter.instruction("str x9, [x0]");                                        // store the \Error class id at the object header
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the owned message pointer
    emitter.instruction("str x9, [x0, #8]");                                    // store the exception message pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the owned message length
    emitter.instruction("str x9, [x0, #16]");                                   // store the exception message length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // previous = null
    abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // publish the active exception object
    emitter.instruction("b __rt_throw_current");                                // enter the standard exception unwinder

    emitter.label("__rt_constant_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in x0
}

/// Emits the x86_64 implementation of `__rt_constant`.
///
/// Frame: [rbp-8] name pointer, [rbp-16] name length, [rbp-24] `::` offset,
/// [rbp-32] message suffix pointer, [rbp-40] message suffix length.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: constant ---");
    emitter.label_global("__rt_constant");

    // -- frame and query spill (slots reused for the owned message on a miss) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for the query and message
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // spill the constant name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // spill the constant name length

    // -- a `Class::NAME` name is a class constant: find the `::` --
    emitter.instruction("xor r9, r9");                                          // scan offset = 0
    emitter.instruction("mov r10, rsi");                                        // the name length
    emitter.instruction("sub r10, 1");                                          // last offset a `::` can start at
    emitter.label("__rt_constant_scan_x86");
    emitter.instruction("cmp r9, r10");                                         // scanned every candidate offset?
    emitter.instruction("jge __rt_constant_global_x86");                        // no `::`: a global constant
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r9]");                     // load the byte at the scan offset
    emitter.instruction("cmp r11d, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_constant_scan_next_x86");                     // keep scanning
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r9 + 1]");                 // load the following byte
    emitter.instruction("cmp r11d, 58");                                        // is it ':' too?
    emitter.instruction("je __rt_constant_class_x86");                          // `::` separates class and constant
    emitter.label("__rt_constant_scan_next_x86");
    emitter.instruction("add r9, 1");                                           // advance the scan offset
    emitter.instruction("jmp __rt_constant_scan_x86");                          // test the next offset

    // -- search the constant registry --
    emitter.label("__rt_constant_global_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the constant name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the constant name length
    abi::emit_symbol_address(emitter, "rdx", "_const_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_const_table_count", 0);
    emitter.instruction("mov r8, 40");                                          // constant entries are 40 bytes wide
    emitter.instruction("call __rt_sorted_name_search");                        // locate the constant entry
    emitter.instruction("test rax, rax");                                       // did the search find a matching entry?
    emitter.instruction("je __rt_constant_miss");                               // a null entry means the constant is undefined

    // -- hit: reconstruct an owned boxed Mixed from the stored payload --
    emitter.instruction("mov r9, rax");                                         // keep the matched entry pointer
    emitter.instruction("mov rax, QWORD PTR [r9 + 16]");                        // load the boxed Mixed tag
    emitter.instruction("mov rdi, QWORD PTR [r9 + 24]");                        // load the payload low word
    emitter.instruction("mov rsi, QWORD PTR [r9 + 32]");                        // load the payload high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the stored scalar into an owned Mixed
    emitter.instruction("jmp __rt_constant_done");                              // return the boxed value

    // -- global miss: `Undefined constant "NAME"` --
    emitter.label("__rt_constant_miss");
    abi::emit_symbol_address(emitter, "r10", "_constant_undefined_msg_suffix");
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // closing-quote suffix pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 1");                         // closing-quote suffix byte length
    abi::emit_symbol_address(emitter, "rax", "_constant_undefined_msg_prefix");
    emitter.instruction(&format!("mov rdx, {}", MSG_PREFIX_LEN));               // message prefix byte length
    emitter.instruction("jmp __rt_constant_throw_x86");                         // build and throw the \Error

    // -- class constant: read `Class::NAME` through the compiled getter --
    emitter.label("__rt_constant_class_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // keep the `::` offset (the class-name length)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // class-name pointer = the name pointer
    emitter.instruction("mov rsi, r9");                                         // class-name length = the `::` offset
    emitter.instruction("lea rdx, [rdi + r9 + 2]");                             // constant-name pointer, past the `::`
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the whole name length
    emitter.instruction("sub rcx, r9");                                         // drop the class name
    emitter.instruction("sub rcx, 2");                                          // and the `::`: the constant-name length
    emitter.instruction("xor r8, r8");                                          // no class scope: public constants only
    emitter.instruction("xor r9, r9");                                          // empty scope name length
    let getter = emitter.target.extern_symbol(CLASS_CONSTANT_GETTER);
    abi::emit_call_label_long_range(emitter, &getter);
    emitter.instruction("test rax, rax");                                       // did the getter box a value?
    emitter.instruction("jne __rt_constant_done");                              // return the owned boxed value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // class-name length
    emitter.instruction("call __rt_class_id_by_name");                          // does any class have that name?
    emitter.instruction("test rax, rax");                                       // a negative id names no class
    emitter.instruction("js __rt_constant_class_missing_x86");                  // report the missing class
    abi::emit_symbol_address(emitter, "rax", "_constant_undefined_msg_prefix");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // an empty suffix: any valid pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // no suffix after `C::X`
    emitter.instruction(&format!("mov rdx, {}", CLASS_CONSTANT_MSG_PREFIX_LEN)); // `Undefined constant ` without the quote
    emitter.instruction("jmp __rt_constant_throw_x86");                         // build and throw the \Error
    emitter.label("__rt_constant_class_missing_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // the class-name length
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // the message names the class only
    abi::emit_symbol_address(emitter, "r10", "_constant_class_msg_suffix");
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // `" not found` suffix pointer
    emitter.instruction(&format!("mov QWORD PTR [rbp - 40], {}", CLASS_MSG_SUFFIX_LEN)); // `" not found` byte length
    abi::emit_symbol_address(emitter, "rax", "_constant_class_msg_prefix");
    emitter.instruction(&format!("mov rdx, {}", CLASS_MSG_PREFIX_LEN));         // `Class "` byte length

    // -- throw: prefix (rax/rdx) + name ([rbp-8]/[rbp-16]) + suffix ([rbp-32]/[rbp-40]) --
    emitter.label("__rt_constant_throw_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the name length
    emitter.instruction("call __rt_concat");                                    // prefix concatenated with the name
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the suffix pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the suffix length
    emitter.instruction("call __rt_concat");                                    // append the suffix to the message
    emitter.instruction("mov rdi, rax");                                        // move the message pointer into the persist argument
    emitter.instruction("call __rt_str_persist");                               // own a heap copy of the message bytes
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // park the owned message pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // park the owned message length
    crate::codegen_support::throwable_layout::emit_allocate(emitter, crate::codegen_support::throwable_layout::PAYLOAD_SIZE);
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(6)
    ));                                                                         // x86_64 heap-kind word: heap marker + object kind 6
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocation as a runtime object
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");                // bind the \Error to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "r10", "_constant_error_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the \Error class id at the object header
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owned message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the exception message pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the owned message length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the exception message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // previous = null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before throwing
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard exception unwinder

    emitter.label("__rt_constant_done");
    emitter.instruction("mov rsp, rbp");                                        // discard the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in rax
}
