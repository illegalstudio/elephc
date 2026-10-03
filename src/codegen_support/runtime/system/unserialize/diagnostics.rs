//! Purpose:
//! Emits target-aware diagnostic and object-string helpers used by `unserialize()`.
//!
//! Called from:
//! - `super::emit_unserialize()` while assembling the unserialize runtime surface.
//!
//! Key details:
//! - Helpers preserve PHP error types and unwind the active unserialize context exactly once.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::{
    UNSER_OBJECT_STRING_ERROR_PREFIX, UNSER_OBJECT_STRING_ERROR_SUFFIX,
    UNSER_TYPE_GIVEN_SUFFIX,
};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

/// Emits the non-returning dynamic unserialize TypeError helper.
///
/// Callers provide a runtime tag/payload plus a message prefix. The helper closes
/// the active public unserialize context exactly once, resolves PHP's actual type
/// name (including declared object class names), persists the composed message,
/// and propagates a standard catchable `TypeError`.
pub(super) fn emit_unserialize_type_error_helper(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: catchable unserialize TypeError ---");
            emitter.label_global("__rt_unser_throw_type_error");
            emitter.instruction("sub sp, sp, #80");                             // reserve tag/payload/prefix/type/message state
            emitter.instruction("stp x29, x30, [sp, #64]");                     // preserve the caller frame and return address
            emitter.instruction("add x29, sp, #64");                            // establish a stable error-construction frame
            emitter.instruction("stp x0, x1, [sp]");                            // save runtime tag and payload
            emitter.instruction("stp x2, x3, [sp, #16]");                       // save diagnostic prefix pointer and length
            emitter.instruction("mov x0, #0");                                  // end cleanup ignores the placeholder parse result
            emitter.instruction("bl __rt_unserialize_end");                     // close this opened context exactly once
            emitter.instruction("ldr x9, [sp]");                                // reload rejected runtime tag
            emitter.instruction("cmp x9, #0");                                  // tag 0 = PHP int
            emitter.instruction("b.eq __rt_unser_type_int");                    // spell the rejected value as int
            emitter.instruction("cmp x9, #1");                                  // tag 1 = PHP string
            emitter.instruction("b.eq __rt_unser_type_string");                 // spell the rejected value as string
            emitter.instruction("cmp x9, #2");                                  // tag 2 = PHP float
            emitter.instruction("b.eq __rt_unser_type_float");                  // spell the rejected value as float
            emitter.instruction("cmp x9, #3");                                  // tag 3 = PHP bool
            emitter.instruction("b.eq __rt_unser_type_bool");                   // spell the rejected value as bool
            emitter.instruction("cmp x9, #4");                                  // tag 4 = indexed PHP array
            emitter.instruction("b.eq __rt_unser_type_array");                  // spell the rejected value as array
            emitter.instruction("cmp x9, #5");                                  // tag 5 = associative PHP array
            emitter.instruction("b.eq __rt_unser_type_array");                  // both array shapes spell as array
            emitter.instruction("cmp x9, #6");                                  // tag 6 = PHP object
            emitter.instruction("b.eq __rt_unser_type_object");                 // objects resolve their concrete class name
            emitter.instruction("cmp x9, #8");                                  // tag 8 = PHP null
            emitter.instruction("b.eq __rt_unser_type_null");                   // spell the rejected value as null
            emitter.instruction("cmp x9, #9");                                  // tag 9 = PHP resource
            emitter.instruction("b.eq __rt_unser_type_resource");               // spell the rejected value as resource
            crate::codegen_support::abi::emit_symbol_address(emitter, "x3", "_unser_type_unknown");
            emitter.instruction("mov x4, #7");                                  // byte length of unknown
            emitter.instruction("b __rt_unser_type_ready");                     // unrecognized tags join with the unknown spelling
            for (label, symbol, len) in [
                ("__rt_unser_type_int", "_unser_type_int", 3),
                ("__rt_unser_type_string", "_unser_type_string", 6),
                ("__rt_unser_type_float", "_unser_type_float", 5),
                ("__rt_unser_type_bool", "_unser_type_bool", 4),
                ("__rt_unser_type_array", "_unser_type_array", 5),
                ("__rt_unser_type_null", "_unser_type_null", 4),
                ("__rt_unser_type_resource", "_unser_type_resource", 8),
            ] {
                emitter.label(label);
                crate::codegen_support::abi::emit_symbol_address(emitter, "x3", symbol);
                emitter.instruction(&format!("mov x4, #{}", len));              // materialize the selected PHP type-name length
                emitter.instruction("b __rt_unser_type_ready");                 // join dynamic message construction
            }
            emitter.label("__rt_unser_type_object");
            emitter.instruction("ldr x9, [sp, #8]");                            // rejected object payload
            emitter.instruction("cbz x9, __rt_unser_type_object_generic");      // null payload has no class metadata
            emitter.instruction("ldr x10, [x9]");                               // object class id
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x11", "_class_name_count", 0);
            emitter.instruction("cmp x10, x11");                                // class id within the dense name table?
            emitter.instruction("b.hs __rt_unser_type_object_generic");         // out-of-range ids fall back to the object spelling
            crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
            emitter.instruction("add x11, x11, x10, lsl #4");                   // select the (ptr,len) class-name row
            emitter.instruction("ldp x3, x4, [x11]");                           // use the concrete PHP class name
            emitter.instruction("cbnz x4, __rt_unser_type_ready");              // empty metadata falls back to object
            emitter.label("__rt_unser_type_object_generic");
            crate::codegen_support::abi::emit_symbol_address(emitter, "x3", "_unser_type_object");
            emitter.instruction("mov x4, #6");                                  // byte length of object
            emitter.label("__rt_unser_type_ready");
            emitter.instruction("ldp x1, x2, [sp, #16]");                       // diagnostic prefix string
            emitter.instruction("bl __rt_concat");                              // append the resolved PHP type name
            crate::codegen_support::abi::emit_symbol_address(emitter, "x3", "_unser_type_given_suffix");
            emitter.instruction(&format!("mov x4, #{}", UNSER_TYPE_GIVEN_SUFFIX.len())); // suffix byte length
            emitter.instruction("bl __rt_concat");                              // append PHP's ` given` suffix
            emitter.instruction("bl __rt_str_persist");                         // give the Throwable stable message ownership
            emitter.instruction("stp x1, x2, [sp, #48]");                       // preserve message pointer/length across allocation
            emitter.instruction("mov x0, #56");                                 // request the canonical Throwable payload size
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the TypeError object payload
            emitter.instruction("mov x9, #6");                                  // heap kind 6 identifies a throwable object
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocation as a runtime object
            emitter.instruction("bl __rt_object_handle_acquire");               // bind the TypeError to its PHP object handle
            crate::codegen_support::abi::emit_load_symbol_to_reg(
                emitter,
                "x9",
                "_spl_type_error_class_id",
                0,
            );
            emitter.instruction("str x9, [x0]");                                // store the built-in TypeError class id
            emitter.instruction("ldr x9, [sp, #48]");                           // recover the persisted TypeError message pointer
            emitter.instruction("str x9, [x0, #8]");                            // store the dynamic TypeError message pointer
            emitter.instruction("ldr x9, [sp, #56]");                           // recover the dynamic message byte length
            emitter.instruction("str x9, [x0, #16]");                           // store the TypeError message byte length
            emitter.instruction("str xzr, [x0, #24]");                          // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(
                emitter, "x0",
            );
            emitter.instruction("str xzr, [x0, #40]");                          // previous Throwable defaults to null
            crate::codegen_support::abi::emit_store_reg_to_symbol(
                emitter,
                "x0",
                "_exc_value",
                0,
            );
            emitter.instruction("ldp x29, x30, [sp, #64]");                     // restore the frame before unwinding
            emitter.instruction("add sp, sp, #80");                             // release error-construction state
            emitter.instruction("b __rt_throw_current");                        // propagate through the standard catchable exception path
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: catchable unserialize TypeError ---");
            emitter.label_global("__rt_unser_throw_type_error");
            emitter.instruction("push rbp");                                    // preserve the caller frame while building the message
            emitter.instruction("mov rbp, rsp");                                // establish an aligned exception-construction frame
            emitter.instruction("sub rsp, 64");                                 // reserve tag/payload/prefix/type/message state
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // save runtime tag
            emitter.instruction("mov QWORD PTR [rbp - 16], rdi");               // save runtime payload
            emitter.instruction("mov QWORD PTR [rbp - 24], rsi");               // save diagnostic prefix pointer
            emitter.instruction("mov QWORD PTR [rbp - 32], rdx");               // save diagnostic prefix length
            emitter.instruction("xor eax, eax");                                // end cleanup ignores the placeholder result
            emitter.instruction("call __rt_unserialize_end");                   // close this opened context exactly once
            emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                 // rejected runtime tag
            emitter.instruction("cmp r8, 0");                                   // tag 0 = PHP int
            emitter.instruction("je __rt_unser_type_int_x");                    // spell the rejected value as int
            emitter.instruction("cmp r8, 1");                                   // tag 1 = PHP string
            emitter.instruction("je __rt_unser_type_string_x");                 // spell the rejected value as string
            emitter.instruction("cmp r8, 2");                                   // tag 2 = PHP float
            emitter.instruction("je __rt_unser_type_float_x");                  // spell the rejected value as float
            emitter.instruction("cmp r8, 3");                                   // tag 3 = PHP bool
            emitter.instruction("je __rt_unser_type_bool_x");                   // spell the rejected value as bool
            emitter.instruction("cmp r8, 4");                                   // tag 4 = indexed PHP array
            emitter.instruction("je __rt_unser_type_array_x");                  // spell the rejected value as array
            emitter.instruction("cmp r8, 5");                                   // tag 5 = associative PHP array
            emitter.instruction("je __rt_unser_type_array_x");                  // both array shapes spell as array
            emitter.instruction("cmp r8, 6");                                   // tag 6 = PHP object
            emitter.instruction("je __rt_unser_type_object_x");                 // objects resolve their concrete class name
            emitter.instruction("cmp r8, 8");                                   // tag 8 = PHP null
            emitter.instruction("je __rt_unser_type_null_x");                   // spell the rejected value as null
            emitter.instruction("cmp r8, 9");                                   // tag 9 = PHP resource
            emitter.instruction("je __rt_unser_type_resource_x");               // spell the rejected value as resource
            emitter.instruction("lea rdi, [rip + _unser_type_unknown]");        // unknown runtime tag spelling
            emitter.instruction("mov rsi, 7");                                  // byte length of unknown
            emitter.instruction("jmp __rt_unser_type_ready_x");                 // unrecognized tags join with the unknown spelling
            for (label, symbol, len) in [
                ("__rt_unser_type_int_x", "_unser_type_int", 3),
                ("__rt_unser_type_string_x", "_unser_type_string", 6),
                ("__rt_unser_type_float_x", "_unser_type_float", 5),
                ("__rt_unser_type_bool_x", "_unser_type_bool", 4),
                ("__rt_unser_type_array_x", "_unser_type_array", 5),
                ("__rt_unser_type_null_x", "_unser_type_null", 4),
                ("__rt_unser_type_resource_x", "_unser_type_resource", 8),
            ] {
                emitter.label(label);
                emitter.instruction(&format!("lea rdi, [rip + {}]", symbol));   // selected PHP type-name pointer
                emitter.instruction(&format!("mov rsi, {}", len));              // selected PHP type-name length
                emitter.instruction("jmp __rt_unser_type_ready_x");             // join dynamic message construction
            }
            emitter.label("__rt_unser_type_object_x");
            emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                // rejected object payload
            emitter.instruction("test r8, r8");                                 // null payload has no class metadata
            emitter.instruction("jz __rt_unser_type_object_generic_x");         // fall back to the generic object spelling
            emitter.instruction("mov r9, QWORD PTR [r8]");                      // object class id
            emitter.instruction("mov r10, QWORD PTR [rip + _class_name_count]"); // dense class-name table bound
            emitter.instruction("cmp r9, r10");                                 // class id within the dense name table?
            emitter.instruction("jae __rt_unser_type_object_generic_x");        // out-of-range ids fall back to the object spelling
            emitter.instruction("lea r10, [rip + _class_name_entries]");        // class-name metadata base
            emitter.instruction("shl r9, 4");                                   // one pointer/length pair per class id
            emitter.instruction("add r10, r9");                                 // select the (ptr,len) class-name row
            emitter.instruction("mov rdi, QWORD PTR [r10]");                    // concrete class-name pointer
            emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                // concrete class-name length
            emitter.instruction("test rsi, rsi");                               // empty metadata falls back to object
            emitter.instruction("jnz __rt_unser_type_ready_x");                 // use the concrete PHP class name
            emitter.label("__rt_unser_type_object_generic_x");
            emitter.instruction("lea rdi, [rip + _unser_type_object]");         // generic fallback type name
            emitter.instruction("mov rsi, 6");                                  // byte length of object
            emitter.label("__rt_unser_type_ready_x");
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // diagnostic prefix pointer
            emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");               // diagnostic prefix length
            emitter.instruction("call __rt_concat");                            // append the resolved PHP type name
            emitter.instruction("lea rdi, [rip + _unser_type_given_suffix]");   // PHP diagnostic suffix
            emitter.instruction(&format!("mov rsi, {}", UNSER_TYPE_GIVEN_SUFFIX.len())); // suffix byte length
            emitter.instruction("call __rt_concat");                            // append ` given`
            emitter.instruction("call __rt_str_persist");                       // give the Throwable stable message ownership
            emitter.instruction("mov QWORD PTR [rbp - 40], rax");               // save persisted message pointer
            emitter.instruction("mov QWORD PTR [rbp - 48], rdx");               // save persisted message length
            emitter.instruction("mov rax, 56");                                 // request the canonical Throwable payload size
            emitter.instruction("call __rt_heap_alloc");                        // allocate the TypeError object payload
            emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // materialize the throwable heap-kind marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocation as a runtime object
            emitter.instruction("call __rt_object_handle_acquire");             // bind the TypeError to its PHP object handle
            crate::codegen_support::abi::emit_load_symbol_to_reg(
                emitter,
                "r10",
                "_spl_type_error_class_id",
                0,
            );
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the built-in TypeError class id
            emitter.instruction("mov r10, QWORD PTR [rbp - 40]");               // persisted TypeError message pointer
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the dynamic TypeError message pointer
            emitter.instruction("mov r10, QWORD PTR [rbp - 48]");               // dynamic TypeError message byte length
            emitter.instruction("mov QWORD PTR [rax + 16], r10");               // store the TypeError message byte length
            emitter.instruction("mov QWORD PTR [rax + 24], 0");                 // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(
                emitter, "rax",
            );
            emitter.instruction("mov QWORD PTR [rax + 40], 0");                 // previous Throwable defaults to null
            crate::codegen_support::abi::emit_store_reg_to_symbol(
                emitter,
                "rax",
                "_exc_value",
                0,
            );
            emitter.instruction("mov rsp, rbp");                                // release the construction frame before unwinding
            emitter.instruction("pop rbp");                                     // restore the caller frame before unwinding
            emitter.instruction("jmp __rt_throw_current");                      // propagate through the standard catchable exception path
        }
    }
}

