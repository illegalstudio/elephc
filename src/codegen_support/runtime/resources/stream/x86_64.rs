//! Purpose:
//! Emits Linux x86_64 helpers that adopt and resolve opaque stream resources.
//! Minimal backend cleanup covers direct descriptors, popen pipes, and directories.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::stream::emit_stream_resources()`.
//!
//! Key details:
//! - Standard streams resolve through persistent generation-one registry slots.
//! - Adoption closes an acquired descriptor whenever publication cannot complete.

use super::super::layout::{
    RESOURCE_FLAG_OWNS_STATE, RESOURCE_KIND_STREAM, RESOURCE_STATUS_CLOSING,
    RESOURCE_STATUS_LIVE, SLOT_KIND_OFFSET, SLOT_STATE_PTR_OFFSET, SLOT_STATUS_OFFSET,
    STREAM_BACKEND_AUX_OFFSET, STREAM_BACKEND_DIRECTORY, STREAM_BACKEND_FD,
    STREAM_BACKEND_GLOB_DIRECTORY, STREAM_BACKEND_KIND_OFFSET,
    STREAM_BACKEND_PHAR_WRITE, STREAM_BACKEND_POPEN, STREAM_BACKEND_USER_DIRECTORY,
    STREAM_BACKEND_USER_WRAPPER, STREAM_CHUNK_SIZE_OFFSET, STREAM_CONNECT_HOST_LEN_OFFSET,
    STREAM_CONNECT_HOST_PTR_OFFSET, STREAM_CONTEXT_HANDLE_OFFSET, STREAM_EOF_OFFSET, STREAM_FD_OFFSET,
    STREAM_FILTERED_BUF_CAP_OFFSET, STREAM_FILTERED_BUF_LEN_OFFSET, STREAM_FILTERED_BUF_POS_OFFSET,
    STREAM_FILTERED_BUF_PTR_OFFSET, STREAM_FILTERED_FLUSHED_OFFSET,
    STREAM_MODE_LEN_OFFSET, STREAM_MODE_PTR_OFFSET,
    STREAM_OWNERSHIP_FLAGS_OFFSET, STREAM_READ_FILTER_HEAD_OFFSET, STREAM_STATE_SIZE,
    STREAM_TLS_SESSION_OFFSET,
    STREAM_URI_LEN_OFFSET,
    STREAM_URI_PTR_OFFSET,
};
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;

/// Emits every Linux x86_64 stream-resource helper.
pub(super) fn emit_stream_resources_x86_64(emitter: &mut Emitter) {
    emit_stream_adopt_fd(emitter);
    emit_stream_state(emitter);
    emit_stream_fd(emitter);
    emit_stream_eof_get(emitter);
    emit_stream_eof_set(emitter);
    emit_stream_tls_session(emitter);
    emit_stream_set_tls_session(emitter);
    emit_stream_chunk_size(emitter);
    emit_stream_set_chunk_size(emitter);
    emit_stream_attach_context(emitter);
    emit_stream_close_backend(emitter);
    emit_stream_destroy_state(emitter);
}

/// Emits context attachment with one retained owner stored on the StreamState.
fn emit_stream_attach_context(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: attach a retained context to an opaque stream ---");
    emitter.label_global("__rt_stream_attach_context");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable attachment frame
    emitter.instruction("sub rsp, 32");                                         // reserve stream, context, and state spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the selected context handle
    emitter.instruction("call __rt_stream_state");                              // resolve the authoritative StreamState
    emitter.instruction("test rax, rax");                                       // did stream lookup succeed?
    emitter.instruction("jz __rt_stream_attach_context_fail");                  // reject stale or non-stream handles
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve StreamState across context validation
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the selected context handle
    emitter.instruction("call __rt_context_state");                             // validate that the selected handle is a live context
    emitter.instruction("test rax, rax");                                       // did context lookup succeed?
    emitter.instruction("jz __rt_stream_attach_context_fail");                  // reject stale or non-context handles
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the validated context handle
    emitter.instruction("call __rt_resource_retain");                           // acquire the StreamState-owned context reference
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload StreamState for atomic replacement
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [r9 + {}]", STREAM_CONTEXT_HANDLE_OFFSET
    ));                                                                         // detach the previously attached context handle
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], rax", STREAM_CONTEXT_HANDLE_OFFSET
    ));                                                                         // publish the newly retained context handle
    emitter.instruction("test r10, r10");                                       // was another context owner attached?
    emitter.instruction("jz __rt_stream_attach_context_success");               // skip release when the field was empty
    emitter.instruction("mov rdi, r10");                                        // pass the detached context owner to registry release
    emitter.instruction("call __rt_resource_release");                          // release the replaced context reference
    emitter.label("__rt_stream_attach_context_success");
    emitter.instruction("mov eax, 1");                                          // report successful context attachment
    emitter.instruction("jmp __rt_stream_attach_context_done");                 // join the common attachment epilogue
    emitter.label("__rt_stream_attach_context_fail");
    emitter.instruction("xor eax, eax");                                        // report that no context was attached
    emitter.label("__rt_stream_attach_context_done");
    emitter.instruction("add rsp, 32");                                         // release attachment scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the attachment status
}

