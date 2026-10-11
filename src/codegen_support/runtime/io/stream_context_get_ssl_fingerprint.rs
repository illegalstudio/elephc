//! Purpose:
//! Read `ssl.peer_fingerprint` from the active stream context.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io::close_crypto_arch`.
//!
//! Key details:
//! - Scalar values are returned as borrowed pointer/length pairs.
//! - Associative arrays are serialized as `algorithm=fingerprint;...` into a
//!   bounded scratch buffer; the TLS bridge validates every entry and rejects
//!   unknown algorithms or malformed values.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the SSL peer-fingerprint context lookup.
///
/// Input: output pointer and length addresses (`x0/x1`, `rdi/rsi`).
/// Output: `x0/rax` is one for a present scalar/array value and zero when the
/// option is absent. Arrays are copied to `_tls_peer_fingerprint_scratch`.
pub fn emit_get_ssl_peer_fingerprint(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_ssl_peer_fingerprint ---");
    emitter.label_global("__rt_get_ssl_peer_fingerprint");
    // Frame: output addresses, array/cursor, serialized length/count, current
    // key/value spans, and saved frame registers.
    emitter.instruction("sub sp, sp, #96");                                     // reserve the temporary frame used by the stream operation
    emitter.instruction("stp x29, x30, [sp, #80]");                             // keep fingerprint conversion state live across validation calls
    emitter.instruction("mov x29, sp");                                         // assemble the canonical fingerprint bytes
    emitter.instruction("str x0, [sp, #0]");                                    // keep fingerprint conversion state live across validation calls
    emitter.instruction("str x1, [sp, #8]");                                    // keep fingerprint conversion state live across validation calls
    abi::emit_symbol_address(emitter, "x9", "_accepted_stream_context");
    emitter.instruction("ldr x0, [x9]");                                        // restore fingerprint formatting state after validation
    emitter.instruction("cbnz x0, __rt_gsfp_root_ready_a");                     // non-zero result follows __rt_gsfp_root_ready_a
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("ldr x0, [x9]");                                        // restore fingerprint formatting state after validation
    emitter.label("__rt_gsfp_root_ready_a");
    emitter.instruction("cbz x0, __rt_gsfp_miss_a");                            // null or false result follows __rt_gsfp_miss_a
    abi::emit_symbol_address(emitter, "x1", "_ssl_key_str");
    emitter.instruction("mov x2, #3");                                          // assemble the canonical fingerprint bytes
    abi::emit_call_label(emitter, "__rt_hash_get");
    emitter.instruction("cbz x0, __rt_gsfp_miss_a");                            // null or false result follows __rt_gsfp_miss_a
    emitter.instruction("mov x0, x1");                                          // assemble the canonical fingerprint bytes
    abi::emit_symbol_address(emitter, "x1", "_ssl_peer_fingerprint_key_str");
    emitter.instruction("mov x2, #16");                                         // assemble the canonical fingerprint bytes
    abi::emit_call_label(emitter, "__rt_hash_get");
    emitter.instruction("cbz x0, __rt_gsfp_miss_a");                            // null or false result follows __rt_gsfp_miss_a
    emitter.instruction("cmp x3, #1");                                          // check the value before selecting the corresponding outcome
    emitter.instruction("b.eq __rt_gsfp_scalar_a");                             // select scalar fingerprint formatting
    emitter.instruction("cmp x3, #5");                                          // check the value before selecting the corresponding outcome
    emitter.instruction("b.ne __rt_gsfp_invalid_a");                            // reject malformed fingerprint input
    emitter.instruction("str x1, [sp, #16]");                                   // keep fingerprint conversion state live across validation calls
    emitter.instruction("str xzr, [sp, #24]");                                  // keep fingerprint conversion state live across validation calls
    emitter.instruction("str xzr, [sp, #32]");                                  // keep fingerprint conversion state live across validation calls
    emitter.instruction("str xzr, [sp, #40]");                                  // keep fingerprint conversion state live across validation calls
    emitter.label("__rt_gsfp_array_loop_a");
    emitter.instruction("ldr x0, [sp, #16]");                                   // restore fingerprint formatting state after validation
    emitter.instruction("ldr x1, [sp, #24]");                                   // restore fingerprint formatting state after validation
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    emitter.instruction("str x0, [sp, #24]");                                   // keep fingerprint conversion state live across validation calls
    emitter.instruction("cmp x0, #-1");                                         // check the value before selecting the corresponding outcome
    emitter.instruction("b.eq __rt_gsfp_array_done_a");                         // continue encoding fingerprint entries
    emitter.instruction("cmp x5, #1");                                          // check the value before selecting the corresponding outcome
    emitter.instruction("b.ne __rt_gsfp_invalid_a");                            // reject malformed fingerprint input
    emitter.instruction("cmp x2, #32");                                         // check the value before selecting the corresponding outcome
    emitter.instruction("b.hi __rt_gsfp_invalid_a");                            // reject malformed fingerprint input
    emitter.instruction("cmp x4, #128");                                        // check the value before selecting the corresponding outcome
    emitter.instruction("b.hi __rt_gsfp_invalid_a");                            // reject malformed fingerprint input
    emitter.instruction("ldr x9, [sp, #40]");                                   // restore fingerprint formatting state after validation
    emitter.instruction("cmp x9, #3");                                          // check the value before selecting the corresponding outcome
    emitter.instruction("b.hs __rt_gsfp_invalid_a");                            // reject malformed fingerprint input
    emitter.instruction("str x1, [sp, #48]");                                   // keep fingerprint conversion state live across validation calls
    emitter.instruction("str x2, [sp, #56]");                                   // keep fingerprint conversion state live across validation calls
    emitter.instruction("str x3, [sp, #64]");                                   // keep fingerprint conversion state live across validation calls
    emitter.instruction("str x4, [sp, #72]");                                   // keep fingerprint conversion state live across validation calls
    emitter.instruction("cmp x9, #0");                                          // check the value before selecting the corresponding outcome
    emitter.instruction("b.eq __rt_gsfp_key_a");                                // continue encoding fingerprint entries
    abi::emit_symbol_address(emitter, "x10", "_tls_peer_fingerprint_scratch");
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore fingerprint formatting state after validation
    emitter.instruction("mov w12, #59");                                        // assemble the canonical fingerprint bytes
    emitter.instruction("strb w12, [x10, x11]");                                // append the generated fingerprint byte to the output
    emitter.instruction("add x11, x11, #1");                                    // assemble the canonical fingerprint bytes
    emitter.instruction("str x11, [sp, #32]");                                  // keep fingerprint conversion state live across validation calls
    emitter.label("__rt_gsfp_key_a");
    abi::emit_symbol_address(emitter, "x10", "_tls_peer_fingerprint_scratch");
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore fingerprint formatting state after validation
    emitter.instruction("mov x12, #0");                                         // assemble the canonical fingerprint bytes
    emitter.label("__rt_gsfp_key_loop_a");
    emitter.instruction("cmp x12, x2");                                         // check the value before selecting the corresponding outcome
    emitter.instruction("b.hs __rt_gsfp_value_sep_a");                          // reject malformed fingerprint input
    emitter.instruction("ldrb w13, [x1, x12]");                                 // assemble the canonical fingerprint bytes
    emitter.instruction("strb w13, [x10, x11]");                                // append the generated fingerprint byte to the output
    emitter.instruction("add x12, x12, #1");                                    // assemble the canonical fingerprint bytes
    emitter.instruction("add x11, x11, #1");                                    // assemble the canonical fingerprint bytes
    emitter.instruction("b __rt_gsfp_key_loop_a");                              // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_value_sep_a");
    emitter.instruction("mov w13, #61");                                        // assemble the canonical fingerprint bytes
    emitter.instruction("strb w13, [x10, x11]");                                // append the generated fingerprint byte to the output
    emitter.instruction("add x11, x11, #1");                                    // assemble the canonical fingerprint bytes
    emitter.instruction("mov x12, #0");                                         // assemble the canonical fingerprint bytes
    emitter.label("__rt_gsfp_value_loop_a");
    emitter.instruction("cmp x12, x4");                                         // check the value before selecting the corresponding outcome
    emitter.instruction("b.hs __rt_gsfp_value_done_a");                         // reject malformed fingerprint input
    emitter.instruction("ldrb w13, [x3, x12]");                                 // assemble the canonical fingerprint bytes
    emitter.instruction("strb w13, [x10, x11]");                                // append the generated fingerprint byte to the output
    emitter.instruction("add x12, x12, #1");                                    // assemble the canonical fingerprint bytes
    emitter.instruction("add x11, x11, #1");                                    // assemble the canonical fingerprint bytes
    emitter.instruction("b __rt_gsfp_value_loop_a");                            // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_value_done_a");
    emitter.instruction("str x11, [sp, #32]");                                  // keep fingerprint conversion state live across validation calls
    emitter.instruction("ldr x9, [sp, #40]");                                   // restore fingerprint formatting state after validation
    emitter.instruction("add x9, x9, #1");                                      // assemble the canonical fingerprint bytes
    emitter.instruction("str x9, [sp, #40]");                                   // keep fingerprint conversion state live across validation calls
    emitter.instruction("b __rt_gsfp_array_loop_a");                            // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_array_done_a");
    emitter.instruction("ldr x9, [sp, #40]");                                   // restore fingerprint formatting state after validation
    emitter.instruction("cbz x9, __rt_gsfp_invalid_a");                         // null or false result follows __rt_gsfp_invalid_a
    abi::emit_symbol_address(emitter, "x10", "_tls_peer_fingerprint_scratch");
    emitter.instruction("ldr x11, [sp, #0]");                                   // restore fingerprint formatting state after validation
    emitter.instruction("str x10, [x11]");                                      // keep fingerprint conversion state live across validation calls
    emitter.instruction("ldr x11, [sp, #8]");                                   // restore fingerprint formatting state after validation
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore fingerprint formatting state after validation
    emitter.instruction("str x12, [x11]");                                      // keep fingerprint conversion state live across validation calls
    emitter.instruction("mov x0, #1");                                          // assemble the canonical fingerprint bytes
    emitter.instruction("b __rt_gsfp_done_a");                                  // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_scalar_a");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore fingerprint formatting state after validation
    emitter.instruction("str x1, [x0]");                                        // keep fingerprint conversion state live across validation calls
    emitter.instruction("ldr x0, [sp, #8]");                                    // restore fingerprint formatting state after validation
    emitter.instruction("str x2, [x0]");                                        // keep fingerprint conversion state live across validation calls
    emitter.instruction("mov x0, #1");                                          // assemble the canonical fingerprint bytes
    emitter.instruction("b __rt_gsfp_done_a");                                  // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_invalid_a");
    abi::emit_symbol_address(emitter, "x9", "_tls_peer_fingerprint_scratch");
    emitter.instruction("mov w10, #63");                                        // assemble the canonical fingerprint bytes
    emitter.instruction("strb w10, [x9]");                                      // append the generated fingerprint byte to the output
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore fingerprint formatting state after validation
    emitter.instruction("str x9, [x0]");                                        // keep fingerprint conversion state live across validation calls
    emitter.instruction("ldr x0, [sp, #8]");                                    // restore fingerprint formatting state after validation
    emitter.instruction("mov x1, #1");                                          // assemble the canonical fingerprint bytes
    emitter.instruction("str x1, [x0]");                                        // keep fingerprint conversion state live across validation calls
    emitter.instruction("mov x0, #1");                                          // assemble the canonical fingerprint bytes
    emitter.instruction("b __rt_gsfp_done_a");                                  // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_miss_a");
    emitter.instruction("mov x0, #0");                                          // assemble the canonical fingerprint bytes
    emitter.label("__rt_gsfp_done_a");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the fingerprint frame and return the conversion status
    emitter.instruction("add sp, sp, #96");                                     // release the temporary frame before returning
    emitter.instruction("ret");                                                 // return the fingerprint conversion status
}

