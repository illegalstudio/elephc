//! Purpose:
//! Emits handle-keyed stream metadata replacement for wrapper identity and URI.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - Stream-open lowering after a backend descriptor has entered the resource registry.
//!
//! Key details:
//! - URI storage belongs to the stable StreamState rather than a reusable OS descriptor.
//! - Replacement persists the new URI before releasing the previous allocation.
//! - Userspace and buffered-Phar backend kinds override the caller's fallback wrapper id.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_BACKEND_KIND_OFFSET, STREAM_BACKEND_PHAR_WRITE, STREAM_BACKEND_USER_DIRECTORY,
    STREAM_BACKEND_USER_WRAPPER, STREAM_OWNERSHIP_FLAGS_OFFSET, STREAM_STATE_FLAG_IS_URL,
    STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET, STREAM_WRAPPER_ID_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_stream_record_meta(handle, wrapper_id, uri_ptr, uri_len) -> handle`.
///
/// AArch64 receives `x0/x1/x2/x3`; x86_64 receives `rdi/rsi/rdx/rcx`.
/// Invalid or stale handles are returned unchanged without taking string ownership.
pub fn emit_stream_record_meta(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_record_meta_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: replace handle-keyed stream metadata ---");
    emitter.label_global("__rt_stream_record_meta");
    emitter.instruction("sub sp, sp, #64");                                     // reserve metadata inputs, state, and a saved frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #48");                                    // establish a stable metadata frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve opaque handle and requested wrapper id
    emitter.instruction("stp x2, x3, [sp, #16]");                               // preserve URI pointer and byte length
    emitter.instruction("bl __rt_stream_state");                                // resolve Live or Closing stable StreamState
    emitter.instruction("cbz x0, __rt_stream_record_meta_done");                // stale handles cannot acquire URI ownership
    emitter.instruction("str x0, [sp, #32]");                                   // preserve StreamState across string persistence
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // load URI bytes for independent persistence
    emitter.instruction("bl __rt_str_persist");                                 // duplicate URI into owned heap storage
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the new owned URI pointer
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload StreamState for atomic ownership replacement
    emitter.instruction(&format!(
        "ldr x10, [x9, #{}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // capture the previous owned URI allocation
    emitter.instruction(&format!(
        "str x1, [x9, #{}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // publish the new owned URI pointer
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the caller-visible URI byte length
    emitter.instruction(&format!(
        "str x11, [x9, #{}]", STREAM_URI_LEN_OFFSET
    ));                                                                         // publish the new URI byte length
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the requested wrapper id
    // A URI that NAMES the zip wrapper is the zip wrapper, whichever route recorded it. The
    // dynamic `fopen()` path cannot know the scheme while it lowers and passes the plainfile
    // fallback, which made `stream_get_meta_data()` report `plainfile`/`STDIO` for a zip entry
    // stream and let it seek. Deciding it HERE, from the URI every caller already hands over,
    // is the one place both the literal and the run-time filename pass through.
    emitter.instruction("ldr x13, [sp, #24]");                                  // the caller-visible URI byte length
    emitter.instruction("cmp x13, #6");                                         // at least "zip://" long?
    emitter.instruction("b.lo __rt_srm_not_zip");                               // too short to name the zip wrapper
    emitter.instruction("ldr x14, [sp, #16]");                                  // the owned URI pointer
    for (offset, byte) in b"zip://".iter().enumerate() {
        emitter.instruction(&format!("ldrb w15, [x14, #{}]", offset));          // one URI byte
        emitter.instruction(&format!("cmp w15, #{:#x}", byte));                 // against the canonical zip:// prefix
        emitter.instruction("b.ne __rt_srm_not_zip");                           // a different scheme keeps the caller's id
    }
    emitter.instruction(&format!(
        "mov x11, #{}",
        super::WRAPPER_ID_ZIP
    ));                                                                         // php's twelfth wrapper, whose ops have no seek
    emitter.label("__rt_srm_not_zip");
    emitter.instruction(&format!(
        "ldr x13, [x9, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // load instance flags before replacing URL identity
    emitter.instruction(&format!(
        "mov x14, #{}", STREAM_STATE_FLAG_IS_URL
    ));                                                                         // materialize the StreamState URL-wrapper bit
    emitter.instruction("bic x13, x13, x14");                                   // clear any previous URL-wrapper identity
    // data:// carries its payload in the URI itself and php reports it as NOT local, so it
    // joins the remote wrappers here even though it never leaves the process.
    emitter.instruction("cmp x11, #7");                                         // the data:// wrapper id
    emitter.instruction("b.eq __rt_stream_record_meta_builtin_url_mark");
    emitter.instruction("cmp x11, #1");                                         // built-in remote wrapper ids start at HTTP
    emitter.instruction("b.lo __rt_stream_record_meta_builtin_url_done");       // plainfile and unset wrappers are local
    emitter.instruction("cmp x11, #4");                                         // built-in remote wrapper ids end at FTPS
    emitter.instruction("b.hi __rt_stream_record_meta_builtin_url_done");       // all later built-in wrappers are local
    emitter.label("__rt_stream_record_meta_builtin_url_mark");
    emitter.instruction("orr x13, x13, x14");                                   // mark built-in HTTP(S)/FTP(S) and data:// as URL streams
    emitter.label("__rt_stream_record_meta_builtin_url_done");
    emitter.instruction(&format!(
        "str x13, [x9, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // publish the refreshed instance flags
    emitter.instruction(&format!(
        "ldr x12, [x9, #{}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // inspect backend-specific wrapper identity
    emitter.instruction(&format!(
        "cmp x12, #{}", STREAM_BACKEND_USER_WRAPPER
    ));                                                                         // is this a userspace stream-wrapper backend?
    emitter.instruction("b.eq __rt_stream_record_meta_user");                   // userspace wrappers report a stable user-space identity
    emitter.instruction(&format!(
        "cmp x12, #{}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // is this a userspace directory-wrapper backend?
    emitter.instruction("b.eq __rt_stream_record_meta_user");                   // directory wrappers share the user-space metadata identity
    emitter.instruction(&format!(
        "cmp x12, #{}", STREAM_BACKEND_PHAR_WRITE
    ));                                                                         // is this a buffered Phar write backend?
    emitter.instruction("b.ne __rt_stream_record_meta_scheme");                 // ordinary backends are identified by their URI
    emitter.instruction("mov x11, #5");                                         // wrapper id 5 selects the Phar wrapper literal
    emitter.instruction("b __rt_stream_record_meta_wrapper");                   // publish the inferred Phar identity

    // A recorded URI beginning `php://` belongs to the `php` wrapper whatever opened it. php
    // decides this by SCHEME, and a `php://filter/...` URL built at run time never reaches the
    // literal parser that re-stamps the identity at the open site — so it kept the inner opener's.
    emitter.label("__rt_stream_record_meta_scheme");
    emitter.instruction("ldp x13, x14, [sp, #16]");                             // the persisted URI and its length
    emitter.instruction("cbz x13, __rt_stream_record_meta_wrapper");            // no URI: keep the caller's fallback
    emitter.instruction("cmp x14, #6");                                         // "php://" is six bytes
    emitter.instruction("b.lt __rt_stream_record_meta_wrapper");
    for (offset, byte) in b"php://".iter().enumerate() {
        emitter.instruction(&format!("ldrb w15, [x13, #{}]", offset));
        emitter.instruction(&format!("cmp w15, #{}", byte));
        emitter.instruction("b.ne __rt_stream_record_meta_wrapper");
    }
    emitter.instruction("mov x11, #6");                                         // wrapper id 6 selects the PHP wrapper literal
    // Explicitly: the userspace arm is emitted immediately below, and falling into it made a
    // filtered plain file report `user-space`.
    emitter.instruction("b __rt_stream_record_meta_wrapper");
    emitter.label("__rt_stream_record_meta_user");
    emitter.instruction("str x10, [sp, #40]");                                  // preserve detached URI ownership across definition lookup
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // pass the persisted URI bytes to wrapper-definition lookup
    emitter.instruction("bl __rt_user_wrapper_uri_flags");                      // resolve registration flags at instance creation time
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload StreamState after definition lookup
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload detached previous URI ownership
    emitter.instruction(&format!(
        "tst x0, #{}", 1
    ));                                                                         // did the definition declare STREAM_IS_URL?
    emitter.instruction("b.eq __rt_stream_record_meta_user_flags_done");        // leave non-URL user wrappers local
    emitter.instruction(&format!(
        "ldr x13, [x9, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // load instance flags for the URL bit update
    emitter.instruction(&format!(
        "orr x13, x13, #{}", STREAM_STATE_FLAG_IS_URL
    ));                                                                         // preserve the definition's URL identity on this instance
    emitter.instruction(&format!(
        "str x13, [x9, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // publish instance-local STREAM_IS_URL state
    emitter.label("__rt_stream_record_meta_user_flags_done");
    emitter.instruction("mov x11, #11");                                        // wrapper id 11 selects the user-space wrapper literal
    emitter.label("__rt_stream_record_meta_wrapper");
    emitter.instruction(&format!(
        "str x11, [x9, #{}]", STREAM_WRAPPER_ID_OFFSET
    ));                                                                         // publish the StreamState-owned wrapper identity
    emitter.instruction("mov x0, x10");                                         // pass the detached previous URI to safe heap release
    emitter.instruction("bl __rt_heap_free_safe");                              // release only live heap-backed URI storage
    emitter.label("__rt_stream_record_meta_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the original opaque stream handle
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #64");                                     // release metadata scratch storage
    emitter.instruction("ret");                                                 // return after replacement or stale-handle no-op
    emit_user_wrapper_uri_flags_aarch64(emitter);
}

/// Emits the Linux x86_64 handle-keyed stream metadata replacement helper.
fn emit_stream_record_meta_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: replace handle-keyed stream metadata ---");
    emitter.label_global("__rt_stream_record_meta");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable metadata frame
    emitter.instruction("sub rsp, 64");                                         // reserve metadata inputs, state, and ownership replacement storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested wrapper id
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the URI pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the URI byte length
    emitter.instruction("call __rt_stream_state");                              // resolve Live or Closing stable StreamState
    emitter.instruction("test rax, rax");                                       // did registry lookup resolve a stream state?
    emitter.instruction("jz __rt_stream_record_meta_done_x86");                 // stale handles cannot acquire URI ownership
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve StreamState across string persistence
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the URI source pointer for persistence
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // load the URI byte length for persistence
    emitter.instruction("call __rt_str_persist");                               // duplicate URI into owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the new owned URI pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload StreamState for atomic ownership replacement
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [r10 + {}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // capture the previous owned URI allocation
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], rax", STREAM_URI_PTR_OFFSET
    ));                                                                         // publish the new owned URI pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the caller-visible URI byte length
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], rax", STREAM_URI_LEN_OFFSET
    ));                                                                         // publish the new URI byte length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the requested wrapper id
    // See the AArch64 counterpart: a URI that names the zip wrapper IS the zip wrapper, and
    // this is the one place both the literal and the run-time filename pass through.
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // the caller-visible URI byte length
    emitter.instruction("cmp rcx, 6");                                          // at least "zip://" long?
    emitter.instruction("jb __rt_srm_not_zip_x86");                             // too short to name the zip wrapper
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // the owned URI pointer
    for (offset, byte) in b"zip://".iter().enumerate() {
        emitter.instruction(&format!(
            "cmp BYTE PTR [rcx + {}], {:#x}", offset, byte
        ));                                                                     // one URI byte against the canonical zip:// prefix
        emitter.instruction("jne __rt_srm_not_zip_x86");                        // a different scheme keeps the caller's id
    }
    emitter.instruction(&format!("mov rax, {}", super::WRAPPER_ID_ZIP));        // php's twelfth wrapper, whose ops have no seek
    emitter.label("__rt_srm_not_zip_x86");
    emitter.instruction(&format!(
        "mov rdx, QWORD PTR [r10 + {}]", STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // load instance flags before replacing URL identity
    emitter.instruction(&format!(
        "and rdx, -{}", STREAM_STATE_FLAG_IS_URL + 1
    ));                                                                         // clear any previous URL-wrapper identity
    // See the AArch64 half: data:// is reported as NOT local by php.
    emitter.instruction("cmp rax, 7");                                          // the data:// wrapper id
    emitter.instruction("je __rt_stream_record_meta_builtin_url_mark_x86");
    emitter.instruction("cmp rax, 1");                                          // built-in remote wrapper ids start at HTTP
    emitter.instruction("jb __rt_stream_record_meta_builtin_url_done_x86");     // plainfile and unset wrappers are local
    emitter.instruction("cmp rax, 4");                                          // built-in remote wrapper ids end at FTPS
    emitter.instruction("ja __rt_stream_record_meta_builtin_url_done_x86");     // all later built-in wrappers are local
    emitter.label("__rt_stream_record_meta_builtin_url_mark_x86");
    emitter.instruction(&format!(
        "or rdx, {}", STREAM_STATE_FLAG_IS_URL
    ));                                                                         // mark built-in HTTP(S)/FTP(S) instances as URL streams
    emitter.label("__rt_stream_record_meta_builtin_url_done_x86");
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], rdx", STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // publish the refreshed instance flags
    emitter.instruction(&format!(
        "mov rcx, QWORD PTR [r10 + {}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // inspect backend-specific wrapper identity
    emitter.instruction(&format!(
        "cmp rcx, {}", STREAM_BACKEND_USER_WRAPPER
    ));                                                                         // is this a userspace stream-wrapper backend?
    emitter.instruction("je __rt_stream_record_meta_user_x86");                 // userspace wrappers report a stable user-space identity
    emitter.instruction(&format!(
        "cmp rcx, {}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // is this a userspace directory-wrapper backend?
    emitter.instruction("je __rt_stream_record_meta_user_x86");                 // directory wrappers share the user-space metadata identity
    emitter.instruction(&format!(
        "cmp rcx, {}", STREAM_BACKEND_PHAR_WRITE
    ));                                                                         // is this a buffered Phar write backend?
    emitter.instruction("jne __rt_stream_record_meta_scheme_x86");              // ordinary backends are identified by their URI
    emitter.instruction("mov eax, 5");                                          // wrapper id 5 selects the Phar wrapper literal
    emitter.instruction("jmp __rt_stream_record_meta_wrapper_x86");             // publish the inferred Phar identity

    // A recorded URI beginning `php://` belongs to the `php` wrapper whatever opened it. php
    // decides this by SCHEME, and a `php://filter/...` URL built at run time never reaches the
    // literal parser that re-stamps the identity at the open site — so it kept the inner opener's.
    emitter.label("__rt_stream_record_meta_scheme_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // the persisted URI
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // and its length
    emitter.instruction("test r8, r8");
    emitter.instruction("jz __rt_stream_record_meta_wrapper_x86");              // no URI: keep the caller's fallback
    emitter.instruction("cmp r9, 6");                                           // "php://" is six bytes
    emitter.instruction("jl __rt_stream_record_meta_wrapper_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 0]");
    emitter.instruction("cmp ecx, 112");
    emitter.instruction("jne __rt_stream_record_meta_wrapper_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 1]");
    emitter.instruction("cmp ecx, 104");
    emitter.instruction("jne __rt_stream_record_meta_wrapper_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 2]");
    emitter.instruction("cmp ecx, 112");
    emitter.instruction("jne __rt_stream_record_meta_wrapper_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 3]");
    emitter.instruction("cmp ecx, 58");
    emitter.instruction("jne __rt_stream_record_meta_wrapper_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 4]");
    emitter.instruction("cmp ecx, 47");
    emitter.instruction("jne __rt_stream_record_meta_wrapper_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 5]");
    emitter.instruction("cmp ecx, 47");
    emitter.instruction("jne __rt_stream_record_meta_wrapper_x86");
    emitter.instruction("mov eax, 6");                                          // wrapper id 6 selects the PHP wrapper literal
    // Explicitly: the userspace arm is emitted immediately below, and falling into it made a
    // filtered plain file report `user-space`.
    emitter.instruction("jmp __rt_stream_record_meta_wrapper_x86");
    emitter.label("__rt_stream_record_meta_user_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve detached URI ownership across definition lookup
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the persisted URI pointer to definition lookup
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the persisted URI byte length to definition lookup
    emitter.instruction("call __rt_user_wrapper_uri_flags");                    // resolve registration flags at instance creation time
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload StreamState after definition lookup
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload detached previous URI ownership
    emitter.instruction("test eax, 1");                                         // did the definition declare STREAM_IS_URL?
    emitter.instruction("jz __rt_stream_record_meta_user_flags_done_x86");      // leave non-URL user wrappers local
    emitter.instruction(&format!(
        "or QWORD PTR [r10 + {}], {}", STREAM_OWNERSHIP_FLAGS_OFFSET,
        STREAM_STATE_FLAG_IS_URL
    ));                                                                         // preserve the definition's URL identity on this instance
    emitter.label("__rt_stream_record_meta_user_flags_done_x86");
    emitter.instruction("mov eax, 11");                                         // wrapper id 11 selects the user-space wrapper literal
    emitter.label("__rt_stream_record_meta_wrapper_x86");
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], rax", STREAM_WRAPPER_ID_OFFSET
    ));                                                                         // publish the StreamState-owned wrapper identity
    emitter.instruction("mov rax, r11");                                        // pass the detached previous URI to safe heap release
    emitter.instruction("call __rt_heap_free_safe");                            // release only live heap-backed URI storage
    emitter.label("__rt_stream_record_meta_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the original opaque stream handle
    emitter.instruction("add rsp, 64");                                         // release metadata scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after replacement or stale-handle no-op
    emit_user_wrapper_uri_flags_x86_64(emitter);
}

/// Emits the AArch64 leaf lookup that resolves definition flags for a user-wrapper URI.
fn emit_user_wrapper_uri_flags_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve user-wrapper definition flags for a URI ---");
    emitter.label("__rt_user_wrapper_uri_flags");
    emitter.instruction("mov x2, #0");                                          // start at wrapper definition slot zero
    super::emit_load_table_base(emitter, "x3");
    emitter.label("__rt_user_wrapper_uri_flags_scan");
    super::emit_load_table_cap(emitter, "x4");
    emitter.instruction("cmp x2, x4");                                         // did lookup exhaust every allocated definition slot?
    emitter.instruction("b.hs __rt_user_wrapper_uri_flags_miss");               // no registered scheme matched the URI
    emitter.instruction("add x4, x3, x2, lsl #5");                              // address the next 32-byte definition slot
    emitter.instruction("ldr x5, [x4]");                                        // load the registered protocol pointer
    emitter.instruction("cbz x5, __rt_user_wrapper_uri_flags_next");            // skip unregistered definition slots
    emitter.instruction("ldr x6, [x4, #8]");                                    // load the registered protocol byte length
    emitter.instruction("cmp x6, x1");                                          // must the URI contain protocol plus a colon?
    emitter.instruction("b.hs __rt_user_wrapper_uri_flags_next");               // reject missing or truncated URI schemes
    emitter.instruction("ldrb w7, [x0, x6]");                                   // load the URI byte immediately after the scheme
    emitter.instruction("cmp w7, #58");                                         // does a colon terminate the URI scheme?
    emitter.instruction("b.ne __rt_user_wrapper_uri_flags_next");               // reject prefix-only protocol matches
    emitter.instruction("mov x7, #0");                                          // initialize protocol byte comparison
    emitter.label("__rt_user_wrapper_uri_flags_cmp");
    emitter.instruction("cmp x7, x6");                                          // have all protocol bytes matched?
    emitter.instruction("b.hs __rt_user_wrapper_uri_flags_match");              // load flags from the matching definition slot
    emitter.instruction("ldrb w8, [x5, x7]");                                   // load one registered protocol byte
    emitter.instruction("ldrb w9, [x0, x7]");                                   // load the corresponding URI scheme byte
    emitter.instruction("cmp w8, w9");                                          // do the protocol bytes match?
    emitter.instruction("b.ne __rt_user_wrapper_uri_flags_next");               // continue scanning after a byte mismatch
    emitter.instruction("add x7, x7, #1");                                      // advance the protocol comparison index
    emitter.instruction("b __rt_user_wrapper_uri_flags_cmp");                   // compare the remaining protocol bytes
    emitter.label("__rt_user_wrapper_uri_flags_match");
    super::emit_load_flags_base(emitter, "x3");
    emitter.instruction("ldr x0, [x3, x2, lsl #3]");                            // return the matching definition flags
    emitter.instruction("ret");                                                 // return definition flags to metadata publication
    emitter.label("__rt_user_wrapper_uri_flags_next");
    emitter.instruction("add x2, x2, #1");                                      // advance to the next definition slot
    emitter.instruction("b __rt_user_wrapper_uri_flags_scan");                  // continue scanning registered schemes
    emitter.label("__rt_user_wrapper_uri_flags_miss");
    emitter.instruction("mov x0, #0");                                          // unmatched URIs have no registration flags
    emitter.instruction("ret");                                                 // return local-wrapper defaults
}

/// Emits the x86_64 leaf lookup that resolves definition flags for a user-wrapper URI.
fn emit_user_wrapper_uri_flags_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve user-wrapper definition flags for a URI ---");
    emitter.label("__rt_user_wrapper_uri_flags");
    emitter.instruction("xor r8d, r8d");                                        // start at wrapper definition slot zero
    super::emit_load_table_base(emitter, "r9");
    emitter.label("__rt_user_wrapper_uri_flags_scan_x86");
    super::emit_load_table_cap(emitter, "r11");
    emitter.instruction("cmp r8, r11");                                          // did lookup exhaust every allocated definition slot?
    emitter.instruction("jae __rt_user_wrapper_uri_flags_miss_x86");            // no registered scheme matched the URI
    emitter.instruction("mov r11, r8");                                         // copy the definition slot for scaling
    emitter.instruction("shl r11, 5");                                          // convert the slot index to a 32-byte offset
    emitter.instruction("add r11, r9");                                         // address the next definition slot
    emitter.instruction("mov r10, QWORD PTR [r11]");                            // load the registered protocol pointer
    emitter.instruction("test r10, r10");                                       // is this definition slot occupied?
    emitter.instruction("jz __rt_user_wrapper_uri_flags_next_x86");             // skip unregistered definition slots
    emitter.instruction("mov rcx, QWORD PTR [r11 + 8]");                        // load the registered protocol byte length
    emitter.instruction("cmp rcx, rsi");                                        // must the URI contain protocol plus a colon?
    emitter.instruction("jae __rt_user_wrapper_uri_flags_next_x86");            // reject missing or truncated URI schemes
    emitter.instruction("cmp BYTE PTR [rdi + rcx], 58");                        // does a colon terminate the URI scheme?
    emitter.instruction("jne __rt_user_wrapper_uri_flags_next_x86");            // reject prefix-only protocol matches
    emitter.instruction("xor edx, edx");                                        // initialize protocol byte comparison
    emitter.label("__rt_user_wrapper_uri_flags_cmp_x86");
    emitter.instruction("cmp rdx, rcx");                                        // have all protocol bytes matched?
    emitter.instruction("jae __rt_user_wrapper_uri_flags_match_x86");           // load flags from the matching definition slot
    emitter.instruction("movzx eax, BYTE PTR [r10 + rdx]");                     // load one registered protocol byte
    emitter.instruction("movzx r11d, BYTE PTR [rdi + rdx]");                    // load the corresponding URI scheme byte
    emitter.instruction("cmp al, r11b");                                        // do the protocol bytes match?
    emitter.instruction("jne __rt_user_wrapper_uri_flags_next_x86");            // continue scanning after a byte mismatch
    emitter.instruction("inc rdx");                                             // advance the protocol comparison index
    emitter.instruction("jmp __rt_user_wrapper_uri_flags_cmp_x86");             // compare the remaining protocol bytes
    emitter.label("__rt_user_wrapper_uri_flags_match_x86");
    super::emit_load_flags_base(emitter, "r9");
    emitter.instruction("mov rax, QWORD PTR [r9 + r8 * 8]");                    // return the matching definition flags
    emitter.instruction("ret");                                                 // return definition flags to metadata publication
    emitter.label("__rt_user_wrapper_uri_flags_next_x86");
    emitter.instruction("inc r8");                                              // advance to the next definition slot
    emitter.instruction("jmp __rt_user_wrapper_uri_flags_scan_x86");            // continue scanning registered schemes
    emitter.label("__rt_user_wrapper_uri_flags_miss_x86");
    emitter.instruction("xor eax, eax");                                        // unmatched URIs have no registration flags
    emitter.instruction("ret");                                                 // return local-wrapper defaults
}