/// Emits descriptor adoption into an owned 320-byte x86_64 stream state.
fn emit_stream_adopt_fd(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: adopt an OS descriptor as an opaque stream ---");
    emitter.label_global("__rt_stream_adopt_fd");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable adoption frame
    emitter.instruction("sub rsp, 64");                                         // reserve aligned adoption scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the acquired OS descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the stream backend kind
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve stream ownership flags
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve backend-specific auxiliary state
    emitter.instruction(&format!(
        "mov eax, {}", STREAM_STATE_SIZE
    ));                                                                         // request one stable stream-state allocation
    emitter.instruction("call __rt_heap_alloc");                                // allocate the stream state
    emitter.instruction("test rax, rax");                                       // did state allocation succeed?
    emitter.instruction("jz __rt_stream_adopt_fd_close_fail");                  // close the descriptor after allocation failure
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the owned stream-state pointer
    emitter.instruction("mov r10, rax");                                        // start zeroing at the stream-state base
    emitter.instruction(&format!(
        "mov ecx, {}", STREAM_STATE_SIZE / 8
    ));                                                                         // count stream-state machine words
    emitter.label("__rt_stream_adopt_fd_zero");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear one stream-state word
    emitter.instruction("add r10, 8");                                          // advance the state zeroing cursor
    emitter.instruction("sub rcx, 1");                                          // consume one zeroed word
    emitter.instruction("jnz __rt_stream_adopt_fd_zero");                       // initialize every stream-state field
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the backend kind
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], r10",
        STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // publish the backend kind
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the acquired OS descriptor
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], r10", STREAM_FD_OFFSET
    ));                                                                         // store the backend descriptor
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload stream ownership flags
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], r10",
        STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // publish stream ownership flags
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload backend-specific auxiliary state
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], r10", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // publish the backend owner independently of its descriptor
    emitter.instruction(&format!(
        "mov edi, {}", RESOURCE_KIND_STREAM
    ));                                                                         // allocate a stream-kind registry slot
    emitter.instruction("mov rsi, rax");                                        // pass the stable stream-state pointer
    emitter.instruction(&format!(
        "mov edx, {}", RESOURCE_FLAG_OWNS_STATE
    ));                                                                         // make the registry own state storage
    emitter.instruction("call __rt_resource_alloc");                            // publish the opaque stream handle
    emitter.instruction("test rax, rax");                                       // did registry publication succeed?
    emitter.instruction("jnz __rt_stream_adopt_fd_done");                       // return the successfully published handle
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload unpublished state storage
    emitter.instruction("call __rt_heap_free");                                 // release state storage after registry failure
    emitter.label("__rt_stream_adopt_fd_close_fail");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the backend kind for typed rollback
    emitter.instruction(&format!("cmp r10, {}", STREAM_BACKEND_POPEN));         // does the failed adoption own a process pipe?
    emitter.instruction("jne __rt_stream_adopt_fd_check_dir");                  // inspect the remaining typed backend owners
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the owning FILE* for pclose
    emitter.instruction("test rdi, rdi");                                       // was a FILE* acquired?
    emitter.instruction("jz __rt_stream_adopt_fd_fail");                        // a missing FILE* leaves nothing safe to reap
    emitter.instruction("call __rt_pclose");                                    // close the pipe and reap its child after publication failure
    emitter.instruction("jmp __rt_stream_adopt_fd_fail");                       // never close the descriptor a second time
    emitter.label("__rt_stream_adopt_fd_check_dir");
    emitter.instruction(&format!("cmp r10, {}", STREAM_BACKEND_DIRECTORY));     // does rollback own a native DIR*?
    emitter.instruction("jne __rt_stream_adopt_fd_check_glob");                 // inspect glob and synthetic backends next
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the owning native DIR*
    emitter.instruction("test rdi, rdi");                                       // was native directory ownership acquired?
    emitter.instruction("jz __rt_stream_adopt_fd_fail");                        // absent native ownership needs no cleanup
    emitter.bl_c("closedir");
    emitter.instruction("jmp __rt_stream_adopt_fd_fail");                       // libc closedir also consumed the descriptor
    emitter.label("__rt_stream_adopt_fd_check_glob");
    emitter.instruction(&format!(
        "cmp r10, {}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // does rollback own a glob iterator state?
    emitter.instruction("jne __rt_stream_adopt_fd_check_user");                 // inspect userspace and direct descriptors next
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the owned glob iterator state
    emitter.instruction("test r11, r11");                                       // was glob state allocated?
    emitter.instruction("jz __rt_stream_adopt_fd_close_plain");                 // without state only the synthetic descriptor remains
    emitter.instruction("lea rdi, [r11 + 24]");                                 // pass the embedded glob_t to globfree
    emitter.instruction("call globfree");                                       // release libc-owned glob children
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the synthetic glob descriptor
    emitter.instruction("test rdi, rdi");                                       // is a descriptor available to close?
    emitter.instruction("js __rt_stream_adopt_fd_glob_free");                   // skip close for an absent descriptor
    emitter.instruction("call close");                                          // close the synthetic glob descriptor
    emitter.label("__rt_stream_adopt_fd_glob_free");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the glob-state allocation itself
    emitter.instruction("call __rt_heap_free");                                 // release the auxiliary glob owner
    emitter.instruction("jmp __rt_stream_adopt_fd_fail");                       // typed glob rollback is complete
    emitter.label("__rt_stream_adopt_fd_check_user");
    emitter.instruction(&format!(
        "cmp r10, {}", STREAM_BACKEND_USER_WRAPPER
    ));                                                                         // does rollback own a userspace stream wrapper?
    emitter.instruction("jne __rt_stream_adopt_fd_check_user_dir");             // inspect userspace directory ownership next
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the synthetic wrapper handle
    emitter.instruction("call __rt_user_wrapper_fclose");                       // invoke stream_close for failed registry publication
    emitter.instruction("jmp __rt_stream_adopt_fd_fail");                       // wrapper rollback consumed its backend
    emitter.label("__rt_stream_adopt_fd_check_user_dir");
    emitter.instruction(&format!(
        "cmp r10, {}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // does rollback own a userspace directory wrapper?
    emitter.instruction("jne __rt_stream_adopt_fd_check_phar");                 // inspect buffered Phar ownership next
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the synthetic directory handle
    emitter.instruction("call __rt_user_wrapper_dir_closedir");                 // invoke dir_closedir for failed publication
    emitter.instruction("jmp __rt_stream_adopt_fd_fail");                       // wrapper rollback consumed its backend
    emitter.label("__rt_stream_adopt_fd_check_phar");
    emitter.instruction(&format!("cmp r10, {}", STREAM_BACKEND_PHAR_WRITE));    // does rollback own buffered Phar output?
    emitter.instruction("jne __rt_stream_adopt_fd_close_plain");                // direct descriptors retain ordinary close rollback
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the synthetic Phar descriptor
    emitter.instruction("call __rt_phar_write_finalize");                       // flush and close buffered Phar output
    emitter.instruction("jmp __rt_stream_adopt_fd_fail");                       // Phar rollback consumed its backend
    emitter.label("__rt_stream_adopt_fd_close_plain");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the already-acquired descriptor
    emitter.instruction("test rdi, rdi");                                       // was a non-negative descriptor acquired?
    emitter.instruction("js __rt_stream_adopt_fd_fail");                        // do not close a negative failure sentinel
    emitter.instruction("call close");                                          // close the descriptor after adoption failure
    emitter.label("__rt_stream_adopt_fd_fail");
    emitter.instruction("xor eax, eax");                                        // return the invalid opaque handle
    emitter.label("__rt_stream_adopt_fd_done");
    emitter.instruction("add rsp, 64");                                         // release adoption scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the opaque stream handle or zero
}

/// Emits typed stream-state lookup for Live and Closing x86_64 resources.
fn emit_stream_state(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve an opaque stream state ---");
    emitter.label_global("__rt_stream_state");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable lookup frame
    emitter.instruction("call __rt_resource_lookup_any");                       // validate and resolve the opaque handle
    emitter.instruction("test rax, rax");                                       // did lookup resolve a resource slot?
    emitter.instruction("jz __rt_stream_state_fail");                           // reject invalid or stale resources
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_KIND_OFFSET, RESOURCE_KIND_STREAM
    ));                                                                         // is the slot a stream?
    emitter.instruction("jne __rt_stream_state_fail");                          // reject contexts, filters, and other resources
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the lifecycle state
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_STATUS_LIVE
    ));                                                                         // accept ordinary live stream operations
    emitter.instruction("je __rt_stream_state_load");                           // resolve the stable live stream state
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_STATUS_CLOSING
    ));                                                                         // allow close paths to resolve the backend
    emitter.instruction("jne __rt_stream_state_fail");                          // reject closed and free stream slots
    emitter.label("__rt_stream_state_load");
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // return the stream-state pointer
    emitter.instruction("jmp __rt_stream_state_done");                          // join the helper epilogue
    emitter.label("__rt_stream_state_fail");
    emitter.instruction("xor eax, eax");                                        // return null for invalid stream resources
    emitter.label("__rt_stream_state_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the stream-state pointer or null
}

