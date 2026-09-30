//! Purpose:
//! Emits AArch64 helpers that adopt OS descriptors into stable stream states.
//! It also resolves opaque stream handles and performs minimal backend cleanup.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::stream::emit_stream_resources()`.
//!
//! Key details:
//! - Standard streams resolve through persistent generation-one registry slots.
//! - Adoption closes the descriptor if state or registry publication fails.

use crate::codegen_support::runtime::resources::layout::STREAM_DEFAULT_CHUNK_SIZE;
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
    STREAM_OWNERSHIP_FLAGS_OFFSET, STREAM_STATE_SIZE,
    STREAM_TLS_SESSION_OFFSET,
    STREAM_URI_LEN_OFFSET,
    STREAM_URI_PTR_OFFSET,
};
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;

/// Emits every AArch64 stream-resource helper.
pub(super) fn emit_stream_resources_aarch64(emitter: &mut Emitter) {
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
    emitter.instruction("sub sp, sp, #48");                                     // reserve stream, context, state, and saved-frame slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #32");                                    // establish a stable attachment frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve the stream and context handles
    emitter.instruction("bl __rt_stream_state");                                // resolve the authoritative StreamState
    emitter.instruction("cbz x0, __rt_stream_attach_context_fail");             // reject stale or non-stream handles
    emitter.instruction("str x0, [sp, #16]");                                   // preserve StreamState across context validation
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the selected context handle
    emitter.instruction("bl __rt_context_state");                               // validate that the selected handle is a live context
    emitter.instruction("cbz x0, __rt_stream_attach_context_fail");             // reject stale or non-context handles
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the validated context handle
    emitter.instruction("bl __rt_resource_retain");                             // acquire the StreamState-owned context reference
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload StreamState for atomic replacement
    emitter.instruction(&format!(
        "ldr x10, [x9, #{}]", STREAM_CONTEXT_HANDLE_OFFSET
    ));                                                                         // detach the previously attached context handle
    emitter.instruction(&format!(
        "str x0, [x9, #{}]", STREAM_CONTEXT_HANDLE_OFFSET
    ));                                                                         // publish the newly retained context handle
    emitter.instruction("cbz x10, __rt_stream_attach_context_success");         // skip release when no context was attached
    emitter.instruction("mov x0, x10");                                         // pass the detached context owner to registry release
    emitter.instruction("bl __rt_resource_release");                            // release the replaced context reference
    emitter.label("__rt_stream_attach_context_success");
    emitter.instruction("mov x0, #1");                                          // report successful context attachment
    emitter.instruction("b __rt_stream_attach_context_done");                   // join the common attachment epilogue
    emitter.label("__rt_stream_attach_context_fail");
    emitter.instruction("mov x0, #0");                                          // report that no context was attached
    emitter.label("__rt_stream_attach_context_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #48");                                     // release attachment scratch storage
    emitter.instruction("ret");                                                 // return the attachment status
}

