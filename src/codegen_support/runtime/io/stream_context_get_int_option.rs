//! Purpose:
//! Emits the `__rt_get_int_context_option` runtime helper — integer-typed
//! sibling of `__rt_get_string_context_option`. Looks up
//! `_stream_context_options[wrapper][option]` and returns the result when
//! the value is an integer (tag 0) or bool (tag 3, 0/1).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - Socket option consumers that need a 0/1 boolean (`tcp_nodelay`,
//!   `so_reuseport`, `ipv6_v6only`) or a numeric value (future
//!   `socket.timeout_ms`-style options).
//!
//! Key details:
//! - Same two-step hash walk as the string variant: wrapper key on the top
//!   hash, option key on the sub-hash.
//! - Direct or Mixed-boxed bool values widen to 0/1 and integers pass through;
//!   any other payload shape misses.
//! - On hit, writes `out_int_addr` with the resolved value and returns 1.
//!   On miss, leaves `*out_int_addr` untouched and returns 0 so callers
//!   can pre-load a default.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// `__rt_get_int_context_option`:
/// Input:  AArch64 x0 = wrapper_ptr, x1 = wrapper_len,
///                 x2 = opt_ptr, x3 = opt_len,
///                 x4 = out_int_addr.
///         x86_64  rdi = wrapper_ptr, rsi = wrapper_len,
///                 rdx = opt_ptr,     rcx = opt_len,
///                 r8 = out_int_addr.
/// Output: x0/rax = 1 on hit (out_int written), 0 on miss.
pub fn emit_get_int_context_option(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_get_int_context_option_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: get_int_context_option ---");
    emitter.label_global("__rt_get_int_context_option");

    // Frame (64 bytes):
    //   [sp,  0] wrapper_ptr
    //   [sp,  8] wrapper_len
    //   [sp, 16] opt_ptr
    //   [sp, 24] opt_len
    //   [sp, 32] out_int_addr
    //   [sp, 40..48] padding
    //   [sp, 48] saved x29
    //   [sp, 56] saved x30
    emitter.instruction("sub sp, sp, #64");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save wrapper_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save wrapper_len
    emitter.instruction("str x2, [sp, #16]");                                   // save opt_ptr
    emitter.instruction("str x3, [sp, #24]");                                   // save opt_len
    emitter.instruction("str x4, [sp, #32]");                                   // save out_int_addr

    // Load top-level options hash; bail when null.
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("ldr x0, [x9]");                                        // top hash pointer (may be null)
    // Outside an fopen scope the active bridge is empty, but the request default
    // context still applies — stream_socket_client() + enable_crypto() reads its
    // ssl options this way.
    emitter.instruction("cbnz x0, __rt_gico_have_options");                     // an explicit scope wins
    // An explicit but EMPTY context must mask the default, so the fallback only
    // applies when no scope is active at all. The options pointer is null in both
    // cases; the active-handle slot is what tells them apart.
    abi::emit_symbol_address(emitter, "x9", "_stream_current_context_handle");
    emitter.instruction("ldr x9, [x9]");                                        // handle of the active context scope
    emitter.instruction(&format!("cbnz x9, __rt_gico_have_options"));           // a scope is active: its emptiness is meaningful
    abi::emit_symbol_address(emitter, "x9", "_stream_default_context_handle");
    emitter.instruction("ldr x0, [x9]");                                        // request-default context handle
    emitter.instruction("cbz x0, __rt_gico_have_options");                      // no default context exists
    emitter.instruction("bl __rt_context_state");                               // resolve its ContextState
    emitter.instruction("cbz x0, __rt_gico_have_options");                      // a closed default context has no options
    emitter.instruction("ldr x0, [x0, #0]");                                    // CONTEXT_OPTIONS_OFFSET
    emitter.label("__rt_gico_have_options");
    emitter.instruction("cbz x0, __rt_gico_miss");                              // branch when the checked value is zero or equal

    // hash_get(top, wrapper) → x1 = value_lo (sub-hash ptr on hit).
    emitter.instruction("ldr x1, [sp, #0]");                                    // wrapper_ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // wrapper_len
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=lo, x2=hi, x3=tag
    emitter.instruction("cbz x0, __rt_gico_miss");                              // branch when the checked value is zero or equal

    // hash_get(sub, option) → x1 = value_lo, x3 = tag.
    emitter.instruction("mov x0, x1");                                          // sub-hash → first arg
    emitter.instruction("ldr x1, [sp, #16]");                                   // opt_ptr
    emitter.instruction("ldr x2, [sp, #24]");                                   // opt_len
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=lo, x2=hi, x3=tag
    emitter.instruction("cbz x0, __rt_gico_miss");                              // branch when the checked value is zero or equal
    // Accept int (tag 0) and bool (tag 3). Both already carry the numeric
    // payload in x1.
    emitter.instruction("cmp x3, #0");                                          // tag 0 = int
    emitter.instruction("b.eq __rt_gico_write");                                // branch when the checked value is zero or equal
    emitter.instruction("cmp x3, #3");                                          // tag 3 = bool
    emitter.instruction("b.eq __rt_gico_write");                                // direct booleans already carry their normalized payload
    emitter.instruction("cmp x3, #7");                                          // is the option stored as a boxed Mixed cell?
    emitter.instruction("b.ne __rt_gico_miss");                                 // every other option shape is non-numeric here
    emitter.instruction("mov x0, x1");                                          // pass the boxed option cell to Mixed unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // recover the runtime tag and scalar payload
    emitter.instruction("cmp x0, #0");                                          // did the Mixed cell contain an integer?
    emitter.instruction("b.eq __rt_gico_write");                                // accept boxed integers
    emitter.instruction("cmp x0, #3");                                          // did the Mixed cell contain a boolean?
    emitter.instruction("b.ne __rt_gico_miss");                                 // boxed non-int/non-bool values miss

    emitter.label("__rt_gico_write");
    emitter.instruction("ldr x9, [sp, #32]");                                   // out_int_addr
    emitter.instruction("str x1, [x9]");                                        // *out_int = value
    emitter.instruction("mov x0, #1");                                          // hit
    emitter.instruction("b __rt_gico_done");                                    // continue at target label

    emitter.label("__rt_gico_miss");
    emitter.instruction("mov x0, #0");                                          // miss
    emitter.label("__rt_gico_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for get int context option.
fn emit_get_int_context_option_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_int_context_option ---");
    emitter.label_global("__rt_get_int_context_option");

    // rbp-relative frame:
    //   [rbp -  8] wrapper_ptr
    //   [rbp - 16] wrapper_len
    //   [rbp - 24] opt_ptr
    //   [rbp - 32] opt_len
    //   [rbp - 40] out_int_addr
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save wrapper_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save wrapper_len
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save opt_ptr
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save opt_len
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save out_int_addr

    // Load top-level options hash.
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_stream_context_options", 0); // prepare SysV call argument
    // See the AArch64 counterpart: the active bridge is only published inside an fopen scope, and
    // outside one — `stream_socket_client()` then `stream_socket_enable_crypto()` — the request
    // default context still applies. Without this fallback x86_64 reported every option missing,
    // so `ssl.verify_peer` never reached the TLS attach and a self-signed peer was always refused.
    emitter.instruction("test rdi, rdi");                                       // check whether the runtime value is zero
    emitter.instruction("jnz __rt_gico_have_options_x86");                      // an explicit scope wins
    // An explicit but EMPTY context must mask the default, so the fallback only applies when no
    // scope is active at all. The options pointer is null in both cases; the active-handle slot is
    // what tells them apart.
    abi::emit_load_symbol_to_reg(emitter, "r10", "_stream_current_context_handle", 0);
    emitter.instruction("test r10, r10");                                       // is a context scope active?
    emitter.instruction("jnz __rt_gico_have_options_x86");                      // a scope is active: its emptiness is meaningful
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_stream_default_context_handle", 0);
    emitter.instruction("test rdi, rdi");                                       // was a request default context ever created?
    emitter.instruction("jz __rt_gico_have_options_x86");                       // no default context exists
    emitter.instruction("call __rt_context_state");                             // resolve its ContextState
    emitter.instruction("test rax, rax");                                       // did the default context resolve?
    emitter.instruction("jz __rt_gico_have_options_x86");                       // a closed default context has no options
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // CONTEXT_OPTIONS_OFFSET
    emitter.label("__rt_gico_have_options_x86");
    emitter.instruction("test rdi, rdi");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gico_miss_x86");                               // branch when the checked value is zero or equal

    // hash_get(top, wrapper).
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // wrapper_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // wrapper_len
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=lo, rsi=hi, rcx=tag
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gico_miss_x86");                               // branch when the checked value is zero or equal

    // hash_get(sub, option) — sub-hash already in rdi.
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // opt_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // opt_len
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=lo, rsi=hi, rcx=tag
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gico_miss_x86");                               // branch when the checked value is zero or equal
    emitter.instruction("cmp rcx, 0");                                          // int tag
    emitter.instruction("je __rt_gico_write_x86");                              // branch when the checked value is zero or equal
    emitter.instruction("cmp rcx, 3");                                          // bool tag
    emitter.instruction("je __rt_gico_write_x86");                              // direct booleans already carry their normalized payload
    emitter.instruction("cmp rcx, 7");                                          // is the option stored as a boxed Mixed cell?
    emitter.instruction("jne __rt_gico_miss_x86");                              // every other option shape is non-numeric here
    emitter.instruction("mov rax, rdi");                                        // pass the boxed option cell to Mixed unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // recover the runtime tag and scalar payload
    emitter.instruction("cmp rax, 0");                                          // did the Mixed cell contain an integer?
    emitter.instruction("je __rt_gico_write_x86");                              // accept boxed integers
    emitter.instruction("cmp rax, 3");                                          // did the Mixed cell contain a boolean?
    emitter.instruction("jne __rt_gico_miss_x86");                              // boxed non-int/non-bool values miss

    emitter.label("__rt_gico_write_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // out_int_addr
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // *out_int = value
    emitter.instruction("mov eax, 1");                                          // hit
    emitter.instruction("jmp __rt_gico_done_x86");                              // continue at target label

    emitter.label("__rt_gico_miss_x86");
    emitter.instruction("xor eax, eax");                                        // miss
    emitter.label("__rt_gico_done_x86");
    emitter.instruction("add rsp, 48");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