/// Emits the catchable PHP Error used when an allowed-class entry object has no `__toString()`.
///
/// The concrete class-name pair is resolved while the object is still borrowed from
/// the live options container. The helper then closes the unserialize context once,
/// composes the stable PHP message, and throws a standard `Error` object.
pub(super) fn emit_unserialize_object_string_error_helper(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: unserialize object string-conversion Error ---");
            emitter.label_global("__rt_unser_throw_object_string_error");
            emitter.instruction("sub sp, sp, #64");                             // reserve class-name and message state
            emitter.instruction("stp x29, x30, [sp, #48]");                     // preserve the caller frame and return address
            emitter.instruction("add x29, sp, #48");                            // establish an aligned Error-construction frame
            emitter.instruction("ldr x13, [x0]");                               // keep class id outside the symbol helper's x9 scratch register
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_class_name_count", 0);
            emitter.instruction("cmp x13, x10");                                // is the class id within the dense name table?
            emitter.instruction("b.hs __rt_unser_object_string_name_fallback"); // malformed ids use the generic object spelling
            crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
            emitter.instruction("add x10, x10, x13, lsl #4");                   // select the static class-name row
            emitter.instruction("ldp x11, x12, [x10]");                         // borrow the static class-name pointer and length
            emitter.instruction("cbnz x12, __rt_unser_object_string_name_ready"); // non-empty metadata is safe after context cleanup
            emitter.label("__rt_unser_object_string_name_fallback");
            crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_unser_type_object");
            emitter.instruction("mov x12, #6");                                 // fallback name length for object
            emitter.label("__rt_unser_object_string_name_ready");
            emitter.instruction("stp x11, x12, [sp]");                          // keep only static name metadata across end
            emitter.instruction("mov x0, #0");                                  // cleanup ignores the placeholder parse result
            emitter.instruction("bl __rt_unserialize_end");                     // close this opened context exactly once
            crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_unser_object_string_error_prefix");
            emitter.instruction(&format!("mov x2, #{}", UNSER_OBJECT_STRING_ERROR_PREFIX.len())); // prefix byte length
            emitter.instruction("ldp x3, x4, [sp]");                            // append the resolved class name
            emitter.instruction("bl __rt_concat");                              // build `Object of class <Class>`
            crate::codegen_support::abi::emit_symbol_address(emitter, "x3", "_unser_object_string_error_suffix");
            emitter.instruction(&format!("mov x4, #{}", UNSER_OBJECT_STRING_ERROR_SUFFIX.len())); // suffix byte length
            emitter.instruction("bl __rt_concat");                              // append PHP's conversion failure suffix
            emitter.instruction("bl __rt_str_persist");                         // give the Error stable message ownership
            emitter.instruction("stp x1, x2, [sp, #16]");                       // preserve message pair across object allocation
            emitter.instruction("mov x0, #56");                                 // request the canonical Throwable payload size
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the Error object payload
            emitter.instruction("mov x9, #6");                                  // heap kind 6 identifies a throwable object
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocation as a runtime object
            emitter.instruction("bl __rt_object_handle_acquire");               // bind the Error to its PHP object handle
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x9", "_spl_error_class_id", 0);
            emitter.instruction("str x9, [x0]");                                // store the built-in Error class id
            emitter.instruction("ldp x10, x11, [sp, #16]");                     // recover the persisted message pair
            emitter.instruction("stp x10, x11, [x0, #8]");                      // store Error message pointer and length
            emitter.instruction("str xzr, [x0, #24]");                          // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "x0");
            emitter.instruction("str xzr, [x0, #40]");                          // previous Throwable defaults to null
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
            emitter.instruction("ldp x29, x30, [sp, #48]");                     // restore the caller frame before unwinding
            emitter.instruction("add sp, sp, #64");                             // release Error-construction state
            emitter.instruction("b __rt_throw_current");                        // propagate through the catchable Throwable path
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: unserialize object string-conversion Error ---");
            emitter.label_global("__rt_unser_throw_object_string_error");
            emitter.instruction("push rbp");                                    // preserve the caller frame while constructing the Error
            emitter.instruction("mov rbp, rsp");                                // establish an aligned construction frame
            emitter.instruction("sub rsp, 48");                                 // reserve class-name and message state
            emitter.instruction("mov r8, QWORD PTR [rax]");                     // resolve class id while the borrowed object is live
            emitter.instruction("test r8, r8");                                 // reject negative synthetic class ids before table indexing
            emitter.instruction("js __rt_unser_object_string_name_fallback_x"); // synthetic ids use the generic object spelling
            emitter.instruction("cmp r8, QWORD PTR [rip + _class_name_count]"); // is the class id within the dense name table?
            emitter.instruction("jae __rt_unser_object_string_name_fallback_x"); // malformed ids use the generic object spelling
            emitter.instruction("lea r9, [rip + _class_name_entries]");         // class-name metadata base
            emitter.instruction("shl r8, 4");                                   // scale id by the pointer/length row width
            emitter.instruction("add r9, r8");                                  // select the static class-name row
            emitter.instruction("mov r10, QWORD PTR [r9]");                     // borrow static class-name pointer
            emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                 // borrow static class-name length
            emitter.instruction("test r11, r11");                               // empty metadata falls back to object
            emitter.instruction("jnz __rt_unser_object_string_name_ready_x");   // non-empty metadata is safe after context cleanup
            emitter.label("__rt_unser_object_string_name_fallback_x");
            emitter.instruction("lea r10, [rip + _unser_type_object]");         // generic object class spelling
            emitter.instruction("mov r11, 6");                                  // fallback name byte length
            emitter.label("__rt_unser_object_string_name_ready_x");
            emitter.instruction("mov QWORD PTR [rbp - 8], r10");                // keep only static name metadata across end
            emitter.instruction("mov QWORD PTR [rbp - 16], r11");               // preserve static class-name length
            emitter.instruction("xor eax, eax");                                // cleanup ignores the placeholder parse result
            emitter.instruction("call __rt_unserialize_end");                   // close this opened context exactly once
            emitter.instruction("lea rax, [rip + _unser_object_string_error_prefix]"); // conversion Error prefix
            emitter.instruction(&format!("mov rdx, {}", UNSER_OBJECT_STRING_ERROR_PREFIX.len())); // prefix byte length
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // resolved class-name pointer
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // resolved class-name length
            emitter.instruction("call __rt_concat");                            // build `Object of class <Class>`
            emitter.instruction("lea rdi, [rip + _unser_object_string_error_suffix]"); // conversion Error suffix
            emitter.instruction(&format!("mov rsi, {}", UNSER_OBJECT_STRING_ERROR_SUFFIX.len())); // suffix byte length
            emitter.instruction("call __rt_concat");                            // append PHP's conversion failure suffix
            emitter.instruction("call __rt_str_persist");                       // give the Error stable message ownership
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // preserve persisted message pointer
            emitter.instruction("mov QWORD PTR [rbp - 32], rdx");               // preserve persisted message length
            emitter.instruction("mov rax, 56");                                 // request the canonical Throwable payload size
            emitter.instruction("call __rt_heap_alloc");                        // allocate the Error object payload
            emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // throwable heap-kind marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocation as a runtime object
            emitter.instruction("call __rt_object_handle_acquire");             // bind the Error to its PHP object handle
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_error_class_id", 0);
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the built-in Error class id
            emitter.instruction("mov r10, QWORD PTR [rbp - 24]");               // recover persisted Error message pointer
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store message pointer
            emitter.instruction("mov r10, QWORD PTR [rbp - 32]");               // recover persisted Error message length
            emitter.instruction("mov QWORD PTR [rax + 16], r10");               // store message byte length
            emitter.instruction("mov QWORD PTR [rax + 24], 0");                 // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "rax");
            emitter.instruction("mov QWORD PTR [rax + 40], 0");                 // previous Throwable defaults to null
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
            emitter.instruction("leave");                                       // restore the caller frame before unwinding
            emitter.instruction("jmp __rt_throw_current");                      // propagate through the catchable Throwable path
        }
    }
}