/// Emits descriptor adoption into an owned 320-byte stream state.
fn emit_stream_adopt_fd(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: adopt an OS descriptor as an opaque stream ---");
    emitter.label_global("__rt_stream_adopt_fd");
    emitter.instruction("sub sp, sp, #64");                                     // reserve adoption state and a saved frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve descriptor and backend kind
    emitter.instruction("str x2, [sp, #16]");                                   // preserve stream ownership flags
    emitter.instruction("str x3, [sp, #32]");                                   // preserve backend-specific auxiliary state
    emitter.instruction(&format!(
        "mov x0, #{}", STREAM_STATE_SIZE
    ));                                                                         // request one stable stream-state allocation
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the stream state
    emitter.instruction("cbz x0, __rt_stream_adopt_fd_close_fail");             // close the acquired descriptor if allocation failed
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the owned stream-state pointer
    emitter.instruction("mov x9, x0");                                          // start zeroing at the stream-state base
    emitter.instruction(&format!(
        "mov x10, #{}", STREAM_STATE_SIZE / 8
    ));                                                                         // count stream-state machine words
    emitter.label("__rt_stream_adopt_fd_zero");
    emitter.instruction("str xzr, [x9], #8");                                   // clear one stream-state word
    emitter.instruction("subs x10, x10, #1");                                   // consume one zeroed word
    emitter.instruction("b.ne __rt_stream_adopt_fd_zero");                      // initialize every stream-state field
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the backend kind
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // publish the backend kind
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the acquired OS descriptor
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", STREAM_FD_OFFSET
    ));                                                                         // store the optional backend descriptor
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload stream ownership flags
    emitter.instruction(&format!(
        "str x9, [x0, #{}]",
        STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // publish stream ownership flags
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload backend-specific auxiliary state
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // publish the backend owner independently of its descriptor
    emitter.instruction("mov x1, x0");                                          // pass the stable state pointer to resource allocation
    emitter.instruction(&format!(
        "mov x0, #{}", RESOURCE_KIND_STREAM
    ));                                                                         // allocate a stream-kind registry slot
    emitter.instruction(&format!(
        "mov x2, #{}", RESOURCE_FLAG_OWNS_STATE
    ));                                                                         // make the registry own stream-state storage
    emitter.instruction("bl __rt_resource_alloc");                              // publish the opaque stream handle
    emitter.instruction("cbnz x0, __rt_stream_adopt_fd_done");                  // return the successfully published handle
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the unpublished stream-state allocation
    emitter.instruction("bl __rt_heap_free");                                   // release state storage after registry failure
    emitter.label("__rt_stream_adopt_fd_close_fail");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the backend kind for typed rollback
    emitter.instruction(&format!("cmp x9, #{}", STREAM_BACKEND_POPEN));         // does the failed adoption own a process pipe?
    emitter.instruction("b.ne __rt_stream_adopt_fd_check_dir");                 // inspect the remaining typed backend owners
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the owning FILE* for pclose
    emitter.instruction("cbz x0, __rt_stream_adopt_fd_fail");                   // a missing FILE* leaves nothing safe to reap
    emitter.instruction("bl __rt_pclose");                                      // close the pipe and reap its child after publication failure
    emitter.instruction("b __rt_stream_adopt_fd_fail");                         // never close the descriptor a second time
    emitter.label("__rt_stream_adopt_fd_check_dir");
    emitter.instruction(&format!("cmp x9, #{}", STREAM_BACKEND_DIRECTORY));     // does rollback own a native DIR*?
    emitter.instruction("b.ne __rt_stream_adopt_fd_check_glob");                // inspect glob and synthetic backends next
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the owning native DIR*
    emitter.instruction("cbz x0, __rt_stream_adopt_fd_fail");                   // absent native ownership needs no cleanup
    emitter.bl_c("closedir");
    emitter.instruction("b __rt_stream_adopt_fd_fail");                         // libc closedir also consumed the descriptor
    emitter.label("__rt_stream_adopt_fd_check_glob");
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // does rollback own a glob iterator state?
    emitter.instruction("b.ne __rt_stream_adopt_fd_check_user");                // inspect userspace and direct descriptors next
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the owned glob iterator state
    emitter.instruction("cbz x10, __rt_stream_adopt_fd_close_plain");           // without state only the synthetic descriptor remains
    emitter.instruction("add x0, x10, #24");                                    // pass the embedded glob_t to globfree
    emitter.bl_c("globfree");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the synthetic glob descriptor
    emitter.instruction("cmp x0, #0");                                          // is a descriptor available to close?
    emitter.instruction("b.lt __rt_stream_adopt_fd_glob_free");                 // skip close for an absent descriptor
    emitter.syscall(6);                                                         // close the synthetic glob descriptor
    emitter.label("__rt_stream_adopt_fd_glob_free");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the glob-state allocation itself
    emitter.instruction("bl __rt_heap_free");                                   // release the auxiliary glob owner
    emitter.instruction("b __rt_stream_adopt_fd_fail");                         // typed glob rollback is complete
    emitter.label("__rt_stream_adopt_fd_check_user");
    emitter.instruction(&format!("cmp x9, #{}", STREAM_BACKEND_USER_WRAPPER));  // does rollback own a userspace stream wrapper?
    emitter.instruction("b.ne __rt_stream_adopt_fd_check_user_dir");            // inspect userspace directory ownership next
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the synthetic wrapper handle
    // nothing was written before the publication failed
    emitter.instruction("mov x9, #0");
    abi::emit_symbol_address(emitter, "x10", "_uw_pending_flush");
    emitter.instruction("str x9, [x10]");
    emitter.instruction("bl __rt_user_wrapper_fclose");                         // invoke stream_close for failed registry publication
    emitter.instruction("b __rt_stream_adopt_fd_fail");                         // wrapper rollback consumed its backend
    emitter.label("__rt_stream_adopt_fd_check_user_dir");
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // does rollback own a userspace directory wrapper?
    emitter.instruction("b.ne __rt_stream_adopt_fd_check_phar");                // inspect buffered Phar ownership next
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the synthetic directory handle
    emitter.instruction("bl __rt_user_wrapper_dir_closedir");                   // invoke dir_closedir for failed publication
    emitter.instruction("b __rt_stream_adopt_fd_fail");                         // wrapper rollback consumed its backend
    emitter.label("__rt_stream_adopt_fd_check_phar");
    emitter.instruction(&format!("cmp x9, #{}", STREAM_BACKEND_PHAR_WRITE));    // does rollback own buffered Phar output?
    emitter.instruction("b.ne __rt_stream_adopt_fd_close_plain");               // direct descriptors retain ordinary close rollback
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the synthetic Phar descriptor
    emitter.instruction("bl __rt_phar_write_finalize");                         // flush and close buffered Phar output
    emitter.instruction("b __rt_stream_adopt_fd_fail");                         // Phar rollback consumed its backend
    emitter.label("__rt_stream_adopt_fd_close_plain");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the already-acquired OS descriptor
    emitter.instruction("cmp x0, #0");                                          // was a non-negative descriptor acquired?
    emitter.instruction("b.lt __rt_stream_adopt_fd_fail");                      // do not close a negative failure sentinel
    emitter.syscall(6);                                                         // close the descriptor after adoption failure
    emitter.label("__rt_stream_adopt_fd_fail");
    emitter.instruction("mov x0, #0");                                          // return the invalid opaque handle
    emitter.label("__rt_stream_adopt_fd_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #64");                                     // release adoption scratch storage
    emitter.instruction("ret");                                                 // return the opaque stream handle or zero
}

/// Emits typed stream-state lookup for Live and Closing resources.
fn emit_stream_state(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve an opaque stream state ---");
    emitter.label_global("__rt_stream_state");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around generic lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate and resolve the opaque handle
    emitter.instruction("cbz x0, __rt_stream_state_fail");                      // reject invalid or stale resources
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // load the registry resource kind
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_KIND_STREAM
    ));                                                                         // is the slot a stream?
    emitter.instruction("b.ne __rt_stream_state_fail");                         // reject contexts, filters, and other resources
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the stream lifecycle state
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // accept ordinary live stream operations
    emitter.instruction("b.eq __rt_stream_state_load");                         // resolve the stable live stream state
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_CLOSING
    ));                                                                         // allow close paths to resolve the backend
    emitter.instruction("b.ne __rt_stream_state_fail");                         // reject closed and free stream slots
    emitter.label("__rt_stream_state_load");
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // return the stable stream-state pointer
    emitter.instruction("b __rt_stream_state_done");                            // join the helper epilogue
    emitter.label("__rt_stream_state_fail");
    emitter.instruction("mov x0, #0");                                          // return null for invalid stream resources
    emitter.label("__rt_stream_state_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return the stream-state pointer or null
}

