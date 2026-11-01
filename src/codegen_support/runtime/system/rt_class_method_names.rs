//! Purpose:
//! Emits `__rt_class_method_names`, which builds `get_class_methods()`'s indexed string array
//! from a class's method rows, filtered by the calling scope.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::get_class_methods`.
//!
//! Key details:
//! - Input: x0/rdi = class id (already bounds-checked against `_class_method_rows_count`),
//!   x1/rsi = calling scope's class id, or -1 outside any class.
//! - Output: x0/rax = a fresh indexed array of the visible names, in row order.
//! - Rows come from `_class_method_rows_ptrs` (see `runtime::data::class_methods_registry`).
//!   A public row is always visible; a private one only from its declaring class; a protected
//!   one when the scope and the declaring class are related either way, php's
//!   `zend_check_protected()` walk over `_class_parent_ids`.
//! - Every value that must survive `__rt_array_new` / `__rt_array_push_str` lives in the frame.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the target-specific `__rt_class_method_names` helper.
pub fn emit_rt_class_method_names(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation.
///
/// Frame (80 bytes): [0] row list, [8] row count, [16] scope id, [24] row index,
/// [32] result array, [64] saved fp/lr.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_method_names ---");
    emitter.label_global("__rt_class_method_names");
    emitter.instruction("sub sp, sp, #80");                                     // reserve the method-row walk frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x9", "_class_method_rows_ptrs");
    emitter.instruction("ldr x9, [x9, x0, lsl #3]");                            // this class's method row list
    emitter.instruction("ldr x10, [x9]");                                       // number of rows in the list
    emitter.instruction("str x9, [sp, #0]");                                    // save the row list across helper calls
    emitter.instruction("str x10, [sp, #8]");                                   // save the row count for the loop guard
    emitter.instruction("str x1, [sp, #16]");                                   // save the calling scope's class id
    emitter.instruction("str xzr, [sp, #24]");                                  // row index = 0
    emitter.instruction("mov x11, #1");                                         // an empty result still needs one slot of capacity
    emitter.instruction("cmp x10, x11");                                        // fewer rows than the minimum capacity?
    emitter.instruction("csel x0, x11, x10, lo");                               // capacity = max(row count, 1)
    emitter.instruction("mov x1, #16");                                         // indexed string slots are 16 bytes
    emitter.instruction("bl __rt_array_new");                                   // x0 = the empty result array
    emitter.instruction("str x0, [sp, #32]");                                   // save the result array

    emitter.label("__rt_class_method_names_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the row index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the row count
    emitter.instruction("cmp x9, x10");                                         // walked every row?
    emitter.instruction("b.ge __rt_class_method_names_done");                   // return the built array
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the row list
    emitter.instruction("add x11, x11, #8");                                    // skip the count word to the first row
    emitter.instruction("add x11, x11, x9, lsl #5");                            // row = rows + index * 32
    emitter.instruction("ldr x12, [x11, #16]");                                 // row visibility: 0 public, 1 protected, 2 private
    emitter.instruction("cbz x12, __rt_class_method_names_include");            // a public method is visible from every scope
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the calling scope's class id
    emitter.instruction("cmp x13, #0");                                         // outside any class?
    emitter.instruction("b.lt __rt_class_method_names_next");                   // global scope sees public methods only
    emitter.instruction("ldr x14, [x11, #24]");                                 // the class that declares this method
    emitter.instruction("cmp x12, #2");                                         // is the method private?
    emitter.instruction("b.ne __rt_class_method_names_protected");              // protected methods follow the ancestry rule
    emitter.instruction("cmp x14, x13");                                        // private: only its declaring class sees it
    emitter.instruction("b.eq __rt_class_method_names_include");                // the scope declared this private method
    emitter.instruction("b __rt_class_method_names_next");                      // any other scope does not
    emitter.label("__rt_class_method_names_protected");
    emitter.instruction("mov x15, x14");                                        // walk the declaring class's ancestry first
    emitter.label("__rt_class_method_names_declaring_walk");
    emitter.instruction("cmp x15, x13");                                        // is the scope an ancestor of the declaring class?
    emitter.instruction("b.eq __rt_class_method_names_include");                // related scopes see protected methods
    emitter.instruction("cmp x15, #0");                                         // walked past the root?
    emitter.instruction("b.lt __rt_class_method_names_scope_start");            // try the scope's own ancestry next
    abi::emit_symbol_address(emitter, "x16", "_class_parent_ids");
    emitter.instruction("ldr x15, [x16, x15, lsl #3]");                         // advance to the parent class id
    emitter.instruction("b __rt_class_method_names_declaring_walk");            // continue up the declaring ancestry
    emitter.label("__rt_class_method_names_scope_start");
    emitter.instruction("mov x15, x13");                                        // walk the scope's ancestry
    emitter.label("__rt_class_method_names_scope_walk");
    emitter.instruction("cmp x15, x14");                                        // is the declaring class an ancestor of the scope?
    emitter.instruction("b.eq __rt_class_method_names_include");                // related scopes see protected methods
    emitter.instruction("cmp x15, #0");                                         // walked past the root?
    emitter.instruction("b.lt __rt_class_method_names_next");                   // unrelated scopes do not
    abi::emit_symbol_address(emitter, "x16", "_class_parent_ids");
    emitter.instruction("ldr x15, [x16, x15, lsl #3]");                         // advance to the parent class id
    emitter.instruction("b __rt_class_method_names_scope_walk");                // continue up the scope ancestry
    emitter.label("__rt_class_method_names_include");
    emitter.instruction("ldr x0, [sp, #32]");                                   // the result array
    emitter.instruction("ldr x1, [x11]");                                       // method name pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // method name length
    emitter.instruction("bl __rt_array_push_str");                              // append the visible method name
    emitter.instruction("str x0, [sp, #32]");                                   // keep the possibly grown array
    emitter.label("__rt_class_method_names_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the row index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next row
    emitter.instruction("str x9, [sp, #24]");                                   // persist the row index
    emitter.instruction("b __rt_class_method_names_loop");                      // continue the row walk
    emitter.label("__rt_class_method_names_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return the result array
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return to the lowered call site
}

/// Emits the x86_64 implementation.
///
/// Frame (rbp-relative): [-8] row list, [-16] row count, [-24] scope id, [-32] row index,
/// [-40] result array.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_method_names ---");
    emitter.label_global("__rt_class_method_names");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve the aligned method-row walk frame
    abi::emit_symbol_address(emitter, "r10", "_class_method_rows_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + rdi*8]");                    // this class's method row list
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // number of rows in the list
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the row list across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the row count for the loop guard
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the calling scope's class id
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // row index = 0
    emitter.instruction("mov rdi, r11");                                        // capacity candidate = row count
    emitter.instruction("mov rax, 1");                                          // an empty result still needs one slot of capacity
    emitter.instruction("cmp rdi, rax");                                        // fewer rows than the minimum capacity?
    emitter.instruction("cmovb rdi, rax");                                      // capacity = max(row count, 1)
    emitter.instruction("mov rsi, 16");                                         // indexed string slots are 16 bytes
    emitter.instruction("call __rt_array_new");                                 // rax = the empty result array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the result array

    emitter.label("__rt_class_method_names_loop_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the row index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // walked every row?
    emitter.instruction("jge __rt_class_method_names_done_x");                  // return the built array
    emitter.instruction("mov r11, rcx");                                        // row index
    emitter.instruction("shl r11, 5");                                          // index * 32
    emitter.instruction("add r11, QWORD PTR [rbp - 8]");                        // rows + index * 32
    emitter.instruction("add r11, 8");                                          // skip the count word to the row
    emitter.instruction("mov r8, QWORD PTR [r11 + 16]");                        // row visibility: 0 public, 1 protected, 2 private
    emitter.instruction("test r8, r8");                                         // is the method public?
    emitter.instruction("jz __rt_class_method_names_include_x");                // a public method is visible from every scope
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the calling scope's class id
    emitter.instruction("test r9, r9");                                         // outside any class?
    emitter.instruction("js __rt_class_method_names_next_x");                   // global scope sees public methods only
    emitter.instruction("mov r10, QWORD PTR [r11 + 24]");                       // the class that declares this method
    emitter.instruction("cmp r8, 2");                                           // is the method private?
    emitter.instruction("jne __rt_class_method_names_protected_x");             // protected methods follow the ancestry rule
    emitter.instruction("cmp r10, r9");                                         // private: only its declaring class sees it
    emitter.instruction("je __rt_class_method_names_include_x");                // the scope declared this private method
    emitter.instruction("jmp __rt_class_method_names_next_x");                  // any other scope does not
    emitter.label("__rt_class_method_names_protected_x");
    emitter.instruction("mov rax, r10");                                        // walk the declaring class's ancestry first
    emitter.label("__rt_class_method_names_declaring_walk_x");
    emitter.instruction("cmp rax, r9");                                         // is the scope an ancestor of the declaring class?
    emitter.instruction("je __rt_class_method_names_include_x");                // related scopes see protected methods
    emitter.instruction("test rax, rax");                                       // walked past the root?
    emitter.instruction("js __rt_class_method_names_scope_start_x");            // try the scope's own ancestry next
    abi::emit_symbol_address(emitter, "rdx", "_class_parent_ids");
    emitter.instruction("mov rax, QWORD PTR [rdx + rax*8]");                    // advance to the parent class id
    emitter.instruction("jmp __rt_class_method_names_declaring_walk_x");        // continue up the declaring ancestry
    emitter.label("__rt_class_method_names_scope_start_x");
    emitter.instruction("mov rax, r9");                                         // walk the scope's ancestry
    emitter.label("__rt_class_method_names_scope_walk_x");
    emitter.instruction("cmp rax, r10");                                        // is the declaring class an ancestor of the scope?
    emitter.instruction("je __rt_class_method_names_include_x");                // related scopes see protected methods
    emitter.instruction("test rax, rax");                                       // walked past the root?
    emitter.instruction("js __rt_class_method_names_next_x");                   // unrelated scopes do not
    abi::emit_symbol_address(emitter, "rdx", "_class_parent_ids");
    emitter.instruction("mov rax, QWORD PTR [rdx + rax*8]");                    // advance to the parent class id
    emitter.instruction("jmp __rt_class_method_names_scope_walk_x");            // continue up the scope ancestry
    emitter.label("__rt_class_method_names_include_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // the result array
    emitter.instruction("mov rsi, QWORD PTR [r11]");                            // method name pointer
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // method name length
    emitter.instruction("call __rt_array_push_str");                            // append the visible method name
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep the possibly grown array
    emitter.label("__rt_class_method_names_next_x");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next row
    emitter.instruction("jmp __rt_class_method_names_loop_x");                  // continue the row walk
    emitter.label("__rt_class_method_names_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the result array
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the lowered call site
}
