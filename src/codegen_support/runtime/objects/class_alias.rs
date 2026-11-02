//! Purpose:
//! Emits the run-time `class_alias()` registry: `__rt_class_alias_register` records an alias of a
//! compiled class, and `__rt_class_alias_resolve` maps a class-name string through it before the
//! by-name lookups (`__rt_new_by_name`, `__rt_class_id_by_name`, dynamic `class_exists()`).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed` (emission).
//! - `crate::codegen::lower_inst::builtins::types::lower_class_alias` (register) and the by-name
//!   lookups (resolve).
//!
//! Key details:
//! - A row of `_class_aliases` is 32 bytes: alias ptr, alias len, target ptr, target len, both
//!   strings persisted (owned for the process). `_class_aliases_count` rows are live; the table
//!   holds `CLASS_ALIAS_CAPACITY` rows (Laravel's `config('app.aliases')` is ~40).
//! - Names compare case-insensitively, a leading `\` ignored, like `__rt_class_id_by_name`.
//! - Register answers php's bool: false when the original names no class (an alias of an alias
//!   resolves first) or when the alias already names a class or an alias. php also warns in both
//!   cases; the warnings are not raised.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Rows the run-time alias table holds.
pub(crate) const CLASS_ALIAS_CAPACITY: usize = 512;

/// Emits both helpers for the active target.
pub(crate) fn emit_class_alias(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_resolve_aarch64(emitter);
            emit_register_aarch64(emitter);
            emit_release_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_resolve_x86_64(emitter);
            emit_register_x86_64(emitter);
            emit_release_x86_64(emitter);
        }
    }
}

/// `__rt_class_alias_resolve`: x1/x2 = class name in, x1/x2 = resolved name out.
fn emit_resolve_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_alias resolve ---");
    emitter.label_global("__rt_class_alias_resolve");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_class_aliases_count", 0);
    emitter.instruction("cbz x9, __rt_car_ret");                                 // no alias registered: the name is its own answer
    emitter.instruction("sub sp, sp, #64");                                     // frame: query, stripped query, cursor, index
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("stp x1, x2, [sp, #16]");                               // keep the original query for a miss
    emitter.instruction("cbz x2, __rt_car_miss");                               // an empty string names no alias
    emitter.instruction("ldrb w9, [x1]");                                       // inspect an optional leading namespace separator
    emitter.instruction("cmp w9, #92");                                         // ASCII '\\'
    emitter.instruction("b.ne __rt_car_stripped");                              // nothing to strip
    emitter.instruction("add x1, x1, #1");                                      // drop the separator
    emitter.instruction("sub x2, x2, #1");                                      // and its byte from the length
    emitter.label("__rt_car_stripped");
    emitter.instruction("stp x1, x2, [sp, #32]");                               // stripped query, compared against each alias
    emitter.instruction("str xzr, [sp, #48]");                                  // row index 0
    emitter.label("__rt_car_loop");
    emitter.instruction("ldr x11, [sp, #48]");                                  // current row index
    abi::emit_load_symbol_to_reg(emitter, "x9", "_class_aliases_count", 0);
    emitter.instruction("cmp x11, x9");                                         // every live row examined?
    emitter.instruction("b.ge __rt_car_miss");                                  // no alias of that name
    abi::emit_symbol_address(emitter, "x10", "_class_aliases");
    emitter.instruction("add x10, x10, x11, lsl #5");                           // row = table + index * 32
    emitter.instruction("ldr x4, [x10, #8]");                                   // alias length
    emitter.instruction("ldr x2, [sp, #40]");                                   // stripped query length
    emitter.instruction("cmp x4, x2");                                          // different lengths cannot match
    emitter.instruction("b.ne __rt_car_next");                                  // try the next row
    emitter.instruction("ldr x3, [x10]");                                       // alias pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // stripped query pointer
    emitter.instruction("bl __rt_strcasecmp");                                  // case-insensitive comparison, x0 = 0 on equality
    emitter.instruction("cbnz x0, __rt_car_next");                              // a different name
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the matching row index
    abi::emit_symbol_address(emitter, "x10", "_class_aliases");
    emitter.instruction("add x10, x10, x11, lsl #5");                           // matching row
    emitter.instruction("ldp x1, x2, [x10, #16]");                              // answer the aliased class name
    emitter.instruction("b __rt_car_done");                                     // skip the miss path
    emitter.label("__rt_car_next");
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the row index
    emitter.instruction("add x11, x11, #1");                                    // advance to the next row
    emitter.instruction("str x11, [sp, #48]");                                  // persist it across the comparison call
    emitter.instruction("b __rt_car_loop");                                     // continue scanning
    emitter.label("__rt_car_miss");
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // the original name answers itself
    emitter.label("__rt_car_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.label("__rt_car_ret");
    emitter.instruction("ret");                                                 // resolved name in x1/x2
}