/// Emits OS-descriptor lookup through the opaque registry.
fn emit_stream_fd(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve an opaque stream OS descriptor ---");
    emitter.label_global("__rt_stream_fd");
    emitter.instruction("lsr x9, x0, #32");                                     // inspect the generation word carried by the stream value
    emitter.instruction("cbz x9, __rt_stream_fd_raw");                          // preserve transitional raw descriptors and wrapper handles
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around state lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_stream_state");                                // resolve Live or Closing stream state
    emitter.instruction("cbz x0, __rt_stream_fd_fail");                         // reject invalid or descriptor-less handles
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", STREAM_FD_OFFSET
    ));                                                                         // return the backend OS descriptor
    emitter.instruction("b __rt_stream_fd_restore");                            // restore the saved link register
    emitter.label("__rt_stream_fd_fail");
    emitter.instruction("mov x0, #-1");                                         // report an unavailable descriptor
    emitter.label("__rt_stream_fd_restore");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return a descriptor or minus one
    emitter.label("__rt_stream_fd_raw");
    emitter.instruction("ret");                                                 // return a generation-zero legacy descriptor unchanged
}

/// Emits EOF lookup keyed by the stable stream state.
///
/// A filtered stream is at EOF only once the reader has actually seen everything: bytes may still
/// sit in the filtered-read buffer after the backend is exhausted, and the filter is still owed
/// its `$closing` dispatch. Reporting the backend's state alone would make `while (!feof($f))`
/// stop while output was still pending — the buffered read exists precisely to hand that output
/// back one `$length` at a time.
fn emit_stream_eof_get(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: read opaque stream EOF state ---");
    emitter.label_global("__rt_stream_eof_get");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around state lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_stream_state");                                // resolve the stable stream state from the opaque handle
    emitter.instruction("cbz x0, __rt_stream_eof_get_fail");                    // invalid or legacy handles have no state-owned EOF bit
    // The question is whether this stream went through the FILTERED BUFFER, not whether a filter
    // is still attached: `stream_filter_remove()` leaves the produced bytes on the stream, and
    // php keeps answering false until a read ATTEMPT comes back empty. Testing the chain head
    // made `feof()` true the instant the filter went, with bytes still owed to the reader.
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", STREAM_FILTERED_BUF_PTR_OFFSET
    ));                                                                         // did this stream ever buffer filtered bytes?
    emitter.instruction("cbz x9, __rt_stream_eof_get_backend");                 // never filtered: the backend's state is the answer
    emitter.instruction(&format!(
        "ldr x10, [x0, #{}]", STREAM_FILTERED_BUF_LEN_OFFSET
    ));                                                                         // filtered bytes held
    emitter.instruction(&format!(
        "ldr x11, [x0, #{}]", STREAM_FILTERED_BUF_POS_OFFSET
    ));                                                                         // filtered bytes already served
    emitter.instruction("cmp x10, x11");                                        // is anything still owed to the reader?
    emitter.instruction("b.ne __rt_stream_eof_get_more");                       // yes: the stream is not finished
    emitter.instruction(&format!(
        "ldr x12, [x0, #{}]", STREAM_FILTERED_FLUSHED_OFFSET
    ));                                                                         // has the closing dispatch run?
    emitter.instruction("cbz x12, __rt_stream_eof_get_more");                   // not yet: the filter may still emit
    emitter.label("__rt_stream_eof_get_backend");
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", STREAM_EOF_OFFSET
    ));                                                                         // load the stream-owned EOF word
    emitter.instruction("cmp x0, #0");                                          // normalize any non-zero state to PHP true
    emitter.instruction("cset x0, ne");                                         // return a strict zero-or-one EOF predicate
    emitter.instruction("b __rt_stream_eof_get_done");                          // join the common helper epilogue
    emitter.label("__rt_stream_eof_get_more");
    emitter.instruction("mov x0, #0");                                          // filtered output is still owed to the reader
    emitter.instruction("b __rt_stream_eof_get_done");                          // join the common helper epilogue
    emitter.label("__rt_stream_eof_get_fail");
    emitter.instruction("mov x0, #0");                                          // report false when no authoritative state exists
    emitter.label("__rt_stream_eof_get_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
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
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around state lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_stream_state");                                // resolve the stable stream state from the opaque handle
    emitter.instruction("cbz x0, __rt_stream_tls_session_none");                // raw descriptors and stale handles carry no session
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", STREAM_TLS_SESSION_OFFSET
    ));                                                                         // load the attached TLS session handle
    emitter.instruction("b __rt_stream_tls_session_done");                      // join the common helper epilogue
    emitter.label("__rt_stream_tls_session_none");
    emitter.instruction("mov x0, #0");                                          // report a plain, unencrypted transport
    emitter.label("__rt_stream_tls_session_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return the attached session handle
}