/// Emits a dynamic, exception-safe `__toString()` invocation for allowed-class entries.
///
/// Input is a borrowed object (`x0`/`rax`). A class without `__toString()` returns
/// a null string pointer; otherwise the method's string pair is returned unchanged.
/// A Throwable escaping the user method is caught at this native boundary so the
/// active unserialize context is closed exactly once before propagation resumes.
pub(super) fn emit_unserialize_object_to_string_helper(emitter: &mut Emitter) {
    let boundary_bytes = TRY_HANDLER_SLOT_SIZE + 32;
    match emitter.target.arch {
        Arch::AArch64 => {
            let frame_link_offset = boundary_bytes - 16;
            let object_offset = TRY_HANDLER_SLOT_SIZE;
            let string_len_offset = object_offset + 8;
            emitter.blank();
            emitter.comment("--- runtime: bounded allowed-class object __toString ---");
            emitter.label_global("__rt_unser_allowed_object_to_string");
            emitter.instruction(&format!("sub sp, sp, #{}", boundary_bytes));   // reserve a complete handler record plus result spills
            emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_link_offset)); // preserve the caller frame and return address
            emitter.instruction(&format!("add x29, sp, #{}", frame_link_offset)); // establish the protected invocation frame
            emitter.instruction(&format!("str x0, [sp, #{}]", object_offset));  // preserve the borrowed receiver across setjmp
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction("str x10, [sp]");                               // handler.next = previous exception-handler head
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
            emitter.instruction("str x10, [sp, #8]");                           // preserve the activation frame surviving this boundary
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!("str x10, [sp, #{}]", TRY_HANDLER_DIAG_DEPTH_OFFSET)); // snapshot diagnostic suppression across longjmp
            emitter.instruction("mov x10, sp");                                 // compute this invocation's handler record address
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!("add x0, sp, #{}", TRY_HANDLER_JMP_BUF_OFFSET)); // pass this boundary's opaque jmp_buf to setjmp
            emitter.bl_c("setjmp"); // catch Throwable control flow escaping __toString
            emitter.instruction("cbnz x0, __rt_unser_allowed_tostring_throw");  // longjmp resumes here for exact context cleanup
            emitter.instruction(&format!("ldr x0, [sp, #{}]", object_offset));  // reload borrowed receiver after setjmp
            emitter.instruction("ldr x11, [x0]");                               // keep class id outside the symbol helper's x9 scratch register
            emitter.instruction("tbnz x11, #63, __rt_unser_allowed_tostring_missing"); // synthetic negative ids cannot index metadata
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_class_tostring_count", 0);
            emitter.instruction("cmp x11, x10");                                // is the id within the dense __toString table?
            emitter.instruction("b.hs __rt_unser_allowed_tostring_missing");    // out-of-range classes have no callable conversion
            crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_class_tostring_ptrs");
            emitter.instruction("ldr x10, [x10, x11, lsl #3]");                 // resolve the concrete or inherited method symbol
            emitter.instruction("cbz x10, __rt_unser_allowed_tostring_missing"); // no __toString means PHP conversion Error
            emitter.instruction("blr x10");                                     // call __toString with borrowed receiver in x0
            emitter.instruction(&format!("stp x1, x2, [sp, #{}]", object_offset)); // preserve returned string pair while popping boundary
            emitter.instruction("b __rt_unser_allowed_tostring_finish");        // share successful boundary teardown
            emitter.label("__rt_unser_allowed_tostring_missing");
            emitter.instruction(&format!("str xzr, [sp, #{}]", object_offset)); // null pointer reports a missing conversion method
            emitter.instruction(&format!("str xzr, [sp, #{}]", string_len_offset)); // missing conversion has no string length
            emitter.label("__rt_unser_allowed_tostring_finish");
            emitter.instruction("ldr x10, [sp]");                               // reload the previous exception-handler head
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!("ldr x10, [sp, #{}]", TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression after the call
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!("ldp x1, x2, [sp, #{}]", object_offset)); // recover the conversion string pair
            emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_link_offset)); // restore caller frame and return address
            emitter.instruction(&format!("add sp, sp, #{}", boundary_bytes));   // release the protected invocation frame
            emitter.instruction("ret");                                         // return the string pair or a null pointer
            emitter.label("__rt_unser_allowed_tostring_throw");
            emitter.instruction("ldr x10, [sp]");                               // reload the handler preceding this internal boundary
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!("ldr x10, [sp, #{}]", TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression skipped by longjmp
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
            emitter.instruction("mov x0, #0");                                  // cleanup ignores the placeholder result on throw
            emitter.instruction("bl __rt_unserialize_end");                     // close this opened context exactly once
            emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_link_offset)); // restore caller frame before rethrow
            emitter.instruction(&format!("add sp, sp, #{}", boundary_bytes));   // discard the protected invocation frame
            emitter.instruction("b __rt_throw_current");                        // resume propagation at the caller's handler
        }
        Arch::X86_64 => {
            let previous_handler_offset = boundary_bytes;
            let survivor_offset = previous_handler_offset - 8;
            emitter.blank();
            emitter.comment("--- runtime: bounded allowed-class object __toString ---");
            emitter.label_global("__rt_unser_allowed_object_to_string");
            emitter.instruction("push rbp");                                    // preserve caller frame across the exception boundary
            emitter.instruction("mov rbp, rsp");                                // establish a stable base for the handler record
            emitter.instruction(&format!("sub rsp, {}", boundary_bytes));       // reserve handler record plus receiver/result spills
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // preserve borrowed receiver across setjmp
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", previous_handler_offset)); // handler.next = previous head
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", survivor_offset)); // activation frame surviving this boundary
            crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", boundary_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // snapshot diagnostic suppression
            emitter.instruction(&format!("lea r10, [rbp - {}]", previous_handler_offset)); // compute this handler record address
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!("lea rdi, [rbp - {}]", boundary_bytes - TRY_HANDLER_JMP_BUF_OFFSET)); // pass opaque jmp_buf to setjmp
            emitter.bl_c("setjmp"); // catch Throwable control flow escaping __toString
            emitter.instruction("test eax, eax");                               // did control return through longjmp?
            emitter.instruction("jnz __rt_unser_allowed_tostring_throw_x");     // clean runtime state before propagating
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // reload borrowed receiver after setjmp
            emitter.instruction("mov r8, QWORD PTR [rdi]");                     // runtime class id
            emitter.instruction("test r8, r8");                                 // reject negative synthetic class ids
            emitter.instruction("js __rt_unser_allowed_tostring_missing_x");    // synthetic ids cannot index metadata
            emitter.instruction("cmp r8, QWORD PTR [rip + _class_tostring_count]"); // is id within dense method table?
            emitter.instruction("jae __rt_unser_allowed_tostring_missing_x");   // out-of-range classes have no conversion method
            emitter.instruction("lea r10, [rip + _class_tostring_ptrs]");       // dense __toString function-pointer table
            emitter.instruction("mov r10, QWORD PTR [r10 + r8 * 8]");           // resolve concrete or inherited method symbol
            emitter.instruction("test r10, r10");                               // does the class expose __toString?
            emitter.instruction("jz __rt_unser_allowed_tostring_missing_x");    // no method means PHP conversion Error
            emitter.emit_platform_callback_call("r10", 1);                                // call generated __toString with the target ABI
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // preserve returned string pointer while popping boundary
            emitter.instruction("mov QWORD PTR [rbp - 16], rdx");               // preserve returned string length
            emitter.instruction("jmp __rt_unser_allowed_tostring_finish_x");    // share normal boundary teardown
            emitter.label("__rt_unser_allowed_tostring_missing_x");
            emitter.instruction("mov QWORD PTR [rbp - 8], 0");                  // null pointer reports a missing conversion method
            emitter.instruction("mov QWORD PTR [rbp - 16], 0");                 // missing conversion has no string length
            emitter.label("__rt_unser_allowed_tostring_finish_x");
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", previous_handler_offset)); // reload previous exception-handler head
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", boundary_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // recover conversion string pointer
            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");               // recover conversion string length
            emitter.instruction("leave");                                       // release protected frame and restore caller frame
            emitter.instruction("ret");                                         // return string pair or null pointer
            emitter.label("__rt_unser_allowed_tostring_throw_x");
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", previous_handler_offset)); // reload handler preceding this boundary
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", boundary_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression skipped by longjmp
            crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
            emitter.instruction("xor eax, eax");                                // cleanup ignores placeholder result on throw
            emitter.instruction("call __rt_unserialize_end");                   // close this opened context exactly once
            emitter.instruction("leave");                                       // discard protected invocation frame
            emitter.instruction("jmp __rt_throw_current");                      // resume propagation at caller's handler
        }
    }
}