/// `__rt_class_alias_register`: x1/x2 = original, x3/x4 = alias; x0 = PHP bool.
fn emit_register_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_alias register ---");
    emitter.label_global("__rt_class_alias_register");
    emitter.instruction("sub sp, sp, #64");                                     // frame: original, alias, resolved original
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("stp x3, x4, [sp, #32]");                               // keep the alias across the lookups
    emitter.instruction("bl __rt_class_alias_resolve");                         // an alias of an alias names the first class
    emitter.instruction("stp x1, x2, [sp, #16]");                               // resolved original name
    emitter.instruction("mov x0, x1");                                          // class_id_by_name takes x0/x1
    emitter.instruction("mov x1, x2");                                          // original name length
    emitter.instruction("bl __rt_class_id_by_name");                            // does the original name a compiled class?
    emitter.instruction("cmn x0, #1");                                          // -1 is a miss
    emitter.instruction("b.eq __rt_cara_false");                                // php: Class "X" not found, false
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // the alias
    emitter.instruction("bl __rt_class_alias_resolve");                         // is the alias already an alias?
    emitter.instruction("ldr x9, [sp, #32]");                                   // the alias pointer as given
    emitter.instruction("cmp x1, x9");                                          // a resolved alias answers another pointer
    emitter.instruction("b.ne __rt_cara_false");                                // the name is in use
    emitter.instruction("ldp x0, x1, [sp, #32]");                               // class_id_by_name takes x0/x1
    emitter.instruction("bl __rt_class_id_by_name");                            // does the alias name a class?
    emitter.instruction("cmn x0, #1");                                          // -1 is a free name
    emitter.instruction("b.ne __rt_cara_false");                                // php: the name is already in use, false
    abi::emit_load_symbol_to_reg(emitter, "x9", "_class_aliases_count", 0);
    emitter.instruction(&format!("cmp x9, #{}", CLASS_ALIAS_CAPACITY));         // is the alias table full?
    emitter.instruction("b.hs __rt_cara_false");                                // no row left
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // persist the alias name
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = owned alias
    emitter.instruction("stp x1, x2, [sp, #32]");                               // keep the owned alias
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // persist the original name
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = owned original
    abi::emit_load_symbol_to_reg(emitter, "x9", "_class_aliases_count", 0);
    abi::emit_symbol_address(emitter, "x10", "_class_aliases");
    emitter.instruction("add x10, x10, x9, lsl #5");                            // the first free row
    emitter.instruction("ldp x11, x12, [sp, #32]");                             // owned alias
    emitter.instruction("stp x11, x12, [x10]");                                 // row alias ptr/len
    emitter.instruction("stp x1, x2, [x10, #16]");                              // row target ptr/len
    emitter.instruction("add x9, x9, #1");                                      // one more live row
    abi::emit_symbol_address(emitter, "x10", "_class_aliases_count");
    emitter.instruction("str x9, [x10]");                                       // publish it
    emitter.instruction("mov x0, #1");                                          // PHP true
    emitter.instruction("b __rt_cara_done");                                    // skip the false path
    emitter.label("__rt_cara_false");
    emitter.instruction("mov x0, #0");                                          // PHP false
    emitter.label("__rt_cara_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // bool in x0
}