/// Emits descriptor lookup through the opaque registry.
fn emit_stream_fd(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve an opaque stream OS descriptor ---");
    emitter.label_global("__rt_stream_fd");
    emitter.instruction("mov r10, rdi");                                        // copy the stream value for generation inspection
    emitter.instruction("shr r10, 32");                                         // isolate the opaque-handle generation word
    emitter.instruction("jz __rt_stream_fd_raw");                               // preserve transitional raw descriptors and wrapper handles
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable descriptor frame
    emitter.instruction("call __rt_stream_state");                              // resolve Live or Closing stream state
    emitter.instruction("test rax, rax");                                       // did state lookup succeed?
    emitter.instruction("jz __rt_stream_fd_fail");                              // reject invalid or descriptor-less handles
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]", STREAM_FD_OFFSET
    ));                                                                         // return the backend OS descriptor
    emitter.instruction("jmp __rt_stream_fd_done");                             // join the helper epilogue
    emitter.label("__rt_stream_fd_fail");
    emitter.instruction("mov rax, -1");                                         // report an unavailable descriptor
    emitter.label("__rt_stream_fd_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return a descriptor or minus one
    emitter.label("__rt_stream_fd_raw");
    emitter.instruction("mov rax, rdi");                                        // return a generation-zero legacy descriptor unchanged
    emitter.instruction("ret");                                                 // return to the compatibility caller
}