/// Emits `__rt_stream_set_tls_session(handle, session) -> ok`.
fn emit_stream_set_tls_session(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: attach a TLS session to an opaque stream ---");
    emitter.label_global("__rt_stream_set_tls_session");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the session slot and saved-frame storage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #16");                                    // establish a stable attachment frame
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the session across state lookup
    emitter.instruction("bl __rt_stream_state");                                // resolve the authoritative StreamState
    emitter.instruction("cbz x0, __rt_stream_set_tls_session_fail");            // reject stale or non-stream handles
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the session handle
    emitter.instruction(&format!(
        "str x1, [x0, #{}]", STREAM_TLS_SESSION_OFFSET
    ));                                                                         // publish the session on the stream state
    emitter.instruction("mov x0, #1");                                          // report successful attachment
    emitter.instruction("b __rt_stream_set_tls_session_done");                  // join the common helper epilogue
    emitter.label("__rt_stream_set_tls_session_fail");
    emitter.instruction("mov x0, #0");                                          // report that no session was attached
    emitter.label("__rt_stream_set_tls_session_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #32");                                     // release attachment scratch storage
    emitter.instruction("ret");                                                 // return the attachment status
}

/// Emits EOF replacement keyed by the stable stream state.
fn emit_stream_eof_set(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: replace opaque stream EOF state ---");
    emitter.label_global("__rt_stream_eof_set");
    emitter.instruction("sub sp, sp, #32");                                     // preserve the requested state and caller frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the requested EOF state
    emitter.instruction("bl __rt_stream_state");                                // resolve the stable stream state from the opaque handle
    emitter.instruction("cbz x0, __rt_stream_eof_set_fail");                    // ignore invalid, closed, and legacy handles
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the requested EOF state
    emitter.instruction("cmp x9, #0");                                          // normalize the state before publication
    emitter.instruction("cset x9, ne");                                         // keep the stored state strictly zero or one
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", STREAM_EOF_OFFSET
    ));                                                                         // publish EOF on this stream state only
    // Clearing EOF is how a seek says reading starts again, so the filtered-read buffer must
    // forget what the previous pass produced — including that the filter was already flushed.
    // Its allocation is kept; only the contents are discarded.
    emitter.instruction("cbnz x9, __rt_stream_eof_set_keep_buffer");            // setting EOF leaves the pending bytes alone
    emitter.instruction(&format!(
        "str xzr, [x0, #{}]", STREAM_FILTERED_BUF_LEN_OFFSET
    ));                                                                         // no filtered bytes carry across the seek
    emitter.instruction(&format!(
        "str xzr, [x0, #{}]", STREAM_FILTERED_BUF_POS_OFFSET
    ));                                                                         // and the read cursor restarts
    emitter.instruction(&format!(
        "str xzr, [x0, #{}]", STREAM_FILTERED_FLUSHED_OFFSET
    ));                                                                         // the new pass earns its own closing dispatch
    emitter.label("__rt_stream_eof_set_keep_buffer");
    emitter.instruction("mov x0, #1");                                          // report that the state was updated
    emitter.instruction("b __rt_stream_eof_set_done");                          // join the common helper epilogue
    emitter.label("__rt_stream_eof_set_fail");
    emitter.instruction("mov x0, #0");                                          // report that no authoritative state was updated
    emitter.label("__rt_stream_eof_set_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #32");                                     // release helper scratch storage
    emitter.instruction("ret");                                                 // return the update predicate
}

