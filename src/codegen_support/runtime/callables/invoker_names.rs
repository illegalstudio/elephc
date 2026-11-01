//! Purpose:
//! Emits `__rt_invoker_param_name_index` and `__rt_invoker_lookup_param`, the by-name argument
//! helpers every descriptor invoker shares.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker`, in the associative-container path of every
//!   invoker body.
//!
//! Key details:
//! - A signature's visible regular parameter names live in a data table the invoker's stub hands
//!   over: `[count, (name ptr, name len) * count]`, one 8-byte word each. Names used to be spelled
//!   into every invoker body as data-label references, so two signatures differing only in
//!   parameter names could not share a body, and each body carried an unrolled comparison per
//!   parameter. With the names in a table the body depends on types and defaults alone.
//! - `__rt_invoker_param_name_index(table, key ptr, key len)` answers the index of the parameter
//!   a string key names, or -1. A null table has no names.
//! - `__rt_invoker_lookup_param(hash, table, index)` probes the argument hash for the parameter's
//!   NAME first and its positional key second, and answers exactly like `__rt_hash_get`, which it
//!   ends in, so the caller's marshalling reads the same result registers as before.
//! - Neither helper touches a callee-saved register. State that must survive a call lives in the
//!   helper's own frame, because the key comparison and the hash probe clobber every caller-saved
//!   register.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_invoker_param_name_index`.
///
/// Input: table pointer, key pointer, key byte length (first three integer argument registers).
/// Output: integer result register = matching parameter index, or -1.
pub fn emit_invoker_param_name_index(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_param_name_index_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: invoker_param_name_index ---");
    emitter.label_global("__rt_invoker_param_name_index");
    // Stack (48 bytes): [sp+16] table, [sp+24] key ptr, [sp+32] key len, [sp+40] cursor.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the walk state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // keep the name table across the key comparisons
    emitter.instruction("str x1, [sp, #8]");                                    // keep the key pointer across the key comparisons
    emitter.instruction("str x2, [sp, #16]");                                   // keep the key length across the key comparisons
    emitter.instruction("str xzr, [sp, #24]");                                  // start at the first parameter name
    emitter.label("__rt_invoker_param_name_index_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the name table
    emitter.instruction("cbz x9, __rt_invoker_param_name_index_miss");          // a signature without a table names nothing
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the cursor
    emitter.instruction("ldr x11, [x9]");                                       // load the number of names in the table
    emitter.instruction("cmp x10, x11");                                        // has every name been compared?
    emitter.instruction("b.hs __rt_invoker_param_name_index_miss");             // no name matched the key
    emitter.instruction("add x9, x9, x10, lsl #4");                             // address the cursor's (pointer, length) pair, 16 bytes per name after the two header words
    emitter.instruction("ldr x3, [x9, #16]");                                    // load the declared name pointer
    emitter.instruction("ldr x4, [x9, #24]");                                   // load the declared name length
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass the key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // pass the key length
    emitter.instruction("bl __rt_hash_key_eq");                                 // compare the key with the declared name byte for byte
    emitter.instruction("cbnz x0, __rt_invoker_param_name_index_hit");          // the key names this parameter
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the cursor after the call
    emitter.instruction("add x10, x10, #1");                                    // advance to the next declared name
    emitter.instruction("str x10, [sp, #24]");                                  // persist the cursor
    emitter.instruction("b __rt_invoker_param_name_index_loop");                // keep comparing
    emitter.label("__rt_invoker_param_name_index_hit");
    emitter.instruction("ldr x0, [sp, #24]");                                   // answer the matching parameter index
    emitter.instruction("b __rt_invoker_param_name_index_done");                // return it
    emitter.label("__rt_invoker_param_name_index_miss");
    emitter.instruction("mov x0, #-1");                                         // answer that no parameter has this name
    emitter.label("__rt_invoker_param_name_index_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the index or -1
}

fn emit_param_name_index_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: invoker_param_name_index ---");
    emitter.label_global("__rt_invoker_param_name_index");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve the walk state, keeping the stack 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the name table across the key comparisons
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the key pointer across the key comparisons
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // keep the key length across the key comparisons
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start at the first parameter name
    emitter.label("__rt_invoker_param_name_index_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the name table
    emitter.instruction("test r10, r10");                                       // a signature without a table names nothing
    emitter.instruction("jz __rt_invoker_param_name_index_miss");               // no table, no match
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the cursor
    emitter.instruction("cmp r11, QWORD PTR [r10]");                            // has every name been compared?
    emitter.instruction("jae __rt_invoker_param_name_index_miss");              // no name matched the key
    emitter.instruction("shl r11, 4");                                          // scale the cursor to the 16-byte (pointer, length) pairs
    emitter.instruction("mov rdx, QWORD PTR [r10 + r11 + 16]");                  // load the declared name pointer
    emitter.instruction("mov rcx, QWORD PTR [r10 + r11 + 24]");                 // load the declared name length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the key pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the key length
    emitter.instruction("call __rt_hash_key_eq");                               // compare the key with the declared name byte for byte
    emitter.instruction("test rax, rax");                                       // did the key name this parameter?
    emitter.instruction("jnz __rt_invoker_param_name_index_hit");               // it did
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next declared name
    emitter.instruction("jmp __rt_invoker_param_name_index_loop");              // keep comparing
    emitter.label("__rt_invoker_param_name_index_hit");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // answer the matching parameter index
    emitter.instruction("jmp __rt_invoker_param_name_index_done");              // return it
    emitter.label("__rt_invoker_param_name_index_miss");
    emitter.instruction("mov rax, -1");                                         // answer that no parameter has this name
    emitter.label("__rt_invoker_param_name_index_done");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the index or -1
}

/// Emits `__rt_invoker_lookup_param`.
///
/// Input: argument hash, name table, parameter index (first three integer argument registers).
/// Output: exactly `__rt_hash_get`'s -- found flag, borrowed value words and tag.
pub fn emit_invoker_lookup_param(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_lookup_param_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: invoker_lookup_param ---");
    emitter.label_global("__rt_invoker_lookup_param");
    emitter.instruction("cbz x1, __rt_invoker_lookup_param_numeric");           // no table: only the positional key can bind
    emitter.instruction("ldr x9, [x1]");                                        // load the number of declared names
    emitter.instruction("cmp x2, x9");                                          // is the index past the declared names?
    emitter.instruction("b.hs __rt_invoker_lookup_param_numeric");              // then only the positional key can bind
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // reserve the helper frame and save the caller linkage
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("str x0, [sp, #16]");                                   // keep the argument hash for the positional probe
    emitter.instruction("str x2, [sp, #24]");                                   // keep the index for the positional probe
    emitter.instruction("add x9, x1, x2, lsl #4");                              // address the index's (pointer, length) pair, 16 bytes per name after the two header words
    emitter.instruction("ldr x1, [x9, #16]");                                   // pass the declared name pointer as the key
    emitter.instruction("ldr x2, [x9, #24]");                                   // pass the declared name length as the key
    emitter.instruction("bl __rt_hash_get");                                    // probe the argument hash for the declared name
    emitter.instruction("cbnz x0, __rt_invoker_lookup_param_done");             // a name binds before the positional key
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the argument hash
    emitter.instruction("ldr x1, [sp, #24]");                                   // the positional key is the index itself
    emitter.instruction("mov x2, #-1");                                         // -1 marks an integer key
    emitter.instruction("bl __rt_hash_get");                                    // probe the argument hash for the positional key
    emitter.label("__rt_invoker_lookup_param_done");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore the caller linkage and release the frame
    emitter.instruction("ret");                                                 // return the lookup result
    emitter.label("__rt_invoker_lookup_param_numeric");
    emitter.instruction("mov x1, x2");                                          // the positional key is the index itself
    emitter.instruction("mov x2, #-1");                                         // -1 marks an integer key
    emitter.instruction("b __rt_hash_get");                                     // answer with the positional probe directly
}

fn emit_lookup_param_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: invoker_lookup_param ---");
    emitter.label_global("__rt_invoker_lookup_param");
    emitter.instruction("test rsi, rsi");                                       // no table: only the positional key can bind
    emitter.instruction("jz __rt_invoker_lookup_param_numeric");                // go straight to the positional probe
    emitter.instruction("cmp rdx, QWORD PTR [rsi]");                            // is the index past the declared names?
    emitter.instruction("jae __rt_invoker_lookup_param_numeric");               // then only the positional key can bind
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve two spill slots, keeping the stack 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the argument hash for the positional probe
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // keep the index for the positional probe
    emitter.instruction("shl rdx, 4");                                          // scale the index to the 16-byte (pointer, length) pairs
    emitter.instruction("mov rcx, QWORD PTR [rsi + rdx + 24]");                 // load the declared name length
    emitter.instruction("mov rsi, QWORD PTR [rsi + rdx + 16]");                  // pass the declared name pointer as the key
    emitter.instruction("mov rdx, rcx");                                        // pass the declared name length as the key
    emitter.instruction("call __rt_hash_get");                                  // probe the argument hash for the declared name
    emitter.instruction("test rax, rax");                                       // did the declared name bind?
    emitter.instruction("jnz __rt_invoker_lookup_param_done");                  // a name binds before the positional key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the argument hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the positional key is the index itself
    emitter.instruction("mov rdx, -1");                                         // -1 marks an integer key
    emitter.instruction("call __rt_hash_get");                                  // probe the argument hash for the positional key
    emitter.label("__rt_invoker_lookup_param_done");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the lookup result
    emitter.label("__rt_invoker_lookup_param_numeric");
    emitter.instruction("mov rsi, rdx");                                        // the positional key is the index itself
    emitter.instruction("mov rdx, -1");                                         // -1 marks an integer key
    emitter.instruction("jmp __rt_hash_get");                                   // answer with the positional probe directly
}