/// Emits EOF lookup keyed by the stable x86_64 stream state.
fn emit_stream_eof_get(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: read opaque stream EOF state ---");
    emitter.label_global("__rt_stream_eof_get");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("call __rt_stream_state");                              // resolve the stable stream state from the opaque handle
    emitter.instruction("test rax, rax");                                       // did lookup produce an authoritative state?
    emitter.instruction("jz __rt_stream_eof_get_fail");                         // invalid or legacy handles have no state-owned EOF bit
    // See the AArch64 counterpart: a filtered stream is at EOF only once the reader has seen
    // everything, so buffered output and an owed `$closing` dispatch both hold `feof()` false.
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], 0", STREAM_READ_FILTER_HEAD_OFFSET
    ));                                                                         // does the stream carry a read filter?
    emitter.instruction("je __rt_stream_eof_get_backend");                      // unfiltered: the backend's state is the answer
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]", STREAM_FILTERED_BUF_LEN_OFFSET
    ));                                                                         // filtered bytes held
    emitter.instruction(&format!(
        "cmp r10, QWORD PTR [rax + {}]", STREAM_FILTERED_BUF_POS_OFFSET
    ));                                                                         // is anything still owed to the reader?
    emitter.instruction("jne __rt_stream_eof_get_more");                        // yes: the stream is not finished
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], 0", STREAM_FILTERED_FLUSHED_OFFSET
    ));                                                                         // has the closing dispatch run?
    emitter.instruction("je __rt_stream_eof_get_more");                         // not yet: the filter may still emit
    emitter.label("__rt_stream_eof_get_backend");
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], 0", STREAM_EOF_OFFSET
    ));                                                                         // test the stream-owned EOF word
    emitter.instruction("setne al");                                            // normalize any non-zero state to PHP true
    emitter.instruction("movzx eax, al");                                       // widen the strict EOF predicate
    emitter.instruction("jmp __rt_stream_eof_get_done");                        // join the common helper epilogue
    emitter.label("__rt_stream_eof_get_more");
    emitter.instruction("xor eax, eax");                                        // filtered output is still owed to the reader
    emitter.instruction("jmp __rt_stream_eof_get_done");                        // join the common helper epilogue
    emitter.label("__rt_stream_eof_get_fail");
    emitter.instruction("xor eax, eax");                                        // report false when no authoritative state exists
    emitter.label("__rt_stream_eof_get_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the state-owned EOF predicate
}