fn emit_x86(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_ssl_peer_fingerprint ---");
    emitter.label_global("__rt_get_ssl_peer_fingerprint");
    // rbp frame: output addresses, scratch, array, cursor, length/count, and
    // the current iterator key/value spans.
    emitter.instruction("push rbp");                                            // preserve the fingerprint frame state across the helper call
    emitter.instruction("mov rbp, rsp");                                        // assemble the canonical fingerprint bytes
    emitter.instruction("sub rsp, 96");                                         // reserve the temporary frame used by the stream operation
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // assemble the canonical fingerprint bytes
    abi::emit_symbol_address(emitter, "rdi", "_accepted_stream_context");
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // assemble the canonical fingerprint bytes
    emitter.instruction("test rdi, rdi");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jnz __rt_gsfp_root_ready_x86");                        // non-zero result follows __rt_gsfp_root_ready_x86
    abi::emit_symbol_address(emitter, "rdi", "_stream_context_options");
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // assemble the canonical fingerprint bytes
    emitter.label("__rt_gsfp_root_ready_x86");
    emitter.instruction("test rdi, rdi");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jz __rt_gsfp_miss_x86");                               // null or false result follows __rt_gsfp_miss_x86
    abi::emit_symbol_address(emitter, "rsi", "_ssl_key_str");
    emitter.instruction("mov edx, 3");                                          // assemble the canonical fingerprint bytes
    emitter.instruction("call __rt_hash_get");                                  // call call __rt_hash_get for the stream operation
    emitter.instruction("test rax, rax");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jz __rt_gsfp_miss_x86");                               // null or false result follows __rt_gsfp_miss_x86
    abi::emit_symbol_address(emitter, "rsi", "_ssl_peer_fingerprint_key_str");
    emitter.instruction("mov edx, 16");                                         // assemble the canonical fingerprint bytes
    emitter.instruction("call __rt_hash_get");                                  // call call __rt_hash_get for the stream operation
    emitter.instruction("test rax, rax");                                       // check the value before selecting the corresponding outcome
    emitter.instruction("jz __rt_gsfp_miss_x86");                               // null or false result follows __rt_gsfp_miss_x86
    emitter.instruction("cmp rcx, 1");                                          // check the value before selecting the corresponding outcome
    emitter.instruction("je __rt_gsfp_scalar_x86");                             // null or false result follows __rt_gsfp_scalar_x86
    emitter.instruction("cmp rcx, 5");                                          // check the value before selecting the corresponding outcome
    emitter.instruction("jne __rt_gsfp_invalid_x86");                           // non-zero result follows __rt_gsfp_invalid_x86
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // assemble the canonical fingerprint bytes
    abi::emit_symbol_address(emitter, "r10", "_tls_peer_fingerprint_scratch");
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // assemble the canonical fingerprint bytes
    emitter.label("__rt_gsfp_array_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("call __rt_hash_iter_next");                            // call call __rt_hash_iter_next for the stream operation
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // assemble the canonical fingerprint bytes
    emitter.instruction("cmp rax, -1");                                         // check the value before selecting the corresponding outcome
    emitter.instruction("je __rt_gsfp_array_done_x86");                         // null or false result follows __rt_gsfp_array_done_x86
    emitter.instruction("cmp r9, 1");                                           // check the value before selecting the corresponding outcome
    emitter.instruction("jne __rt_gsfp_invalid_x86");                           // non-zero result follows __rt_gsfp_invalid_x86
    emitter.instruction("cmp rdx, 32");                                         // check the value before selecting the corresponding outcome
    emitter.instruction("ja __rt_gsfp_invalid_x86");                            // reject malformed fingerprint input
    emitter.instruction("cmp r8, 128");                                         // check the value before selecting the corresponding outcome
    emitter.instruction("ja __rt_gsfp_invalid_x86");                            // reject malformed fingerprint input
    emitter.instruction("cmp QWORD PTR [rbp - 64], 3");                         // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_gsfp_invalid_x86");                           // reject malformed fingerprint input
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [rbp - 88], rcx");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [rbp - 96], r8");                        // assemble the canonical fingerprint bytes
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check the value before selecting the corresponding outcome
    emitter.instruction("je __rt_gsfp_key_x86");                                // null or false result follows __rt_gsfp_key_x86
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov BYTE PTR [r10 + r11], 59");                        // assemble the canonical fingerprint bytes
    emitter.instruction("inc QWORD PTR [rbp - 56]");                            // restore the fingerprint frame and return the conversion status
    emitter.label("__rt_gsfp_key_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("xor eax, eax");                                        // restore the fingerprint frame and return the conversion status
    emitter.label("__rt_gsfp_key_loop_x86");
    emitter.instruction("cmp rax, QWORD PTR [rbp - 80]");                       // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_gsfp_value_separator_x86");                   // continue encoding fingerprint entries
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov dl, BYTE PTR [rdi + rax]");                        // assemble the canonical fingerprint bytes
    emitter.instruction("mov BYTE PTR [r10 + r11], dl");                        // assemble the canonical fingerprint bytes
    emitter.instruction("inc rax");                                             // restore the fingerprint frame and return the conversion status
    emitter.instruction("inc r11");                                             // restore the fingerprint frame and return the conversion status
    emitter.instruction("jmp __rt_gsfp_key_loop_x86");                          // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_value_separator_x86");
    emitter.instruction("mov BYTE PTR [r10 + r11], 61");                        // assemble the canonical fingerprint bytes
    emitter.instruction("inc r11");                                             // restore the fingerprint frame and return the conversion status
    emitter.instruction("xor eax, eax");                                        // restore the fingerprint frame and return the conversion status
    emitter.label("__rt_gsfp_value_loop_x86");
    emitter.instruction("cmp rax, QWORD PTR [rbp - 96]");                       // check the value before selecting the corresponding outcome
    emitter.instruction("jae __rt_gsfp_value_done_x86");                        // continue encoding fingerprint entries
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov dl, BYTE PTR [rdi + rax]");                        // assemble the canonical fingerprint bytes
    emitter.instruction("mov BYTE PTR [r10 + r11], dl");                        // assemble the canonical fingerprint bytes
    emitter.instruction("inc rax");                                             // restore the fingerprint frame and return the conversion status
    emitter.instruction("inc r11");                                             // restore the fingerprint frame and return the conversion status
    emitter.instruction("jmp __rt_gsfp_value_loop_x86");                        // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_value_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // assemble the canonical fingerprint bytes
    emitter.instruction("inc QWORD PTR [rbp - 64]");                            // restore the fingerprint frame and return the conversion status
    emitter.instruction("jmp __rt_gsfp_array_loop_x86");                        // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_array_done_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check the value before selecting the corresponding outcome
    emitter.instruction("je __rt_gsfp_invalid_x86");                            // null or false result follows __rt_gsfp_invalid_x86
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [r11], r10");                            // assemble the canonical fingerprint bytes
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // restore fingerprint formatting state after validation
    emitter.instruction("mov QWORD PTR [r11], rax");                            // assemble the canonical fingerprint bytes
    emitter.instruction("mov eax, 1");                                          // assemble the canonical fingerprint bytes
    emitter.instruction("jmp __rt_gsfp_done_x86");                              // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_scalar_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // assemble the canonical fingerprint bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [r10], rsi");                            // assemble the canonical fingerprint bytes
    emitter.instruction("mov eax, 1");                                          // assemble the canonical fingerprint bytes
    emitter.instruction("jmp __rt_gsfp_done_x86");                              // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_invalid_x86");
    abi::emit_symbol_address(emitter, "r10", "_tls_peer_fingerprint_scratch");
    emitter.instruction("mov BYTE PTR [r10], 63");                              // assemble the canonical fingerprint bytes
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [r11], r10");                            // assemble the canonical fingerprint bytes
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // assemble the canonical fingerprint bytes
    emitter.instruction("mov QWORD PTR [r11], 1");                              // assemble the canonical fingerprint bytes
    emitter.instruction("mov eax, 1");                                          // assemble the canonical fingerprint bytes
    emitter.instruction("jmp __rt_gsfp_done_x86");                              // continue encoding fingerprint entries
    emitter.label("__rt_gsfp_miss_x86");
    emitter.instruction("xor eax, eax");                                        // restore the fingerprint frame and return the conversion status
    emitter.label("__rt_gsfp_done_x86");
    emitter.instruction("add rsp, 96");                                         // release the temporary frame before returning
    emitter.instruction("pop rbp");                                             // preserve the fingerprint frame state across the helper call
    emitter.instruction("ret");                                                 // return the fingerprint conversion status
}
