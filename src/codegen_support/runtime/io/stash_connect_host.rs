//! Purpose:
//! Emits handle-keyed storage and lookup for the transport host used by TLS defaults.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `stream_socket_client` after registry adoption and `stream_socket_enable_crypto`.
//!
//! Key details:
//! - Host ownership belongs to stable StreamState rather than a reusable OS descriptor.
//! - Replacement persists new bytes before releasing the previous allocation.
//! - Parsing strips an optional scheme and the final port separator.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_CONNECT_HOST_LEN_OFFSET, STREAM_CONNECT_HOST_PTR_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits handle-keyed connect-host replacement and lookup helpers.
pub fn emit_stash_connect_host(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stash_connect_host_linux_x86_64(emitter);
        emit_stream_get_connect_host_linux_x86_64(emitter);
        return;
    }
    emit_stash_connect_host_aarch64(emitter);
    emit_stream_get_connect_host_aarch64(emitter);
}

/// Emits `__rt_stash_connect_host(handle, address_ptr, address_len) -> handle`.
fn emit_stash_connect_host_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: replace handle-keyed stream connect host ---");
    emitter.label_global("__rt_stash_connect_host");
    emitter.instruction("sub sp, sp, #64");                                     // reserve host inputs, StreamState, and a saved frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #48");                                    // establish a stable host frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the opaque stream handle
    emitter.instruction("stp x1, x2, [sp, #8]");                                // preserve the full transport address
    emitter.instruction("bl __rt_stream_state");                                // resolve the stable StreamState
    emitter.instruction("cbz x0, __rt_stash_connect_host_done");                // stale handles cannot acquire host ownership
    emitter.instruction("str x0, [sp, #24]");                                   // preserve StreamState across parsing and persistence
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // restore address bytes for parsing
    emitter.instruction("mov x9, #0");                                          // default host start offset to the address start
    emitter.instruction("mov x10, #0");                                         // initialize the optional-scheme scan index
    emitter.label("__rt_sch_scheme");
    emitter.instruction("add x11, x10, #3");                                    // require three bytes for the scheme separator
    emitter.instruction("cmp x11, x2");                                         // do enough address bytes remain?
    emitter.instruction("b.gt __rt_sch_have_start");                            // no separator leaves the host at offset zero
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the candidate colon byte
    emitter.instruction("cmp w12, #58");                                        // is this the scheme colon?
    emitter.instruction("b.ne __rt_sch_scheme_next");                           // continue scanning after a non-colon byte
    emitter.instruction("add x13, x10, #1");                                    // address the first slash
    emitter.instruction("ldrb w12, [x1, x13]");                                 // load the first slash candidate
    emitter.instruction("cmp w12, #47");                                        // is the first separator byte a slash?
    emitter.instruction("b.ne __rt_sch_scheme_next");                           // reject a partial separator
    emitter.instruction("add x13, x10, #2");                                    // address the second slash
    emitter.instruction("ldrb w12, [x1, x13]");                                 // load the second slash candidate
    emitter.instruction("cmp w12, #47");                                        // is the second separator byte a slash?
    emitter.instruction("b.ne __rt_sch_scheme_next");                           // reject a partial separator
    emitter.instruction("add x9, x10, #3");                                     // host begins immediately after `://`
    emitter.instruction("b __rt_sch_have_start");                               // continue with the host/port split
    emitter.label("__rt_sch_scheme_next");
    emitter.instruction("add x10, x10, #1");                                    // advance the scheme scan index
    emitter.instruction("b __rt_sch_scheme");                                   // keep scanning for `://`
    emitter.label("__rt_sch_have_start");
    emitter.instruction("add x1, x1, x9");                                      // advance the source pointer past any scheme
    emitter.instruction("sub x2, x2, x9");                                      // keep only bytes following the optional scheme
    emitter.instruction("mov x10, #0");                                         // initialize the port-separator scan index
    emitter.instruction("mov x11, x2");                                         // default host length to the entire remainder
    emitter.label("__rt_sch_port");
    emitter.instruction("cmp x10, x2");                                         // have all remaining address bytes been scanned?
    emitter.instruction("b.ge __rt_sch_persist");                               // use the last colon position as the host length
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the next host byte
    emitter.instruction("cmp w12, #58");                                        // is this byte a colon?
    emitter.instruction("b.ne __rt_sch_port_next");                             // keep the previous host length for non-colon bytes
    emitter.instruction("mov x11, x10");                                        // remember the last colon as the port separator
    emitter.label("__rt_sch_port_next");
    emitter.instruction("add x10, x10, #1");                                    // advance the port-separator scan
    emitter.instruction("b __rt_sch_port");                                     // keep scanning for the last colon
    emitter.label("__rt_sch_persist");
    emitter.instruction("mov x2, x11");                                         // pass the parsed host byte length to persistence
    emitter.instruction("bl __rt_str_persist");                                 // duplicate host bytes into owned heap storage
    emitter.instruction("str x1, [sp, #32]");                                   // preserve the new owned host pointer
    emitter.instruction("str x2, [sp, #40]");                                   // preserve the parsed host byte length
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload StreamState for atomic ownership replacement
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // capture the previous owned host allocation
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the new owned host pointer
    emitter.instruction(&format!(
        "str x10, [x9, #{}]", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // publish the new TLS-default host pointer
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the parsed host byte length
    emitter.instruction(&format!(
        "str x10, [x9, #{}]", STREAM_CONNECT_HOST_LEN_OFFSET
    ));                                                                         // publish the new TLS-default host length
    emitter.instruction("bl __rt_heap_free_safe");                              // release only live heap-backed previous host storage
    emitter.label("__rt_stash_connect_host_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the original opaque stream handle
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #64");                                     // release host scratch storage
    emitter.instruction("ret");                                                 // return after replacement or stale-handle no-op
}

/// Emits the AArch64 connect-host getter.
fn emit_stream_get_connect_host_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: read handle-keyed stream connect host ---");
    emitter.label_global("__rt_stream_get_connect_host");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around StreamState lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_stream_state");                                // resolve the stable StreamState
    emitter.instruction("cbz x0, __rt_stream_get_connect_host_empty");          // stale handles have no transport host
    emitter.instruction(&format!(
        "ldr x1, [x0, #{}]", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // return the saved host pointer
    emitter.instruction(&format!(
        "ldr x2, [x0, #{}]", STREAM_CONNECT_HOST_LEN_OFFSET
    ));                                                                         // return the saved host byte length
    emitter.instruction("cmp x1, #0");                                          // is a host allocation attached?
    emitter.instruction("cset x0, ne");                                         // report whether the pointer/length pair is present
    emitter.instruction("b __rt_stream_get_connect_host_done");                 // join the common helper epilogue
    emitter.label("__rt_stream_get_connect_host_empty");
    emitter.instruction("mov x0, #0");                                          // report an absent connect host
    emitter.instruction("mov x1, #0");                                          // return a null host pointer
    emitter.instruction("mov x2, #0");                                          // return a zero host length
    emitter.label("__rt_stream_get_connect_host_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return presence plus the host pointer/length pair
}

/// Emits the Linux x86_64 connect-host replacement helper.
fn emit_stash_connect_host_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: replace handle-keyed stream connect host ---");
    emitter.label_global("__rt_stash_connect_host");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable host frame
    emitter.instruction("sub rsp, 48");                                         // reserve host inputs and StreamState storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the full address pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the full address byte length
    emitter.instruction("call __rt_stream_state");                              // resolve the stable StreamState
    emitter.instruction("test rax, rax");                                       // did registry lookup resolve a stream state?
    emitter.instruction("jz __rt_stash_connect_host_done_x86");                 // stale handles cannot acquire host ownership
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve StreamState across parsing and persistence
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore address bytes for parsing
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // restore the address byte length
    emitter.instruction("xor r9d, r9d");                                        // default host start offset to the address start
    emitter.instruction("xor r10d, r10d");                                      // initialize the optional-scheme scan index
    emitter.label("__rt_sch_scheme_x86");
    emitter.instruction("lea r11, [r10 + 3]");                                  // require three bytes for the scheme separator
    emitter.instruction("cmp r11, rdx");                                        // do enough address bytes remain?
    emitter.instruction("jg __rt_sch_have_start_x86");                          // no separator leaves the host at offset zero
    emitter.instruction("cmp BYTE PTR [rsi + r10], 58");                        // is this the scheme colon?
    emitter.instruction("jne __rt_sch_scheme_next_x86");                        // continue scanning after a non-colon byte
    emitter.instruction("cmp BYTE PTR [rsi + r10 + 1], 47");                    // is the first separator byte a slash?
    emitter.instruction("jne __rt_sch_scheme_next_x86");                        // reject a partial separator
    emitter.instruction("cmp BYTE PTR [rsi + r10 + 2], 47");                    // is the second separator byte a slash?
    emitter.instruction("jne __rt_sch_scheme_next_x86");                        // reject a partial separator
    emitter.instruction("lea r9, [r10 + 3]");                                   // host begins immediately after `://`
    emitter.instruction("jmp __rt_sch_have_start_x86");                         // continue with the host/port split
    emitter.label("__rt_sch_scheme_next_x86");
    emitter.instruction("add r10, 1");                                          // advance the scheme scan index
    emitter.instruction("jmp __rt_sch_scheme_x86");                             // keep scanning for `://`
    emitter.label("__rt_sch_have_start_x86");
    emitter.instruction("add rsi, r9");                                         // advance the source pointer past any scheme
    emitter.instruction("sub rdx, r9");                                         // keep only bytes following the optional scheme
    emitter.instruction("xor r10d, r10d");                                      // initialize the port-separator scan index
    emitter.instruction("mov r11, rdx");                                        // default host length to the entire remainder
    emitter.label("__rt_sch_port_x86");
    emitter.instruction("cmp r10, rdx");                                        // have all remaining address bytes been scanned?
    emitter.instruction("jge __rt_sch_persist_x86");                            // use the last colon position as the host length
    emitter.instruction("cmp BYTE PTR [rsi + r10], 58");                        // is the next host byte a colon?
    emitter.instruction("jne __rt_sch_port_next_x86");                          // keep the previous host length for non-colon bytes
    emitter.instruction("mov r11, r10");                                        // remember the last colon as the port separator
    emitter.label("__rt_sch_port_next_x86");
    emitter.instruction("add r10, 1");                                          // advance the port-separator scan
    emitter.instruction("jmp __rt_sch_port_x86");                               // keep scanning for the last colon
    emitter.label("__rt_sch_persist_x86");
    emitter.instruction("mov rax, rsi");                                        // pass the parsed host pointer to string persistence
    emitter.instruction("mov rdx, r11");                                        // pass the parsed host byte length
    emitter.instruction("call __rt_str_persist");                               // duplicate host bytes into owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the new owned host pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve the parsed host byte length
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload StreamState for atomic ownership replacement
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [r10 + {}]", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // capture the previous owned host allocation
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], rax", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // publish the new TLS-default host pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the parsed host byte length
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], rax", STREAM_CONNECT_HOST_LEN_OFFSET
    ));                                                                         // publish the new TLS-default host length
    emitter.instruction("mov rax, r11");                                        // pass the detached previous host to safe heap release
    emitter.instruction("call __rt_heap_free_safe");                            // release only live heap-backed previous host storage
    emitter.label("__rt_stash_connect_host_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the original opaque stream handle
    emitter.instruction("add rsp, 48");                                         // release host scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after replacement or stale-handle no-op
}

/// Emits the Linux x86_64 connect-host getter.
fn emit_stream_get_connect_host_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: read handle-keyed stream connect host ---");
    emitter.label_global("__rt_stream_get_connect_host");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer around StreamState lookup
    emitter.instruction("mov rbp, rsp");                                        // establish a stable getter frame
    emitter.instruction("call __rt_stream_state");                              // resolve the stable StreamState
    emitter.instruction("test rax, rax");                                       // did registry lookup resolve a stream state?
    emitter.instruction("jz __rt_stream_get_connect_host_empty_x86");           // stale handles have no transport host
    emitter.instruction(&format!(
        "mov rdx, QWORD PTR [rax + {}]", STREAM_CONNECT_HOST_LEN_OFFSET
    ));                                                                         // return the saved host byte length
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // return the saved host pointer
    emitter.instruction("jmp __rt_stream_get_connect_host_done_x86");           // join the common helper epilogue
    emitter.label("__rt_stream_get_connect_host_empty_x86");
    emitter.instruction("xor eax, eax");                                        // return a null host pointer
    emitter.instruction("xor edx, edx");                                        // return a zero host length
    emitter.label("__rt_stream_get_connect_host_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the host pointer/length pair
}