/// Emits `__rt_stream_tls_session(handle) -> session`, zero when the transport is
/// plain or the handle is not a live stream.
///
/// Returning zero for a non-handle is what lets the write path accept either a
/// handle or a raw descriptor during the migration: a raw descriptor simply has
/// no state, hence no session, hence the plain-write path.
fn emit_stream_tls_session(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: read the stream-owned TLS session handle ---");
    emitter.label_global("__rt_stream_tls_session");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("call __rt_stream_state");                              // resolve the stable stream state from the opaque handle
    emitter.instruction("test rax, rax");                                       // did lookup produce an authoritative state?
    emitter.instruction("jz __rt_stream_tls_session_none_x86");                 // raw descriptors and stale handles carry no session
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]", STREAM_TLS_SESSION_OFFSET
    ));                                                                         // load the attached TLS session handle
    emitter.instruction("jmp __rt_stream_tls_session_done_x86");                // join the common helper epilogue
    emitter.label("__rt_stream_tls_session_none_x86");
    emitter.instruction("xor eax, eax");                                        // report a plain, unencrypted transport
    emitter.label("__rt_stream_tls_session_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the attached session handle
}

/// Emits `__rt_stream_set_tls_session(handle, session) -> ok`.
fn emit_stream_set_tls_session(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: attach a TLS session to an opaque stream ---");
    emitter.label_global("__rt_stream_set_tls_session");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable attachment frame
    emitter.instruction("sub rsp, 16");                                         // reserve the session slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the session across state lookup
    emitter.instruction("call __rt_stream_state");                              // resolve the authoritative StreamState
    emitter.instruction("test rax, rax");                                       // did lookup produce an authoritative state?
    emitter.instruction("jz __rt_stream_set_tls_session_fail_x86");             // reject stale or non-stream handles
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the session handle
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], rcx", STREAM_TLS_SESSION_OFFSET
    ));                                                                         // publish the session on the stream state
    emitter.instruction("mov eax, 1");                                          // report successful attachment
    emitter.instruction("jmp __rt_stream_set_tls_session_done_x86");            // join the common helper epilogue
    emitter.label("__rt_stream_set_tls_session_fail_x86");
    emitter.instruction("xor eax, eax");                                        // report that no session was attached
    emitter.label("__rt_stream_set_tls_session_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard the attachment scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the attachment status
}

/// Emits EOF replacement keyed by the stable x86_64 stream state.
fn emit_stream_eof_set(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: replace opaque stream EOF state ---");
    emitter.label_global("__rt_stream_eof_set");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned storage for the requested state
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the requested EOF state
    emitter.instruction("call __rt_stream_state");                              // resolve the stable stream state from the opaque handle
    emitter.instruction("test rax, rax");                                       // did lookup produce an authoritative state?
    emitter.instruction("jz __rt_stream_eof_set_fail");                         // ignore invalid, closed, and legacy handles
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // normalize the state before publication
    emitter.instruction("setne r10b");                                          // keep the stored state strictly zero or one
    emitter.instruction("movzx r10, r10b");                                     // widen the normalized EOF word
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], r10", STREAM_EOF_OFFSET
    ));                                                                         // publish EOF on this stream state only
    // See the AArch64 counterpart: clearing EOF is a seek saying reading starts again, so the
    // filtered-read buffer discards what the previous pass produced. The allocation is kept.
    emitter.instruction("test r10, r10");
    emitter.instruction("jnz __rt_stream_eof_set_keep_buffer_x86");             // setting EOF leaves the pending bytes alone
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], 0", STREAM_FILTERED_BUF_LEN_OFFSET
    ));                                                                         // no filtered bytes carry across the seek
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], 0", STREAM_FILTERED_BUF_POS_OFFSET
    ));                                                                         // and the read cursor restarts
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], 0", STREAM_FILTERED_FLUSHED_OFFSET
    ));                                                                         // the new pass earns its own closing dispatch
    emitter.label("__rt_stream_eof_set_keep_buffer_x86");
    emitter.instruction("mov eax, 1");                                          // report that the state was updated
    emitter.instruction("jmp __rt_stream_eof_set_done");                        // join the common helper epilogue
    emitter.label("__rt_stream_eof_set_fail");
    emitter.instruction("xor eax, eax");                                        // report that no authoritative state was updated
    emitter.label("__rt_stream_eof_set_done");
    emitter.instruction("add rsp, 16");                                         // release requested-state storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the update predicate
}