/// Emits state-owned chunk-size lookup with the existing 4096-byte read-loop fallback.
fn emit_stream_chunk_size(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve opaque stream chunk size ---");
    emitter.label_global("__rt_stream_chunk_size");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around state lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_stream_state");                                // resolve the live opaque stream state
    emitter.instruction("cbz x0, __rt_stream_chunk_size_default");              // invalid handles use the defensive read-loop fallback
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", STREAM_CHUNK_SIZE_OFFSET
    ));                                                                         // load the configured state-owned chunk size
    emitter.instruction("cbnz x0, __rt_stream_chunk_size_done");                // return an explicitly configured size
    emitter.label("__rt_stream_chunk_size_default");
    emitter.instruction(&format!("mov x0, #{STREAM_DEFAULT_CHUNK_SIZE}"));      // php's default when nothing configured one
    emitter.label("__rt_stream_chunk_size_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release aligned link-register storage
    emitter.instruction("ret");                                                 // return the effective read-loop chunk size
}

/// Emits chunk-size replacement keyed by opaque stream state rather than descriptor identity.
fn emit_stream_set_chunk_size(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: replace opaque stream chunk size ---");
    emitter.label_global("__rt_stream_set_chunk_size");
    emitter.instruction("sub sp, sp, #32");                                     // preserve the requested size and caller frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the requested chunk size
    emitter.instruction("bl __rt_stream_state");                                // resolve the live opaque stream state
    emitter.instruction("cbz x0, __rt_stream_set_chunk_size_fail");             // reject stale, closed, or non-stream handles
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", STREAM_CHUNK_SIZE_OFFSET
    ));                                                                         // load the previous state-owned chunk size
    emitter.instruction("cbnz x9, __rt_stream_set_chunk_size_have_old");        // preserve an explicitly configured previous size
    emitter.instruction("mov x9, #8192");                                       // materialize PHP's default stream chunk size
    emitter.label("__rt_stream_set_chunk_size_have_old");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the requested chunk size
    emitter.instruction(&format!(
        "str x10, [x0, #{}]", STREAM_CHUNK_SIZE_OFFSET
    ));                                                                         // publish the new size in the authoritative StreamState
    emitter.instruction("mov x0, x9");                                          // return the previous chunk size
    emitter.instruction("mov x1, #1");                                          // report successful live-stream resolution
    emitter.instruction("b __rt_stream_set_chunk_size_done");                   // join the helper epilogue
    emitter.label("__rt_stream_set_chunk_size_fail");
    emitter.instruction("mov x0, #0");                                          // invalid handles have no previous chunk size
    emitter.instruction("mov x1, #0");                                          // report failed stream-state resolution
    emitter.label("__rt_stream_set_chunk_size_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release helper scratch storage
    emitter.instruction("ret");                                                 // return the previous size and success flag
}

