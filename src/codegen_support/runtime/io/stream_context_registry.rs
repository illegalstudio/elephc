//! Purpose:
//! Maintains per-resource stream-context and listener metadata.
//!
//! Called from:
//! - stream-context lowering and the socket-server/accept runtime helpers.
//!
//! Key details:
//! - Context ids are bounded synthetic resource ids, not native descriptors.
//! - Listener metadata is copied into accepted-stream scratch globals before
//!   the TLS lowering reads it.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

/// Emits the context/listener registry helpers.
pub fn emit_stream_context_registry(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream context/listener registry ---");
    emitter.label_global("__rt_stream_context_register");
    // x0 = options pointer; return x0 = synthetic context id, 0 on exhaustion.
    abi::emit_symbol_address(emitter, "x9", "_stream_context_next_id");
    emitter.instruction("ldr x10, [x9]");                                       // reload the registry index or context handle
    emitter.instruction("cbnz x10, __rt_sctx_reg_have_id");                     // non-zero result follows __rt_sctx_reg_have_id
    emitter.instruction("mov x10, #1");                                         // compute the registry slot or context metadata
    emitter.label("__rt_sctx_reg_have_id");
    emitter.instruction("cmp x10, #256");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("b.ge __rt_sctx_reg_fail");                             // reject an exhausted or out-of-range registry slot
    emitter.instruction("add x11, x10, #1");                                    // compute the registry slot or context metadata
    emitter.instruction("str x11, [x9]");                                       // preserve registry state across the context helper call
    emitter.instruction("lsl x11, x10, #3");                                    // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "x9", "_stream_context_table");
    emitter.instruction("str x0, [x9, x11]");                                   // preserve registry state across the context helper call
    emitter.instruction("sub sp, sp, #16");                                     // reserve the context id and link-register frame
    emitter.instruction("stp x10, x30, [sp]");                                  // preserve the context id and caller return address
    emitter.instruction("mov x1, x0");                                          // compute the registry slot or context metadata
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("ldp x0, x30, [sp]");                                   // restore the context id and caller return address
    emitter.instruction("add sp, sp, #16");                                     // release the registry helper frame
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_sctx_reg_fail");
    emitter.instruction("mov x0, #0");                                          // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_context_lookup");
    emitter.instruction("cbz x0, __rt_sctx_lookup_global");                     // null or false result follows __rt_sctx_lookup_global
    emitter.instruction("cmp x0, #256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("b.hs __rt_sctx_lookup_empty");                         // reject an exhausted or out-of-range registry slot
    emitter.instruction("lsl x1, x0, #3");                                      // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "x9", "_stream_context_table");
    emitter.instruction("ldr x0, [x9, x1]");                                    // reload the registry index or context handle
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_sctx_lookup_global");
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("ldr x0, [x9]");                                        // reload the registry index or context handle
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_sctx_lookup_empty");
    emitter.instruction("mov x0, #0");                                          // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_context_update");
    // x0 = id, x1 = options pointer.
    emitter.instruction("sub sp, sp, #16");                                     // preserve the old options pointer and caller return address
    emitter.instruction("str x30, [sp, #8]");                                   // save the link register across retain/release calls
    emitter.instruction("cbz x0, __rt_sctx_update_global");                     // null or false result follows __rt_sctx_update_global
    emitter.instruction("cmp x0, #256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("b.hs __rt_sctx_update_done");                          // continue after the registry update
    emitter.instruction("lsl x2, x0, #3");                                      // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "x9", "_stream_context_table");
    emitter.instruction("ldr x3, [x9, x2]");                                    // reload the registry index or context handle
    emitter.instruction("mov x4, x3");                                          // compute the registry slot or context metadata
    emitter.instruction("str x1, [x9, x2]");                                    // preserve registry state across the context helper call
    emitter.instruction("cbz x1, __rt_sctx_update_release_old");                // null or false result follows __rt_sctx_update_release_old
    emitter.instruction("str x4, [sp]");                                        // preserve registry state across the context helper call
    emitter.instruction("mov x0, x1");                                          // compute the registry slot or context metadata
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("ldr x4, [sp]");                                        // reload the registry index or context handle
    emitter.label("__rt_sctx_update_release_old");
    emitter.instruction("mov x0, x4");                                          // compute the registry slot or context metadata
    emitter.instruction("cbz x0, __rt_sctx_update_done");                       // null or false result follows __rt_sctx_update_done
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_sctx_update_done");
    emitter.instruction("mov x0, #1");                                          // compute the registry slot or context metadata
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller return address
    emitter.instruction("add sp, sp, #16");                                     // release the registry helper frame
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_sctx_update_global");
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str x1, [x9]");                                        // preserve registry state across the context helper call
    emitter.instruction("b __rt_sctx_update_done");                             // return through the shared link-register epilogue

    emitter.label_global("__rt_stream_listener_register");
    // x0=listener fd, x1=context ptr, x2=flags, x3=tls method.
    emitter.instruction("cmp x0, #256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("b.hs __rt_listener_reg_fail");                         // reject an exhausted or out-of-range registry slot
    emitter.instruction("sub sp, sp, #48");                                     // reserve arguments plus the caller return address
    emitter.instruction("stp x0, x1, [sp]");                                    // preserve registry state across the context helper call
    emitter.instruction("stp x2, x3, [sp, #16]");                               // preserve registry state across the context helper call
    emitter.instruction("str x30, [sp, #32]");                                  // save the link register across the retain call
    emitter.instruction("mov x0, x1");                                          // compute the registry slot or context metadata
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("ldp x0, x1, [sp]");                                    // return the registry operation status
    emitter.instruction("ldp x2, x3, [sp, #16]");                               // return the registry operation status
    emitter.instruction("ldr x30, [sp, #32]");                                  // restore the caller return address
    emitter.instruction("add sp, sp, #48");                                     // release the registry helper frame
    emitter.instruction("lsl x4, x0, #3");                                      // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_context");
    emitter.instruction("str x1, [x9, x4]");                                    // preserve registry state across the context helper call
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_flags");
    emitter.instruction("str x2, [x9, x4]");                                    // preserve registry state across the context helper call
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_tls_method");
    emitter.instruction("str x3, [x9, x4]");                                    // preserve registry state across the context helper call
    emitter.instruction("mov x0, #1");                                          // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_listener_reg_fail");
    emitter.instruction("mov x0, #0");                                          // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_listener_prepare_accept");
    // x0=listener fd; return x0=tls method and publish context/flags globals.
    emitter.instruction("cmp x0, #256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("b.hs __rt_listener_prepare_empty");                    // reject an exhausted or out-of-range registry slot
    emitter.instruction("lsl x1, x0, #3");                                      // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_context");
    emitter.instruction("ldr x2, [x9, x1]");                                    // reload the registry index or context handle
    emitter.instruction("mov x11, x2");                                         // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "x9", "_accepted_stream_context");
    emitter.instruction("str x2, [x9]");                                        // preserve registry state across the context helper call
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_flags");
    emitter.instruction("ldr x2, [x9, x1]");                                    // reload the registry index or context handle
    abi::emit_symbol_address(emitter, "x9", "_accepted_stream_flags");
    emitter.instruction("str x2, [x9]");                                        // preserve registry state across the context helper call
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_tls_method");
    emitter.instruction("ldr x0, [x9, x1]");                                    // reload the registry index or context handle
    abi::emit_symbol_address(emitter, "x9", "_accepted_stream_tls_method");
    emitter.instruction("str x0, [x9]");                                        // preserve registry state across the context helper call
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_listener_prepare_empty");
    emitter.instruction("mov x0, #0");                                          // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_listener_clear");
    emitter.instruction("cmp x0, #256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("b.hs __rt_listener_clear_empty");                      // reject an exhausted or out-of-range registry slot
    emitter.instruction("sub sp, sp, #16");                                     // reserve the temporary frame used by the stream operation
    emitter.instruction("stp x0, x30, [sp]");                                   // preserve the listener descriptor and caller return address
    emitter.instruction("lsl x1, x0, #3");                                      // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_context");
    emitter.instruction("ldr x2, [x9, x1]");                                    // reload the registry index or context handle
    emitter.instruction("cbz x2, __rt_listener_clear_slots");                   // null or false result follows __rt_listener_clear_slots
    emitter.instruction("mov x0, x2");                                          // compute the registry slot or context metadata
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction("ldr x0, [sp]");                                        // reload the registry index or context handle
    emitter.instruction("lsl x1, x0, #3");                                      // compute the registry slot or context metadata
    emitter.label("__rt_listener_clear_slots");
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_context");
    emitter.instruction("str xzr, [x9, x1]");                                   // preserve registry state across the context helper call
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_flags");
    emitter.instruction("str xzr, [x9, x1]");                                   // preserve registry state across the context helper call
    abi::emit_symbol_address(emitter, "x9", "_stream_listener_tls_method");
    emitter.instruction("str xzr, [x9, x1]");                                   // preserve registry state across the context helper call
    emitter.label("__rt_listener_clear_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller return address
    emitter.instruction("add sp, sp, #16");                                     // release the registry helper frame
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_listener_clear_empty");
    emitter.instruction("ret");                                                 // return the registry operation status
}

fn emit_x86(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream context/listener registry ---");
    emitter.label_global("__rt_stream_context_register");
    emitter.instruction("mov r8, rdi");                                         // compute the registry slot or context metadata
    abi::emit_load_symbol_to_reg(emitter, "rax", "_stream_context_next_id", 0);
    emitter.instruction("test rax, rax");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jnz __rt_sctx_reg_have_id_x86");                       // non-zero result follows __rt_sctx_reg_have_id_x86
    emitter.instruction("mov rax, 1");                                          // compute the registry slot or context metadata
    emitter.label("__rt_sctx_reg_have_id_x86");
    emitter.instruction("cmp rax, 256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_sctx_reg_fail_x86");                          // reject an exhausted or out-of-range registry slot
    emitter.instruction("mov r9, rax");                                         // compute the registry slot or context metadata
    emitter.instruction("lea rcx, [rax + 1]");                                  // compute the registry slot or context metadata
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_stream_context_next_id", 0);
    emitter.instruction("shl rax, 3");                                          // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "rdx", "_stream_context_table");
    emitter.instruction("mov QWORD PTR [rdx + rax], r8");                       // compute the registry slot or context metadata
    emitter.instruction("sub rsp, 24");                                         // reserve 8-byte call alignment plus saved id
    emitter.instruction("mov QWORD PTR [rsp], r9");                             // compute the registry slot or context metadata
    emitter.instruction("mov rax, r8");                                         // compute the registry slot or context metadata
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // reload the registry index or context handle
    emitter.instruction("add rsp, 24");                                         // release aligned context frame
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_sctx_reg_fail_x86");
    emitter.instruction("xor eax, eax");                                        // return the registry operation status
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_context_lookup");
    emitter.instruction("test rdi, rdi");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jz __rt_sctx_lookup_global_x86");                      // null or false result follows __rt_sctx_lookup_global_x86
    emitter.instruction("cmp rdi, 256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_sctx_lookup_empty_x86");                      // reject an exhausted or out-of-range registry slot
    emitter.instruction("lea rsi, [rdi * 8]");                                  // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "rax", "_stream_context_table");
    emitter.instruction("mov rax, QWORD PTR [rax + rsi]");                      // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_sctx_lookup_global_x86");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_stream_context_options", 0);
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_sctx_lookup_empty_x86");
    emitter.instruction("xor eax, eax");                                        // return the registry operation status
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_context_update");
    emitter.instruction("test rdi, rdi");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jz __rt_sctx_update_global_x86");                      // null or false result follows __rt_sctx_update_global_x86
    emitter.instruction("cmp rdi, 256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_sctx_update_done_x86");                       // continue after the registry update
    emitter.instruction("lea rdx, [rdi * 8]");                                  // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "rax", "_stream_context_table");
    emitter.instruction("mov rcx, QWORD PTR [rax + rdx]");                      // compute the registry slot or context metadata
    emitter.instruction("mov r11, rcx");                                        // compute the registry slot or context metadata
    emitter.instruction("mov QWORD PTR [rax + rdx], rsi");                      // compute the registry slot or context metadata
    emitter.instruction("test rsi, rsi");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jz __rt_sctx_update_old_x86");                         // null or false result follows __rt_sctx_update_old_x86
    emitter.instruction("sub rsp, 24");                                         // reserve 8-byte call alignment plus saved old context
    emitter.instruction("mov QWORD PTR [rsp], r11");                            // compute the registry slot or context metadata
    emitter.instruction("mov rax, rsi");                                        // compute the registry slot or context metadata
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov r11, QWORD PTR [rsp]");                            // compute the registry slot or context metadata
    emitter.instruction("add rsp, 24");                                         // release aligned update frame
    emitter.label("__rt_sctx_update_old_x86");
    emitter.instruction("test r11, r11");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jz __rt_sctx_update_done_x86");                        // null or false result follows __rt_sctx_update_done_x86
    emitter.instruction("mov rax, r11");                                        // compute the registry slot or context metadata
    emitter.instruction("sub rsp, 8");                                          // align the decref call after the frameless update path
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction("add rsp, 8");                                          // release the decref alignment pad
    emitter.label("__rt_sctx_update_done_x86");
    emitter.instruction("mov eax, 1");                                          // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_sctx_update_global_x86");
    abi::emit_store_reg_to_symbol(emitter, "rsi", "_stream_context_options", 0);
    emitter.instruction("mov eax, 1");                                          // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_listener_register");
    if emitter.platform == Platform::Windows {
        emitter.instruction("sub rsp, 40");                                     // reserve aligned Windows slot frame
        emitter.instruction("mov QWORD PTR [rsp + 0], rsi");                    // compute the registry slot or context metadata
        emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // compute the registry slot or context metadata
        emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                   // compute the registry slot or context metadata
        emitter.instruction("call __rt_win_stream_slot");                       // publish the Windows stream-slot metadata
        emitter.instruction("mov rdi, rax");                                    // compute the registry slot or context metadata
        emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                    // reload the registry index or context handle
        emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                    // compute the registry slot or context metadata
        emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                   // compute the registry slot or context metadata
        emitter.instruction("add rsp, 40");                                     // release aligned Windows slot frame
    }
    emitter.instruction("cmp rdi, 256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_listener_reg_fail_x86");                      // reject an exhausted or out-of-range registry slot
    emitter.instruction("sub rsp, 40");                                         // reserve aligned listener retain frame
    emitter.instruction("mov QWORD PTR [rsp + 0], rdi");                        // compute the registry slot or context metadata
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // compute the registry slot or context metadata
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // compute the registry slot or context metadata
    emitter.instruction("mov QWORD PTR [rsp + 24], rcx");                       // compute the registry slot or context metadata
    emitter.instruction("mov rax, rsi");                                        // compute the registry slot or context metadata
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // reload the registry index or context handle
    emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                        // reload the registry index or context handle
    emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");                       // compute the registry slot or context metadata
    emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");                       // compute the registry slot or context metadata
    emitter.instruction("add rsp, 40");                                         // release aligned listener retain frame
    emitter.instruction("lea rax, [rdi * 8]");                                  // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_context");
    emitter.instruction("mov QWORD PTR [r10 + rax], rsi");                      // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_flags");
    emitter.instruction("mov QWORD PTR [r10 + rax], rdx");                      // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_tls_method");
    emitter.instruction("mov QWORD PTR [r10 + rax], rcx");                      // compute the registry slot or context metadata
    emitter.instruction("mov eax, 1");                                          // compute the registry slot or context metadata
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_listener_reg_fail_x86");
    emitter.instruction("xor eax, eax");                                        // return the registry operation status
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_listener_prepare_accept");
    if emitter.platform == Platform::Windows {
        emitter.instruction("sub rsp, 8");                                      // align Windows slot lookup call
        emitter.instruction("call __rt_win_stream_slot");                       // map opaque SOCKET to bounded slot
        emitter.instruction("add rsp, 8");                                      // release lookup alignment
        emitter.instruction("mov rdi, rax");                                    // compute the registry slot or context metadata
    }
    emitter.instruction("cmp rdi, 256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_listener_prepare_empty_x86");                 // reject an exhausted or out-of-range registry slot
    emitter.instruction("lea rsi, [rdi * 8]");                                  // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_context");
    emitter.instruction("mov rax, QWORD PTR [r10 + rsi]");                      // compute the registry slot or context metadata
    abi::emit_store_reg_to_symbol(emitter, "rax", "_accepted_stream_context", 0);
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_flags");
    emitter.instruction("mov rax, QWORD PTR [r10 + rsi]");                      // compute the registry slot or context metadata
    abi::emit_store_reg_to_symbol(emitter, "rax", "_accepted_stream_flags", 0);
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_tls_method");
    emitter.instruction("mov rax, QWORD PTR [r10 + rsi]");                      // compute the registry slot or context metadata
    abi::emit_store_reg_to_symbol(emitter, "rax", "_accepted_stream_tls_method", 0);
    emitter.instruction("ret");                                                 // return the registry operation status
    emitter.label("__rt_listener_prepare_empty_x86");
    emitter.instruction("xor eax, eax");                                        // return the registry operation status
    emitter.instruction("ret");                                                 // return the registry operation status

    emitter.label_global("__rt_stream_listener_clear");
    emitter.instruction("sub rsp, 24");                                         // align calls and retain original descriptor plus compact slot
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // retain the caller-visible descriptor for the return path
    if emitter.platform == Platform::Windows {
        emitter.instruction("call __rt_win_stream_slot");                       // map opaque SOCKET to bounded slot
        emitter.instruction("mov rdi, rax");                                    // compute the registry slot or context metadata
    }
    emitter.instruction("mov QWORD PTR [rsp + 8], rdi");                        // preserve the compact listener slot across decref
    emitter.instruction("cmp rdi, 256");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_listener_clear_done_x86");                    // continue after the registry update
    emitter.instruction("lea rsi, [rdi * 8]");                                  // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_context");
    emitter.instruction("mov rax, QWORD PTR [r10 + rsi]");                      // compute the registry slot or context metadata
    emitter.instruction("test rax, rax");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jz __rt_listener_clear_slots_x86");                    // null or false result follows __rt_listener_clear_slots_x86
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                        // restore the compact slot after decref clobbers scratch registers
    emitter.instruction("lea rsi, [rdi * 8]");                                  // compute the registry slot or context metadata
    emitter.label("__rt_listener_clear_slots_x86");
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_context");
    emitter.instruction("mov QWORD PTR [r10 + rsi], 0");                        // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_flags");
    emitter.instruction("mov QWORD PTR [r10 + rsi], 0");                        // compute the registry slot or context metadata
    abi::emit_symbol_address(emitter, "r10", "_stream_listener_tls_method");
    emitter.instruction("mov QWORD PTR [r10 + rsi], 0");                        // compute the registry slot or context metadata
    emitter.label("__rt_listener_clear_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // return the original listener descriptor
    emitter.instruction("add rsp, 24");                                         // release the aligned clear frame
    emitter.instruction("ret");                                                 // return the registry operation status
}