/// Emits x86_64 state-owned chunk-size lookup with the existing read-loop fallback.
fn emit_stream_chunk_size(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve opaque stream chunk size ---");
    emitter.label_global("__rt_stream_chunk_size");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable lookup frame
    emitter.instruction("call __rt_stream_state");                              // resolve the live opaque stream state
    emitter.instruction("test rax, rax");                                       // did the handle resolve to an open stream?
    emitter.instruction("jz __rt_stream_chunk_size_default");                   // invalid handles use the defensive read-loop fallback
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]",
        STREAM_CHUNK_SIZE_OFFSET
    ));                                                                         // load the configured state-owned chunk size
    emitter.instruction("test rax, rax");                                       // was an explicit size configured?
    emitter.instruction("jnz __rt_stream_chunk_size_done");                     // return the configured size
    emitter.label("__rt_stream_chunk_size_default");
    emitter.instruction("mov eax, 4096");                                       // preserve the current read-loop default
    emitter.label("__rt_stream_chunk_size_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the effective read-loop chunk size
}

/// Emits x86_64 chunk-size replacement through the authoritative opaque stream state.
fn emit_stream_set_chunk_size(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: replace opaque stream chunk size ---");
    emitter.label_global("__rt_stream_set_chunk_size");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned storage for the requested size
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the requested chunk size
    emitter.instruction("call __rt_stream_state");                              // resolve the live opaque stream state
    emitter.instruction("test rax, rax");                                       // did the handle resolve to an open stream?
    emitter.instruction("jz __rt_stream_set_chunk_size_fail");                  // reject stale, closed, or non-stream handles
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]",
        STREAM_CHUNK_SIZE_OFFSET
    ));                                                                         // load the previous state-owned chunk size
    emitter.instruction("test r10, r10");                                       // was a custom size configured?
    emitter.instruction("jnz __rt_stream_set_chunk_size_have_old");             // preserve the configured previous size
    emitter.instruction("mov r10, 8192");                                       // materialize PHP's default stream chunk size
    emitter.label("__rt_stream_set_chunk_size_have_old");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the requested chunk size
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], r11",
        STREAM_CHUNK_SIZE_OFFSET
    ));                                                                         // publish the new size in the authoritative StreamState
    emitter.instruction("mov rax, r10");                                        // return the previous chunk size
    emitter.instruction("mov edx, 1");                                          // report successful live-stream resolution
    emitter.instruction("jmp __rt_stream_set_chunk_size_done");                 // join the helper epilogue
    emitter.label("__rt_stream_set_chunk_size_fail");
    emitter.instruction("xor eax, eax");                                        // invalid handles have no previous chunk size
    emitter.instruction("xor edx, edx");                                        // report failed stream-state resolution
    emitter.label("__rt_stream_set_chunk_size_done");
    emitter.instruction("add rsp, 16");                                         // release helper scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the previous size and success flag
}