/// `__rt_class_alias_resolve`: rax/rdx = class name in, rax/rdx = resolved name out.
fn emit_resolve_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_alias resolve ---");
    emitter.label_global("__rt_class_alias_resolve");
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_aliases_count", 0);
    emitter.instruction("test r9, r9");                                         // no alias registered?
    emitter.instruction("jz __rt_car_ret_x86");                                 // the name is its own answer
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // query, stripped query, index
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // original query pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // original query length
    emitter.instruction("test rdx, rdx");                                       // an empty string names no alias
    emitter.instruction("jz __rt_car_miss_x86");                                // answer it unchanged
    emitter.instruction("cmp BYTE PTR [rax], 92");                              // leading namespace separator?
    emitter.instruction("jne __rt_car_stripped_x86");                           // nothing to strip
    emitter.instruction("add rax, 1");                                          // drop the separator
    emitter.instruction("sub rdx, 1");                                          // and its byte from the length
    emitter.label("__rt_car_stripped_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // stripped query pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // stripped query length
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // row index 0
    emitter.label("__rt_car_loop_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // current row index
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_aliases_count", 0);
    emitter.instruction("cmp r11, r9");                                         // every live row examined?
    emitter.instruction("jge __rt_car_miss_x86");                               // no alias of that name
    abi::emit_symbol_address(emitter, "r10", "_class_aliases");
    emitter.instruction("shl r11, 5");                                          // index * 32
    emitter.instruction("add r10, r11");                                        // row address
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // alias length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // different lengths cannot match
    emitter.instruction("jne __rt_car_next_x86");                               // try the next row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // stripped query pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // stripped query length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // alias pointer
    emitter.instruction("call __rt_strcasecmp");                                // rax = 0 on equality
    emitter.instruction("test rax, rax");                                       // the same name?
    emitter.instruction("jnz __rt_car_next_x86");                               // a different name
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the matching row index
    abi::emit_symbol_address(emitter, "r10", "_class_aliases");
    emitter.instruction("shl r11, 5");                                          // index * 32
    emitter.instruction("add r10, r11");                                        // matching row
    emitter.instruction("mov rax, QWORD PTR [r10 + 16]");                       // aliased class name pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 24]");                       // aliased class name length
    emitter.instruction("jmp __rt_car_done_x86");                               // skip the miss path
    emitter.label("__rt_car_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance to the next row
    emitter.instruction("jmp __rt_car_loop_x86");                               // continue scanning
    emitter.label("__rt_car_miss_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the original name answers itself
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // with its original length
    emitter.label("__rt_car_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.label("__rt_car_ret_x86");
    emitter.instruction("ret");                                                 // resolved name in rax/rdx
}

/// `__rt_class_alias_register`: rdi/rsi = original, rdx/rcx = alias; rax = PHP bool.
fn emit_register_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_alias register ---");
    emitter.label_global("__rt_class_alias_register");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // original, alias, owned alias
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // alias pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // alias length
    emitter.instruction("mov rax, rdi");                                        // resolve takes rax/rdx
    emitter.instruction("mov rdx, rsi");                                        // original length
    emitter.instruction("call __rt_class_alias_resolve");                       // an alias of an alias names the first class
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // resolved original pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // resolved original length
    emitter.instruction("mov rdi, rax");                                        // class_id_by_name takes rdi/rsi
    emitter.instruction("mov rsi, rdx");                                        // original length
    emitter.instruction("call __rt_class_id_by_name");                          // does the original name a compiled class?
    emitter.instruction("cmp rax, -1");                                         // -1 is a miss
    emitter.instruction("je __rt_cara_false_x86");                              // php: Class "X" not found, false
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the alias
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // its length
    emitter.instruction("call __rt_class_alias_resolve");                       // is the alias already an alias?
    emitter.instruction("cmp rax, QWORD PTR [rbp - 24]");                       // a resolved alias answers another pointer
    emitter.instruction("jne __rt_cara_false_x86");                             // the name is in use
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // class_id_by_name takes rdi/rsi
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // alias length
    emitter.instruction("call __rt_class_id_by_name");                          // does the alias name a class?
    emitter.instruction("cmp rax, -1");                                         // -1 is a free name
    emitter.instruction("jne __rt_cara_false_x86");                             // php: the name is already in use, false
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_aliases_count", 0);
    emitter.instruction(&format!("cmp r9, {}", CLASS_ALIAS_CAPACITY));          // is the alias table full?
    emitter.instruction("jae __rt_cara_false_x86");                             // no row left
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // persist the alias name
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // alias length
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = owned alias
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the owned alias
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // and its length
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // persist the original name
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // original length
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = owned original
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_aliases_count", 0);
    abi::emit_symbol_address(emitter, "r10", "_class_aliases");
    emitter.instruction("mov r11, r9");                                         // the first free row index
    emitter.instruction("shl r11, 5");                                          // index * 32
    emitter.instruction("add r10, r11");                                        // row address
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // owned alias pointer
    emitter.instruction("mov QWORD PTR [r10], r11");                            // row alias ptr
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // owned alias length
    emitter.instruction("mov QWORD PTR [r10 + 8], r11");                        // row alias len
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // row target ptr
    emitter.instruction("mov QWORD PTR [r10 + 24], rdx");                       // row target len
    emitter.instruction("add r9, 1");                                           // one more live row
    abi::emit_symbol_address(emitter, "r10", "_class_aliases_count");
    emitter.instruction("mov QWORD PTR [r10], r9");                             // publish it
    emitter.instruction("mov eax, 1");                                          // PHP true
    emitter.instruction("jmp __rt_cara_done_x86");                              // skip the false path
    emitter.label("__rt_cara_false_x86");
    emitter.instruction("xor eax, eax");                                        // PHP false
    emitter.label("__rt_cara_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // bool in rax
}

/// `__rt_class_alias_release`: frees every row's persisted names and empties the table (the
/// main epilogue calls it before the heap-debug report).
fn emit_release_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_alias release ---");
    emitter.label_global("__rt_class_alias_release");
    emitter.instruction("sub sp, sp, #32");                                     // frame: row index
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("str xzr, [sp, #16]");                                  // row index 0
    emitter.label("__rt_carl_loop");
    emitter.instruction("ldr x11, [sp, #16]");                                  // current row index
    abi::emit_load_symbol_to_reg(emitter, "x9", "_class_aliases_count", 0);
    emitter.instruction("cmp x11, x9");                                         // every live row freed?
    emitter.instruction("b.ge __rt_carl_done");                                 // the table is empty
    abi::emit_symbol_address(emitter, "x10", "_class_aliases");
    emitter.instruction("add x10, x10, x11, lsl #5");                           // row = table + index * 32
    emitter.instruction("str x10, [sp, #24]");                                  // keep the row across the first free
    emitter.instruction("ldr x0, [x10]");                                       // owned alias name
    emitter.instruction("bl __rt_heap_free_safe");                              // release it
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the row
    emitter.instruction("ldr x0, [x10, #16]");                                  // owned target name
    emitter.instruction("bl __rt_heap_free_safe");                              // release it
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the row index
    emitter.instruction("add x11, x11, #1");                                    // advance to the next row
    emitter.instruction("str x11, [sp, #16]");                                  // persist it across the frees
    emitter.instruction("b __rt_carl_loop");                                    // continue
    emitter.label("__rt_carl_done");
    abi::emit_symbol_address(emitter, "x10", "_class_aliases_count");
    emitter.instruction("str xzr, [x10]");                                      // no alias is left
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // done
}

/// `__rt_class_alias_release` on x86_64.
fn emit_release_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_alias release ---");
    emitter.label_global("__rt_class_alias_release");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 16");                                         // row index, row address
    emitter.instruction("mov QWORD PTR [rbp - 8], 0");                          // row index 0
    emitter.label("__rt_carl_loop_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // current row index
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_aliases_count", 0);
    emitter.instruction("cmp r11, r9");                                         // every live row freed?
    emitter.instruction("jge __rt_carl_done_x86");                              // the table is empty
    abi::emit_symbol_address(emitter, "r10", "_class_aliases");
    emitter.instruction("shl r11, 5");                                          // index * 32
    emitter.instruction("add r10, r11");                                        // row address
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // keep the row across the first free
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // owned alias name
    emitter.instruction("call __rt_heap_free_safe");                            // release it
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the row
    emitter.instruction("mov rax, QWORD PTR [r10 + 16]");                       // owned target name
    emitter.instruction("call __rt_heap_free_safe");                            // release it
    emitter.instruction("add QWORD PTR [rbp - 8], 1");                          // advance to the next row
    emitter.instruction("jmp __rt_carl_loop_x86");                              // continue
    emitter.label("__rt_carl_done_x86");
    abi::emit_symbol_address(emitter, "r10", "_class_aliases_count");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // no alias is left
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // done
}