/// Emits minimal exact-once backend closure for fd, popen, and directory streams.
fn emit_stream_close_backend(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: minimally close an opaque stream backend ---");
    emitter.label_global("__rt_stream_close_backend");
    emitter.instruction("sub sp, sp, #64");                                     // reserve close state and a saved frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the opaque stream handle
    emitter.instruction("bl __rt_resource_mark_closing");                       // publish Closing before backend work
    emitter.instruction("cbz x0, __rt_stream_close_backend_fail");              // reject stale, closed, or re-entrant closes
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // resolve the Closing stream state
    emitter.instruction("cbz x0, __rt_stream_close_backend_mark");              // mark closed even when state is unexpectedly absent
    emitter.instruction("str x0, [sp, #24]");                                   // preserve StreamState for typed backend destructors
    // -- close any attached TLS session before the backend descriptor goes away --
    // Closing here rather than in the fclose lowering means every path that
    // destroys a stream sends close_notify, not just an explicit fclose().
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", STREAM_TLS_SESSION_OFFSET
    ));                                                                         // load the attached TLS session handle
    emitter.instruction("cbz x9, __rt_stream_close_backend_tls_done");          // plain transports have nothing to shut down
    emitter.instruction(&format!(
        "str xzr, [x0, #{}]", STREAM_TLS_SESSION_OFFSET
    ));                                                                         // detach before the call so a re-entrant close cannot double-free
    abi::emit_symbol_address(emitter, "x10", "_elephc_tls_close_fn");
    emitter.instruction("ldr x10, [x10]");                                      // load the published TLS close function pointer
    emitter.instruction("cbz x10, __rt_stream_close_backend_tls_done");         // the TLS backend is not linked into this program
    emitter.instruction("mov x0, x9");                                          // pass the session handle to the close helper
    emitter.instruction("blr x10");                                             // send close_notify and free the session
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload StreamState clobbered by the close call
    emitter.label("__rt_stream_close_backend_tls_done");
    emitter.instruction("ldr x9, [x0, #0]");                                    // load the stream backend kind
    emitter.instruction("ldr x10, [x0, #16]");                                  // load the backend descriptor or handle
    emitter.instruction(&format!(
        "ldr x11, [x0, #{}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // load backend-specific ownership independently of the descriptor
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the backend handle
    emitter.instruction(&format!("cmp x9, #{}", STREAM_BACKEND_FD));            // direct OS descriptor backend?
    emitter.instruction("b.eq __rt_stream_close_backend_fd");                   // close direct file and socket descriptors
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_USER_WRAPPER
    ));                                                                         // userspace stream-wrapper backend?
    emitter.instruction("b.eq __rt_stream_close_backend_user_wrapper");         // dispatch the wrapper's stream_close callback
    emitter.instruction(&format!("cmp x9, #{}", STREAM_BACKEND_POPEN));         // popen pipe backend?
    emitter.instruction("b.eq __rt_stream_close_backend_popen");                // close and reap popen resources
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_DIRECTORY
    ));                                                                         // native directory stream backend?
    emitter.instruction("b.eq __rt_stream_close_backend_dir");                  // close directory resources
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // glob directory stream backend?
    emitter.instruction("b.eq __rt_stream_close_backend_dir");                  // close the typed glob iterator
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_PHAR_WRITE
    ));                                                                         // buffered Phar write backend?
    emitter.instruction("b.eq __rt_stream_close_backend_phar");                 // finalize buffered Phar output
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // userspace directory-wrapper backend?
    emitter.instruction("b.eq __rt_stream_close_backend_user_dir");             // dispatch the wrapper's dir_closedir callback
    emitter.instruction("b __rt_stream_close_backend_mark");                    // unknown backends currently have no close hook
    emitter.label("__rt_stream_close_backend_fd");
    emitter.instruction("mov x0, x10");                                         // pass the owned descriptor to close
    emitter.instruction("cmp x0, #0");                                          // skip absent descriptors
    emitter.instruction("b.lt __rt_stream_close_backend_mark");                 // an absent descriptor needs no syscall
    // -- forget this descriptor's filters BEFORE the kernel can hand its number to anyone else --
    //
    // The same argument the TLS shutdown above makes: doing it in the `fclose()` lowering covers
    // only an explicit `fclose()`, and a stream released by refcount — a reassigned variable, a
    // scope exit — closed the descriptor with its filter byte still set. The next `fopen()` got
    // the recycled number and inherited a dead filter, so a plain `string.rot13` stream answered
    // deflate. Bounded because the tables are 512 bytes indexed by descriptor.
    // TWO SLOTS per descriptor, `fd` and `fd + 256`, in each 512-byte table — so the bound is
    // 256 descriptors, not 512, and both slots have to go. Indexing a descriptor at 300 would
    // otherwise wipe descriptor 44's second slot, and a filter parked in slot 1 would survive the
    // close and reach the recycled descriptor anyway.
    emitter.instruction("mov x9, #256");                                        // the filter tables cover descriptors below 256
    emitter.instruction("cmp x0, x9");
    emitter.instruction("b.hs __rt_stream_close_backend_filters_done");         // a descriptor past the table indexes nothing
    emitter.instruction("add x11, x0, #256");                                   // this descriptor's second slot
    abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
    emitter.instruction("strb wzr, [x9, x0]");                                  // clear the read filter, slot 0
    emitter.instruction("strb wzr, [x9, x11]");                                 // and slot 1
    abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
    emitter.instruction("strb wzr, [x9, x0]");                                  // clear the write filter, slot 0
    emitter.instruction("strb wzr, [x9, x11]");                                 // and slot 1
    emitter.label("__rt_stream_close_backend_filters_done");
    emitter.syscall(6);                                                         // close the native file or socket descriptor
    // `tmpfile()` owns the file its URI names, and php removes it exactly here: the path stays
    // reachable — `file_exists()`, `filesize()`, a second `fopen()` — for as long as the handle
    // lives. The deterministic shutdown closes request-owned resources at exit, so a program that
    // never calls `fclose()` still leaves nothing behind.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_unlink_if_owned");                      // `tmpfile()` removes its file here
    emitter.instruction("b __rt_stream_close_backend_mark");                    // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_user_wrapper");
    emitter.instruction("mov x0, x10");                                         // pass the synthetic wrapper handle to stream_close dispatch
    emitter.instruction("ldr x9, [sp, #24]");                                   // the StreamState this close is tearing down
    emitter.instruction(&format!(
        "ldr x9, [x9, #{}]",
        crate::codegen_support::runtime::resources::layout::STREAM_WRITTEN_SINCE_FLUSH_OFFSET
    ));                                                                         // php flushes only what was written
    abi::emit_symbol_address(emitter, "x11", "_uw_pending_flush");
    emitter.instruction("str x9, [x11]");
    emitter.instruction("bl __rt_user_wrapper_fclose");                         // invoke the userspace wrapper close callback exactly once
    emitter.instruction("b __rt_stream_close_backend_mark");                    // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_popen");
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload StreamState before detaching process ownership
    emitter.instruction(&format!(
        "str xzr, [x12, #{}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // prevent any re-entrant process close from reusing FILE*
    emitter.instruction("mov x0, x11");                                         // pass the owning FILE* to pclose
    emitter.instruction("bl __rt_pclose");                                      // close the FILE pointer and reap its child
    emitter.instruction("b __rt_stream_close_backend_mark");                    // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_dir");
    emitter.instruction("ldr x0, [sp, #24]");                                   // pass authoritative StreamState to directory cleanup
    emitter.instruction("bl __rt_closedir");                                    // close the typed native or glob iterator
    emitter.instruction("b __rt_stream_close_backend_mark");                    // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_phar");
    emitter.instruction("mov x0, x10");                                         // pass the synthetic Phar descriptor to its finalizer
    emitter.instruction("bl __rt_phar_write_finalize");                         // flush and close the buffered Phar write stream
    emitter.instruction("b __rt_stream_close_backend_mark");                    // finish lifecycle publication
    emitter.label("__rt_stream_close_backend_user_dir");
    emitter.instruction("ldr x0, [sp, #24]");                                   // pass authoritative StreamState to wrapper directory cleanup
    emitter.instruction("bl __rt_closedir");                                    // invoke userspace directory close exactly once
    emitter.label("__rt_stream_close_backend_mark");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_resource_mark_closed");                        // publish the terminal Closed state
    emitter.instruction("mov x0, #1");                                          // report an exact-once close attempt
    emitter.instruction("b __rt_stream_close_backend_done");                    // join the helper epilogue
    emitter.label("__rt_stream_close_backend_fail");
    emitter.instruction("mov x0, #0");                                          // report invalid or already-closing resources
    emitter.label("__rt_stream_close_backend_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #64");                                     // release close scratch storage
    emitter.instruction("ret");                                                 // return the close status
}

/// Emits owned StreamState teardown in child-before-parent order.
fn emit_stream_destroy_state(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: destroy an owned stream state ---");
    emitter.label_global("__rt_stream_destroy_state");
    emitter.instruction("cbz x0, __rt_stream_destroy_state_done");              // null stream states own no children or storage
    emitter.instruction("sub sp, sp, #32");                                     // reserve stable state storage and a saved frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #16");                                    // establish a stable teardown frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve StreamState across nested releases
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload StreamState for the attached filter chains
    emitter.instruction("bl __rt_stream_close_filter_chains");                  // PHP invalidates filter resources when their stream closes
    // Reload: x0 is caller-saved, and the chain teardown now runs user `onClose()`
    // hooks, so whatever it leaves behind is not the StreamState.
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload StreamState after the chain teardown
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // load the owned URI allocation
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload StreamState before detaching URI ownership
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // detach URI before potentially re-entrant heap cleanup
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_URI_LEN_OFFSET
    ));                                                                         // clear the detached URI length
    emitter.instruction("bl __rt_heap_free_safe");                              // release owned URI storage and ignore static standard-stream literals
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload StreamState for connect-host teardown
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // load the owned TLS-default host allocation
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_CONNECT_HOST_PTR_OFFSET
    ));                                                                         // detach host ownership before nested cleanup
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_CONNECT_HOST_LEN_OFFSET
    ));                                                                         // clear the detached host length
    emitter.instruction("bl __rt_heap_free_safe");                              // release owned host storage when present
    // The filtered-read buffer belongs to the state, so it is released here beside the URI and
    // host rather than at backend close: not every teardown path dispatches a backend close, and
    // one that skipped it left the buffer live for the life of the process.
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload StreamState for filtered-buffer teardown
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]", STREAM_FILTERED_BUF_PTR_OFFSET
    ));                                                                         // load the buffer a filtered read allocated
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_FILTERED_BUF_PTR_OFFSET
    ));                                                                         // detach before the free so a re-entrant teardown cannot double-free
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_FILTERED_BUF_LEN_OFFSET
    ));                                                                         // no bytes are held any more
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_FILTERED_BUF_CAP_OFFSET
    ));                                                                         // and no capacity remains
    emitter.instruction("bl __rt_heap_free_safe");                              // release the filtered-read buffer when present
    // The reported mode string is persisted into owned storage at open time, exactly like the URI,
    // so it is released on the same path.
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload StreamState for mode-string teardown
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]", STREAM_MODE_PTR_OFFSET
    ));                                                                         // load the owned reported-mode allocation
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_MODE_PTR_OFFSET
    ));                                                                         // detach before the free so a re-entrant teardown cannot double-free
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_MODE_LEN_OFFSET
    ));                                                                         // clear the detached mode length
    emitter.instruction("bl __rt_heap_free_safe");                              // release owned mode storage when present
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload StreamState for context teardown
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]", STREAM_CONTEXT_HANDLE_OFFSET
    ));                                                                         // load the retained stream-context handle
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]", STREAM_CONTEXT_HANDLE_OFFSET
    ));                                                                         // detach context ownership before nested registry cleanup
    emitter.instruction("cbz x0, __rt_stream_destroy_state_context_done");      // skip release when no context is attached
    emitter.instruction("bl __rt_resource_release");                            // release the StreamState-owned context reference
    emitter.label("__rt_stream_destroy_state_context_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass StreamState itself to the heap allocator
    emitter.instruction("bl __rt_heap_free");                                   // release the owned 320-byte state allocation
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #32");                                     // release teardown scratch storage
    emitter.label("__rt_stream_destroy_state_done");
    emitter.instruction("ret");                                                 // return after exact-once child and state teardown
}
