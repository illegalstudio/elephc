//! Purpose:
//! Emits the runtime class-string-to-id lookup used by dynamic class constants.
//! Searches the same closed-world `_classes_by_name` registry as `new $class()`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::class_name_id`.
//! - `crate::codegen_support::runtime::emitters::managed`.
//!
//! Key details:
//! - Input names are compared case-insensitively and one leading namespace separator is ignored.
//! - A miss returns `-1`; the caller decides whether that becomes a PHP runtime error.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific runtime class-name lookup helper.
pub(crate) fn emit_class_id_by_name(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 class-name lookup using the 32-byte dynamic-class registry rows.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class id by name ---");
    emitter.label_global("__rt_class_id_by_name");
    emitter.instruction("sub sp, sp, #64");                                   // reserve saved input, registry cursor, and return frame slots
    emitter.instruction("stp x29, x30, [sp, #0]");                             // preserve frame pointer and return address
    emitter.instruction("mov x29, sp");                                       // establish a stable helper frame
    emitter.instruction("mov x2, x1");                                        // alias resolution takes the name in x1/x2
    emitter.instruction("mov x1, x0");                                        // name pointer
    emitter.instruction("bl __rt_class_alias_resolve");                       // a class_alias() name answers its class id
    emitter.instruction("mov x0, x1");                                        // resolved name pointer back in x0
    emitter.instruction("mov x1, x2");                                        // resolved name length back in x1
    emitter.instruction("cbz x1, __rt_cibn_miss");                            // an empty string cannot name a class
    emitter.instruction("ldrb w9, [x0]");                                     // inspect an optional leading namespace separator
    emitter.instruction("cmp w9, #92");                                       // ASCII '\\'
    emitter.instruction("b.ne __rt_cibn_input_ready");                        // preserve ordinary class names unchanged
    emitter.instruction("add x0, x0, #1");                                    // strip exactly one leading namespace separator
    emitter.instruction("sub x1, x1, #1");                                    // keep the name length aligned with the stripped pointer
    emitter.instruction("cbz x1, __rt_cibn_miss");                            // a separator-only string is not a class name
    emitter.label("__rt_cibn_input_ready");
    emitter.instruction("str x0, [sp, #16]");                                 // save query pointer across case-folded comparison calls
    emitter.instruction("str x1, [sp, #24]");                                 // save query length across case-folded comparison calls
    abi::emit_load_symbol_to_reg(emitter, "x9", "_classes_by_name_count", 0);
    emitter.instruction("cbz x9, __rt_cibn_miss");                            // avoid addressing a vacant table
    abi::emit_symbol_address(emitter, "x10", "_classes_by_name");
    emitter.instruction("str x10, [sp, #32]");                                // persist the current 32-byte row pointer
    emitter.instruction("str xzr, [sp, #40]");                                // begin at registry row zero
    emitter.label("__rt_cibn_loop");
    emitter.instruction("ldr x11, [sp, #40]");                                // reload the current row index
    abi::emit_load_symbol_to_reg(emitter, "x9", "_classes_by_name_count", 0);
    emitter.instruction("cmp x11, x9");                                       // stop after every registered class was examined
    emitter.instruction("b.ge __rt_cibn_miss");                               // the requested class is absent
    emitter.instruction("ldr x10, [sp, #32]");                                // reload the current row pointer
    emitter.instruction("ldr x13, [x10, #8]");                                // stored class-name length
    emitter.instruction("ldr x2, [sp, #24]");                                 // reload query length for a cheap rejection
    emitter.instruction("cmp x13, x2");                                       // different lengths cannot compare equal
    emitter.instruction("b.ne __rt_cibn_skip");                               // continue with the next row when lengths differ
    emitter.instruction("str x11, [sp, #48]");                                // preserve row index across __rt_strcasecmp
    emitter.instruction("ldr x1, [sp, #16]");                                 // class-name query pointer
    emitter.instruction("ldr x2, [sp, #24]");                                 // class-name query length
    emitter.instruction("ldr x3, [x10]");                                     // stored class-name pointer
    emitter.instruction("mov x4, x13");                                       // stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                // compare PHP class names case-insensitively
    emitter.instruction("ldr x11, [sp, #48]");                                // restore the row index after the helper call
    emitter.instruction("cbz x0, __rt_cibn_match");                           // equal names select this class id
    emitter.label("__rt_cibn_skip");
    emitter.instruction("ldr x10, [sp, #32]");                                // reload the current row cursor
    emitter.instruction("add x10, x10, #32");                                 // advance by one dynamic-class registry row
    emitter.instruction("str x10, [sp, #32]");                                // persist the advanced cursor
    emitter.instruction("add x11, x11, #1");                                  // advance the logical row index
    emitter.instruction("str x11, [sp, #40]");                                // store it for the next loop iteration
    emitter.instruction("b __rt_cibn_loop");                                  // keep scanning until a match or exhaustion
    emitter.label("__rt_cibn_match");
    emitter.instruction("ldr x10, [sp, #32]");                                // reload the matched registry row
    emitter.instruction("ldr x0, [x10, #16]");                                // return its runtime class id
    emitter.instruction("b __rt_cibn_done");                                  // skip the miss sentinel
    emitter.label("__rt_cibn_miss");
    emitter.instruction("mov x0, #-1");                                       // sentinel consumed by dynamic constant dispatch
    emitter.label("__rt_cibn_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                             // restore caller frame state
    emitter.instruction("add sp, sp, #64");                                   // release helper frame storage
    emitter.instruction("ret");                                                // return class id or miss sentinel
}

/// Emits the x86_64 class-name lookup using the 32-byte dynamic-class registry rows.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class id by name ---");
    emitter.label_global("__rt_class_id_by_name");
    emitter.instruction("push rbp");                                          // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                      // establish a stable helper frame
    emitter.instruction("sub rsp, 48");                                       // reserve query, cursor, and loop-index slots
    emitter.instruction("mov rax, rdi");                                      // alias resolution takes the name in rax/rdx
    emitter.instruction("mov rdx, rsi");                                      // name length
    emitter.instruction("call __rt_class_alias_resolve");                     // a class_alias() name answers its class id
    emitter.instruction("mov rdi, rax");                                      // resolved name pointer back in rdi
    emitter.instruction("mov rsi, rdx");                                      // resolved name length back in rsi
    emitter.instruction("test rsi, rsi");                                     // an empty string cannot name a class
    emitter.instruction("jz __rt_cibn_miss_x86");                             // return the miss sentinel immediately
    emitter.instruction("cmp BYTE PTR [rdi], 92");                             // inspect an optional leading namespace separator
    emitter.instruction("jne __rt_cibn_input_ready_x86");                     // preserve ordinary class names unchanged
    emitter.instruction("add rdi, 1");                                        // strip exactly one leading namespace separator
    emitter.instruction("sub rsi, 1");                                        // keep the length synchronized with the pointer
    emitter.instruction("jz __rt_cibn_miss_x86");                             // a separator-only string is not a class name
    emitter.label("__rt_cibn_input_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                      // save query pointer across string comparisons
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                     // save query length across string comparisons
    abi::emit_load_symbol_to_reg(emitter, "r9", "_classes_by_name_count", 0);
    emitter.instruction("test r9, r9");                                       // avoid addressing a vacant table
    emitter.instruction("jz __rt_cibn_miss_x86");                             // no registered classes means no match
    abi::emit_symbol_address(emitter, "r10", "_classes_by_name");
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                     // persist the current row cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                       // begin at registry row zero
    emitter.label("__rt_cibn_loop_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                     // reload logical row index
    abi::emit_load_symbol_to_reg(emitter, "r9", "_classes_by_name_count", 0);
    emitter.instruction("cmp r11, r9");                                       // stop after every registered class was examined
    emitter.instruction("jge __rt_cibn_miss_x86");                            // the requested class is absent
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                     // reload the current row pointer
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                      // stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                     // different lengths cannot compare equal
    emitter.instruction("jne __rt_cibn_skip_x86");                            // continue with the next row when lengths differ
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                     // preserve row index across __rt_strcasecmp
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                      // class-name query pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                     // class-name query length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                          // stored class-name pointer
    emitter.instruction("call __rt_strcasecmp");                              // compare PHP class names case-insensitively
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                     // restore the row index after the helper call
    emitter.instruction("test rax, rax");                                     // zero denotes a case-insensitive match
    emitter.instruction("jz __rt_cibn_match_x86");                            // select the matching class id
    emitter.label("__rt_cibn_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                     // reload current row cursor
    emitter.instruction("add r10, 32");                                       // advance by one registry row
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                     // persist the advanced cursor
    emitter.instruction("add r11, 1");                                        // advance logical row index
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                     // store it for the next iteration
    emitter.instruction("jmp __rt_cibn_loop_x86");                            // keep scanning until a match or exhaustion
    emitter.label("__rt_cibn_match_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                     // reload matched registry row
    emitter.instruction("mov rax, QWORD PTR [r10 + 16]");                     // return its runtime class id
    emitter.instruction("jmp __rt_cibn_done_x86");                            // skip miss sentinel
    emitter.label("__rt_cibn_miss_x86");
    emitter.instruction("mov rax, -1");                                       // sentinel consumed by dynamic constant dispatch
    emitter.label("__rt_cibn_done_x86");
    emitter.instruction("add rsp, 48");                                       // release helper frame storage
    emitter.instruction("pop rbp");                                           // restore caller frame pointer
    emitter.instruction("ret");                                                // return class id or miss sentinel
}