/// Emits minimal exact-once x86_64 backend closure for fd, popen, and directory streams.
fn emit_stream_close_backend(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: minimally close an opaque stream backend ---");
    emitter.label_global("__rt_stream_close_backend");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable close frame
    emitter.instruction("sub rsp, 48");                                         // reserve aligned close scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("call __rt_resource_mark_closing");                     // publish Closing before backend work
    emitter.instruction("test rax, rax");                                       // did this call own the close transition?
    emitter.instruction("jz __rt_stream_close_backend_fail");                   // reject stale, closed, or re-entrant closes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // resolve the Closing stream state
    emitter.instruction("test rax, rax");                                       // is the stable stream state available?
    emitter.instruction("jz __rt_stream_close_backend_mark");                   // still publish Closed for an absent state
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve StreamState for typed backend destructors
    // -- close any attached TLS session before the backend descriptor goes away --
    // Closing here rather than in the fclose lowering means every path that
    // destroys a stream sends close_notify, not just an explicit fclose().
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]", STREAM_TLS_SESSION_OFFSET
    ));                                                                         // load the attached TLS session handle
    emitter.instruction("test r10, r10");                                       // is a TLS session attached?
    emitter.instruction("jz __rt_stream_close_backend_tls_done_x86");           // plain transports have nothing to shut down
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], 0", STREAM_TLS_SESSION_OFFSET
    ));                                                                         // detach before the call so a re-entrant close cannot double-free
    abi::emit_load_symbol_to_reg(emitter, "r11", "_elephc_tls_close_fn", 0);    // load the published TLS close function pointer
    emitter.instruction("test r11, r11");                                       // is the TLS backend linked into this program?
    emitter.instruction("jz __rt_stream_close_backend_tls_done_x86");           // no TLS backend: nothing to shut down
    emitter.instruction("mov rdi, r10");                                        // pass the session handle to the close helper
    emitter.instruction("call r11");                                            // send close_notify and free the session
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload StreamState clobbered by the close call
    emitter.label("__rt_stream_close_backend_tls_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the stream backend kind
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // load the backend descriptor or handle
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // load backend-specific ownership independently of the descriptor
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // preserve auxiliary ownership across dispatch
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve the backend handle
    emitter.instruction(&format!("cmp r10, {}", STREAM_BACKEND_FD));            // direct OS descriptor backend?
    emitter.instruction("je __rt_stream_close_backend_fd");                     // close direct file and socket descriptors
    emitter.instruction(&format!(
        "cmp r10, {}", STREAM_BACKEND_USER_WRAPPER
    ));                                                                         // userspace stream-wrapper backend?
    emitter.instruction("je __rt_stream_close_backend_user_wrapper");           // dispatch the wrapper's stream_close callback
    emitter.instruction(&format!("cmp r10, {}", STREAM_BACKEND_POPEN));         // popen pipe backend?
    emitter.instruction("je __rt_stream_close_backend_popen");                  // close and reap popen resources
    emitter.instruction(&format!(
        "cmp r10, {}", STREAM_BACKEND_DIRECTORY
    ));                                                                         // native directory stream backend?
    emitter.instruction("je __rt_stream_close_backend_dir");                    // close directory resources
    emitter.instruction(&format!(
        "cmp r10, {}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // glob directory stream backend?
    emitter.instruction("je __rt_stream_close_backend_dir");                    // close the typed glob iterator
    emitter.instruction(&format!(
        "cmp r10, {}", STREAM_BACKEND_PHAR_WRITE
    ));                                                                         // buffered Phar write backend?
    emitter.instruction("je __rt_stream_close_backend_phar");                   // finalize buffered Phar output
    emitter.instruction(&format!(
        "cmp r10, {}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // userspace directory-wrapper backend?
    emitter.instruction("je __rt_stream_close_backend_user_dir");               // dispatch the wrapper's dir_closedir callback
    emitter.instruction("jmp __rt_stream_close_backend_mark");                  // unknown backends currently have no close hook
    emitter.label("__rt_stream_close_backend_fd");
    emitter.instruction("mov rdi, r11");                                        // pass the owned descriptor to libc close
    emitter.instruction("test rdi, rdi");                                       // skip absent descriptors
    emitter.instruction("js __rt_stream_close_backend_mark");                   // an absent descriptor needs no syscall
    emitter.instruction("call close");                                          // close the native file or socket descriptor
    emitter.instruction("jmp __rt_stream_close_backend_mark");                  // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_user_wrapper");
    emitter.instruction("mov rdi, r11");                                        // pass the synthetic wrapper handle to stream_close dispatch
    emitter.instruction("call __rt_user_wrapper_fclose");                       // invoke the userspace wrapper close callback exactly once
    emitter.instruction("jmp __rt_stream_close_backend_mark");                  // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_popen");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload StreamState before detaching process ownership
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], 0", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // prevent any re-entrant process close from reusing FILE*
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the owning FILE* to pclose
    emitter.instruction("call __rt_pclose");                                    // close the FILE pointer and reap its child
    emitter.instruction("jmp __rt_stream_close_backend_mark");                  // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_dir");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass authoritative StreamState to directory cleanup
    emitter.instruction("call __rt_closedir");                                  // close the typed native or glob iterator
    emitter.instruction("jmp __rt_stream_close_backend_mark");                  // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_phar");
    emitter.instruction("mov rdi, r11");                                        // pass the synthetic Phar descriptor to its finalizer
    emitter.instruction("call __rt_phar_write_finalize");                       // flush and close the buffered Phar write stream
    emitter.instruction("jmp __rt_stream_close_backend_mark");                  // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_user_dir");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass authoritative StreamState to wrapper directory cleanup
    emitter.instruction("call __rt_closedir");                                  // invoke userspace directory close exactly once
    emitter.label("__rt_stream_close_backend_mark");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_resource_mark_closed");                      // publish the terminal Closed state
    emitter.instruction("mov eax, 1");                                          // report an exact-once close attempt
    emitter.instruction("jmp __rt_stream_close_backend_done");                  // join the helper epilogue
    emitter.label("__rt_stream_close_backend_fail");
    emitter.instruction("xor eax, eax");                                        // report invalid or already-closing resources
    emitter.label("__rt_stream_close_backend_done");
    emitter.instruction("add rsp, 48");                                         // release close scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the close status
}

/// Emits Linux x86_64 owned StreamState teardown in child-before-parent order.
fn emit_stream_destroy_state(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: destroy an owned stream state ---");
    emitter.label_global("__rt_stream_destroy_state");
    emitter.instruction("test rax, rax");                                       // do null stream states own any storage?
    emitter.instruction("jz __rt_stream_destroy_state_done");                   // no, return without entering a teardown frame
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable teardown frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned StreamState storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve StreamState across nested releases
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload StreamState for the attached filter chains
    emitter.instruction("call __rt_stream_close_filter_chains");                // PHP invalidates filter resources when their stream closes
    // Reload: see the AArch64 counterpart — the chain teardown runs user `onClose()`
    // hooks now, so rax no longer holds the StreamState on return.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload StreamState after the chain teardown
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // load the owned URI allocation
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload StreamState before detaching URI ownership
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_URI_PTR_OFFSET
    ));                                                                         // detach URI before potentially re-entrant heap cleanup
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_URI_LEN_OFFSET
    ));                                                                         // clear the detached URI length
    emitter.instruction("call __rt_heap_free_safe");                            // release owned URI storage and ignore static standard-stream literals
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload StreamState for connect-host teardown
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r10 + {}]", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // load the owned TLS-default host allocation
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // detach host ownership before nested cleanup
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_CONNECT_HOST_LEN_OFFSET
    ));                                                                         // clear the detached host length
    emitter.instruction("call __rt_heap_free_safe");                            // release owned host storage when present
    // See the AArch64 counterpart: the filtered-read buffer belongs to the state, so it is
    // released here beside the URI and host rather than at backend close.
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload StreamState for filtered-buffer teardown
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r10 + {}]", STREAM_FILTERED_BUF_PTR_OFFSET
    ));                                                                         // load the buffer a filtered read allocated
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_FILTERED_BUF_PTR_OFFSET
    ));                                                                         // detach before the free so a re-entrant teardown cannot double-free
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_FILTERED_BUF_LEN_OFFSET
    ));                                                                         // no bytes are held any more
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_FILTERED_BUF_CAP_OFFSET
    ));                                                                         // and no capacity remains
    emitter.instruction("call __rt_heap_free_safe");                            // release the filtered-read buffer when present
    // See the AArch64 counterpart: the reported mode string is persisted at open time like the URI.
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload StreamState for mode-string teardown
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r10 + {}]", STREAM_MODE_PTR_OFFSET
    ));                                                                         // load the owned reported-mode allocation
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_MODE_PTR_OFFSET
    ));                                                                         // detach before the free so a re-entrant teardown cannot double-free
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_MODE_LEN_OFFSET
    ));                                                                         // clear the detached mode length
    emitter.instruction("call __rt_heap_free_safe");                            // release owned mode storage when present
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload StreamState for context teardown
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r10 + {}]", STREAM_CONTEXT_HANDLE_OFFSET
    ));                                                                         // load the retained stream-context handle
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0", STREAM_CONTEXT_HANDLE_OFFSET
    ));                                                                         // detach context ownership before nested registry cleanup
    emitter.instruction("test rax, rax");                                       // is a context owner attached?
    emitter.instruction("jz __rt_stream_destroy_state_context_done");           // skip release when the field is empty
    emitter.instruction("mov rdi, rax");                                        // pass the attached context handle to registry release
    emitter.instruction("call __rt_resource_release");                          // release the StreamState-owned context reference
    emitter.label("__rt_stream_destroy_state_context_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // pass StreamState itself to the heap allocator
    emitter.instruction("call __rt_heap_free");                                 // release the owned 320-byte state allocation
    emitter.instruction("add rsp, 16");                                         // release teardown scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.label("__rt_stream_destroy_state_done");
    emitter.instruction("ret");                                                 // return after exact-once child and state teardown
}
