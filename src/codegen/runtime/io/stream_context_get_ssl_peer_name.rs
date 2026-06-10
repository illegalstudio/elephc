//! Purpose:
//! Emits the `__rt_get_ssl_peer_name` runtime helper used by
//! `stream_socket_enable_crypto` to read the SNI / cert-validation
//! peer-name from `_stream_context_options["ssl"]["peer_name"]`. Returns
//! 1 on success (writing the string pointer/length to caller-provided
//! output addresses), 0 when the context has no SSL peer-name set.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - The `stream_socket_enable_crypto` builtin emitter when enabling
//!   TLS — falls back to a hardcoded `localhost` SNI when this helper
//!   reports no peer-name.
//!
//! Key details:
//! - Two nested `__rt_hash_get` lookups (top-level → "ssl" → "peer_name")
//!   navigate the nested options structure built by
//!   `stream_context_create` / `set_option`.
//! - The peer-name value must have runtime tag 1 (string). Non-string
//!   peer_name entries are treated as missing.
//! - The output addresses receive the raw pointer + length pair; the
//!   caller (`stream_socket_enable_crypto`) passes them straight to
//!   `elephc_tls_attach_fd`.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// `__rt_get_ssl_peer_name`:
/// Input:  AArch64 x0 = ptr-out address, x1 = len-out address.
///         x86_64  rdi = ptr-out address, rsi = len-out address.
/// Output: x0/rax = 1 on success (out_ptr/out_len written), 0 on miss.
pub fn emit_get_ssl_peer_name(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_get_ssl_peer_name_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: get_ssl_peer_name ---");
    emitter.label_global("__rt_get_ssl_peer_name");

    // Frame (64 bytes):
    //   [sp,  0] out_ptr_addr
    //   [sp,  8] out_len_addr
    //   [sp, 16] padding
    //   [sp, 24] saved key literal address (rodata)
    //   [sp, 32] saved x29
    //   [sp, 40] saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save out_ptr_addr
    emitter.instruction("str x1, [sp, #8]");                                    // save out_len_addr

    // -- load the top-level options hash; bail when null --
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("ldr x0, [x9]");                                        // load runtime value
    emitter.instruction("cbz x0, __rt_gspn_miss");                              // no context options at all

    // -- hash_get(top, "ssl", 3) → value_lo = sub-hash ptr, tag 5 --
    abi::emit_symbol_address(emitter, "x1", "_ssl_key_str");
    emitter.instruction("mov x2, #3");                                          // strlen("ssl") = 3
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo
    emitter.instruction("cbz x0, __rt_gspn_miss");                              // no "ssl" sub-hash

    // -- hash_get(sub, "peer_name", 9) → value_lo=str ptr, x2=str len, x3=tag --
    emitter.instruction("mov x0, x1");                                          // sub-hash → hash_get's first arg
    abi::emit_symbol_address(emitter, "x1", "_ssl_peer_name_key_str");
    emitter.instruction("mov x2, #9");                                          // strlen("peer_name") = 9
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=lo, x2=hi, x3=tag
    emitter.instruction("cbz x0, __rt_gspn_miss");                              // no "peer_name" in ssl sub-hash
    emitter.instruction("cmp x3, #1");                                          // require string tag
    emitter.instruction("b.ne __rt_gspn_miss");                                 // non-string peer_name → miss

    // -- write the (ptr, len) pair through the caller's output addresses --
    emitter.instruction("ldr x9, [sp, #0]");                                    // out_ptr_addr
    emitter.instruction("str x1, [x9]");                                        // *out_ptr = peer_name ptr
    emitter.instruction("ldr x9, [sp, #8]");                                    // out_len_addr
    emitter.instruction("str x2, [x9]");                                        // *out_len = peer_name len

    emitter.instruction("mov x0, #1");                                          // success
    emitter.instruction(&format!("b {}", "__rt_gspn_done"));                    // continue at target label

    emitter.label("__rt_gspn_miss");
    emitter.instruction("mov x0, #0");                                          // peer_name not available
    emitter.label("__rt_gspn_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for get ssl peer name.
fn emit_get_ssl_peer_name_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_ssl_peer_name ---");
    emitter.label_global("__rt_get_ssl_peer_name");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save out_ptr_addr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save out_len_addr

    // -- load top-level options hash --
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_stream_context_options", 0); // prepare SysV call argument
    emitter.instruction("test rdi, rdi");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gspn_miss_x86");                               // branch when the checked value is zero or equal

    abi::emit_symbol_address(emitter, "rsi", "_ssl_key_str");                   // load runtime data address
    emitter.instruction("mov edx, 3");                                          // strlen("ssl")
    // __rt_hash_get's x86_64 returns: rax=found, rdi=value_lo, rsi=value_hi,
    // rcx=tag — the rdi/rsi mapping mirrors the SysV first-two-args
    // registers so callers can pipeline a follow-up hash_get without
    // explicit reshuffling.
    emitter.instruction("call __rt_hash_get");                                  // call runtime helper
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gspn_miss_x86");                               // branch when the checked value is zero or equal

    // -- second hash_get(sub, "peer_name", 9): sub-hash is already in
    //    rdi from the previous call's value_lo return. --
    abi::emit_symbol_address(emitter, "rsi", "_ssl_peer_name_key_str");         // load runtime data address
    emitter.instruction("mov edx, 9");                                          // strlen("peer_name")
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=lo, rsi=hi, rcx=tag
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gspn_miss_x86");                               // branch when the checked value is zero or equal
    emitter.instruction("cmp rcx, 1");                                          // require string tag
    emitter.instruction("jne __rt_gspn_miss_x86");                              // branch when the checked value is nonzero or different

    // -- write through the caller's output addresses --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // out_ptr_addr
    emitter.instruction("mov QWORD PTR [r9], rdi");                             // *out_ptr = peer_name ptr (value_lo)
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // out_len_addr
    emitter.instruction("mov QWORD PTR [r9], rsi");                             // *out_len = peer_name len (value_hi)

    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("jmp __rt_gspn_done_x86");                              // continue at target label

    emitter.label("__rt_gspn_miss_x86");
    emitter.instruction("xor eax, eax");                                        // clear register value
    emitter.label("__rt_gspn_done_x86");
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
