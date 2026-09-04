//! Purpose:
//! Emits the `__rt_get_string_context_option` runtime helper —
//! generalized string-option reader — it replaced a dedicated `ssl.peer_name`
//! helper that accepted only a raw string tag. Looks up
//! `_stream_context_options[wrapper][option]` and returns the result
//! when the value is a string. Used by consumers like
//! `stream_socket_enable_crypto` (`ssl.peer_name`) and the http://
//! request builder (`http.method`, `http.header`, `http.content`).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - Any consumer that needs to pull a string-typed nested option from
//!   the persisted stream context.
//!
//! Key details:
//! - Two nested `__rt_hash_get` calls walk the structure: outer hash
//!   keyed by wrapper, sub-hash keyed by option name.
//! - Direct strings and strings boxed by `stream_context_set_option()` are
//!   accepted; every other value shape is treated as missing.
//! - The output addresses receive the raw ptr + len pair on hit; they
//!   are left untouched on miss so callers can pre-load a fallback
//!   default into them.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// `__rt_get_string_context_option`:
/// Input:  AArch64 x0 = wrapper_ptr, x1 = wrapper_len,
///                 x2 = opt_ptr, x3 = opt_len,
///                 x4 = out_ptr_addr, x5 = out_len_addr.
///         x86_64  rdi = wrapper_ptr, rsi = wrapper_len,
///                 rdx = opt_ptr,     rcx = opt_len,
///                 r8 = out_ptr_addr, r9 = out_len_addr.
/// Output: x0/rax = 1 on hit (out_ptr/out_len written), 0 on miss.
pub fn emit_get_string_context_option(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_get_string_context_option_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: get_string_context_option ---");
    emitter.label_global("__rt_get_string_context_option");

    // Frame (80 bytes):
    //   [sp,  0] wrapper_ptr
    //   [sp,  8] wrapper_len
    //   [sp, 16] opt_ptr
    //   [sp, 24] opt_len
    //   [sp, 32] out_ptr_addr
    //   [sp, 40] out_len_addr
    //   [sp, 48..64] padding
    //   [sp, 64] saved x29
    //   [sp, 72] saved x30
    emitter.instruction("sub sp, sp, #80");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // store runtime value
    emitter.instruction("str x1, [sp, #8]");                                    // store runtime value
    emitter.instruction("str x2, [sp, #16]");                                   // store runtime value
    emitter.instruction("str x3, [sp, #24]");                                   // store runtime value
    emitter.instruction("str x4, [sp, #32]");                                   // store runtime value
    emitter.instruction("str x5, [sp, #40]");                                   // store runtime value

    // -- load top-level options hash; bail when null --
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("ldr x0, [x9]");                                        // load runtime value
    // The active bridge is only published inside an fopen scope. Outside one —
    // stream_socket_client() followed by stream_socket_enable_crypto(), for
    // instance — the request default context still applies, so fall back to it
    // rather than reporting every option missing.
    emitter.instruction("cbnz x0, __rt_gsco_have_options");                     // an explicit scope wins
    // An explicit but EMPTY context must mask the default, so the fallback only
    // applies when no scope is active at all. The options pointer is null in both
    // cases; the active-handle slot is what tells them apart.
    abi::emit_symbol_address(emitter, "x9", "_stream_current_context_handle");
    emitter.instruction("ldr x9, [x9]");                                        // handle of the active context scope
    emitter.instruction(&format!("cbnz x9, __rt_gsco_have_options"));           // a scope is active: its emptiness is meaningful
    abi::emit_symbol_address(emitter, "x9", "_stream_default_context_handle");
    emitter.instruction("ldr x0, [x9]");                                        // request-default context handle
    emitter.instruction("cbz x0, __rt_gsco_miss");                              // no default context was ever created
    emitter.instruction("bl __rt_context_state");                               // resolve its ContextState
    emitter.instruction("cbz x0, __rt_gsco_miss");                              // a closed default context has no options
    emitter.instruction("ldr x0, [x0, #0]");                                    // CONTEXT_OPTIONS_OFFSET
    emitter.label("__rt_gsco_have_options");
    emitter.instruction("cbz x0, __rt_gsco_miss");                              // branch when the checked value is zero or equal

    // -- hash_get(top, wrapper) → value_lo = sub-hash on hit --
    emitter.instruction("ldr x1, [sp, #0]");                                    // wrapper_ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // wrapper_len
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo
    emitter.instruction("cbz x0, __rt_gsco_miss");                              // branch when the checked value is zero or equal

    // -- hash_get(sub, option) → value_lo=str ptr, x2=str len, x3=tag --
    emitter.instruction("mov x0, x1");                                          // sub-hash → first arg
    emitter.instruction("ldr x1, [sp, #16]");                                   // opt_ptr
    emitter.instruction("ldr x2, [sp, #24]");                                   // opt_len
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=lo, x2=hi, x3=tag
    emitter.instruction("cbz x0, __rt_gsco_miss");                              // branch when the checked value is zero or equal
    emitter.instruction("cmp x3, #1");                                          // is this a direct string option?
    emitter.instruction("b.eq __rt_gsco_write");                                // direct strings already expose pointer and length
    emitter.instruction("cmp x3, #7");                                          // is the option stored as a boxed Mixed cell?
    emitter.instruction("b.ne __rt_gsco_miss");                                 // every other option shape is not a string
    emitter.instruction("mov x0, x1");                                          // pass the boxed option cell to Mixed unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // recover tag plus string pointer and length
    emitter.instruction("cmp x0, #1");                                          // did the Mixed cell contain a string?
    emitter.instruction("b.ne __rt_gsco_miss");                                 // boxed non-strings do not satisfy this lookup
    emitter.label("__rt_gsco_write");

    // -- write (ptr, len) through the caller's output addresses --
    emitter.instruction("ldr x9, [sp, #32]");                                   // out_ptr_addr
    emitter.instruction("str x1, [x9]");                                        // *out_ptr = value_lo
    emitter.instruction("ldr x9, [sp, #40]");                                   // out_len_addr
    emitter.instruction("str x2, [x9]");                                        // *out_len = value_hi

    emitter.instruction("mov x0, #1");                                          // prepare AArch64 call argument
    emitter.instruction("b __rt_gsco_done");                                    // continue at target label

    emitter.label("__rt_gsco_miss");
    emitter.instruction("mov x0, #0");                                          // prepare AArch64 call argument
    emitter.label("__rt_gsco_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for get string context option.
fn emit_get_string_context_option_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_string_context_option ---");
    emitter.label_global("__rt_get_string_context_option");

    // rbp-relative frame:
    //   [rbp -  8] wrapper_ptr
    //   [rbp - 16] wrapper_len
    //   [rbp - 24] opt_ptr
    //   [rbp - 32] opt_len
    //   [rbp - 40] out_ptr_addr
    //   [rbp - 48] out_len_addr
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // store runtime value

    // -- load top-level options hash --
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_stream_context_options", 0); // prepare SysV call argument
    // See the AArch64 counterpart: outside an fopen scope the request default context still
    // applies, which is how `ssl.peer_name` reaches a `stream_socket_enable_crypto()` handshake.
    emitter.instruction("test rdi, rdi");                                       // check whether the runtime value is zero
    emitter.instruction("jnz __rt_gsco_have_options_x86");                      // an explicit scope wins
    abi::emit_load_symbol_to_reg(emitter, "r10", "_stream_current_context_handle", 0);
    emitter.instruction("test r10, r10");                                       // is a context scope active?
    emitter.instruction("jnz __rt_gsco_have_options_x86");                      // a scope is active: its emptiness is meaningful
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_stream_default_context_handle", 0);
    emitter.instruction("test rdi, rdi");                                       // was a request default context ever created?
    emitter.instruction("jz __rt_gsco_miss_x86");                               // no default context was ever created
    emitter.instruction("call __rt_context_state");                             // resolve its ContextState
    emitter.instruction("test rax, rax");                                       // did the default context resolve?
    emitter.instruction("jz __rt_gsco_miss_x86");                               // a closed default context has no options
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // CONTEXT_OPTIONS_OFFSET
    emitter.label("__rt_gsco_have_options_x86");
    emitter.instruction("test rdi, rdi");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gsco_miss_x86");                               // branch when the checked value is zero or equal

    // hash_get(top, wrapper)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // wrapper_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // wrapper_len
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=hi, rcx=tag
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gsco_miss_x86");                               // branch when the checked value is zero or equal

    // hash_get(sub, option) — sub-hash already in rdi
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // opt_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // opt_len
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=lo, rsi=hi, rcx=tag
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_gsco_miss_x86");                               // branch when the checked value is zero or equal
    emitter.instruction("cmp rcx, 1");                                          // is this a direct string option?
    emitter.instruction("je __rt_gsco_write_x86");                              // direct strings already expose pointer and length
    emitter.instruction("cmp rcx, 7");                                          // is the option stored as a boxed Mixed cell?
    emitter.instruction("jne __rt_gsco_miss_x86");                              // every other option shape is not a string
    emitter.instruction("mov rax, rdi");                                        // pass the boxed option cell to Mixed unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // recover tag plus string pointer and length
    emitter.instruction("cmp rax, 1");                                          // did the Mixed cell contain a string?
    emitter.instruction("jne __rt_gsco_miss_x86");                              // boxed non-strings do not satisfy this lookup
    emitter.instruction("mov rsi, rdx");                                        // normalize the unboxed string length to the direct lookup ABI
    emitter.label("__rt_gsco_write_x86");

    // -- write through output addresses --
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // out_ptr_addr
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // *out_ptr = value_lo
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // out_len_addr
    emitter.instruction("mov QWORD PTR [r10], rsi");                            // *out_len = value_hi

    emitter.instruction("mov eax, 1");                                          // prepare runtime result value
    emitter.instruction("jmp __rt_gsco_done_x86");                              // continue at target label

    emitter.label("__rt_gsco_miss_x86");
    emitter.instruction("xor eax, eax");                                        // clear register value
    emitter.label("__rt_gsco_done_x86");
    emitter.instruction("add rsp, 48");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
