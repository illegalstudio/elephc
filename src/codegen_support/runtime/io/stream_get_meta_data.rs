//! Purpose:
//! Emits the `__rt_stream_get_meta_data` runtime helper, which builds the
//! PHP-compatible metadata hash describing an open stream resource.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Returns a `{string => mixed}` hash with the nine documented keys. `eof`
//!   comes from the stable `StreamState`; `seekable`/`stream_type` are derived
//!   from `lseek`; `blocked`/`mode` from `fcntl(F_GETFL)`.
//! - `wrapper_type` and `uri` come from handle-keyed StreamState metadata.

use crate::codegen_support::abi;
use crate::codegen_support::runtime::resources::layout::{
    STREAM_BACKEND_DIRECTORY, STREAM_BACKEND_GLOB_DIRECTORY, STREAM_BACKEND_KIND_OFFSET,
    STREAM_BACKEND_USER_DIRECTORY, STREAM_FD_OFFSET, STREAM_MODE_LEN_OFFSET,
    STREAM_MODE_PTR_OFFSET, STREAM_TRANSPORT_OFFSET, STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET,
    STREAM_WRAPPER_ID_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Wrapper id 6 is `php://`, whose `temp` sub-wrapper is the one that answers no metadata API.
const WRAPPER_ID_PHP: u64 = 6;

/// Wrapper id 7 is `data:`, which answers no metadata API at all.
const WRAPPER_ID_DATA: u64 = 7;

/// Wrapper id 1 is `http://`, one of the two that carry a response under `wrapper_data`.
const WRAPPER_ID_HTTP: u64 = 1;

/// Wrapper id 2 is `https://`, the other one.
const WRAPPER_ID_HTTPS: u64 = 2;

/// stream_get_meta_data: build the metadata hash for an opaque stream handle.
/// Input:  AArch64 x0 = handle / x86_64 rdi = handle
/// Output: pointer to a `{string => mixed}` hash table
pub fn emit_stream_get_meta_data(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_meta_data_linux_x86_64(emitter);
        emit_stream_fd_is_regular_x86_64(emitter);
        emit_stream_meta_has_api_flags_x86_64(emitter);
        emit_data_uri_params_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let nonblock = plat.o_nonblock();

    emitter.blank();
    emitter.comment("--- runtime: stream_get_meta_data ---");
    emitter.label_global("__rt_stream_get_meta_data");

    // Frame (112 bytes): [0]=handle [8]=hash [16]=seekable [24]=blocked [32]=eof
    //                   [40]=mode_ptr [48]=mode_len [56]=stype_ptr [64]=stype_len
    //                   [72]=backend fd [80]=StreamState [96]=x29 [104]=x30
    emitter.instruction("sub sp, sp, #112");                                    // allocate the metadata frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // resolve handle-keyed metadata and backend state
    emitter.instruction("str x0, [sp, #80]");                                   // preserve StreamState across metadata hash construction
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", STREAM_FD_OFFSET
    ));                                                                         // load the backend descriptor for native probes
    emitter.instruction("str x0, [sp, #72]");                                   // preserve the descriptor for native metadata probes

    // -- seekability: lseek(fd, 0, SEEK_CUR) --
    emitter.instruction("mov x1, #0");                                          // offset 0
    emitter.instruction("mov x2, #1");                                          // SEEK_CUR
    emitter.syscall(199);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means lseek failed
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_sgmd_seekable")); // lseek ok: the stream is seekable

    // -- not seekable: socket-like stream --
    emitter.instruction("mov x9, #0");                                          // seekable = false
    emitter.instruction("str x9, [sp, #16]");                                   // save the seekable flag
    abi::emit_symbol_address(emitter, "x10", "_meta_stype_socket");             // load page of the "tcp_socket" literal
    emitter.instruction("str x10, [sp, #56]");                                  // save the stream_type pointer
    emitter.instruction("mov x10, #10");                                        // length of "tcp_socket"
    emitter.instruction("str x10, [sp, #64]");                                  // save the stream_type length
    emitter.instruction("b __rt_sgmd_seek_done");                               // skip the seekable branch

    emitter.label("__rt_sgmd_seekable");
    emitter.instruction("mov x9, #1");                                          // seekable = true
    emitter.instruction("str x9, [sp, #16]");                                   // save the seekable flag
    abi::emit_symbol_address(emitter, "x10", "_meta_stype_stdio");              // load page of the "STDIO" literal
    emitter.instruction("str x10, [sp, #56]");                                  // save the stream_type pointer
    emitter.instruction("mov x10, #5");                                         // length of "STDIO"
    emitter.instruction("str x10, [sp, #64]");                                  // save the stream_type length
    emitter.label("__rt_sgmd_seek_done");
    // `stream_type` is a wrapper and backend identity in php-src, not a descriptor property. The
    // derivation above only knows whether `lseek` worked, which called `php://memory` STDIO and a
    // `popen()` pipe a socket; a stream that records an identity reports that instead.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_type_name");                            // x0 = name or 0, x1 = length
    emitter.instruction("cbz x0, __rt_sgmd_stype_kept");                        // nothing recorded: keep the derived name
    emitter.instruction("str x0, [sp, #56]");                                   // report the recorded name
    emitter.instruction("str x1, [sp, #64]");                                   // and its length
    emitter.label("__rt_sgmd_stype_kept");

    // `seekable` is not the lseek probe above. php-src decides it once, at open:
    // `php_stream_fopen_from_fd` sets `is_pipe` from `!S_ISREG(st_mode)` and that becomes
    // `PHP_STREAM_FLAG_NO_SEEK`, which is what `_php_stream_get_metadata` reports. The two
    // questions agree for regular files, sockets and FIFOs and part company on a CHARACTER
    // DEVICE: `lseek` succeeds on `/dev/null`, so elephc called it seekable where php says
    // false. The lseek answer stays where it is because the stream_type fallback above still
    // wants it — only the reported flag moves.
    emitter.instruction("ldr x0, [sp, #72]");                                   // the backend descriptor
    emitter.instruction("bl __rt_stream_fd_is_regular");                        // S_ISREG, the question php asks
    // ...unless the WRAPPER has no seek at all. php-src builds a stream from a set of ops, and
    // `ext/zip`'s entry ops leave `seek` NULL, which `_php_stream_get_metadata` reports as
    // `seekable => false` no matter what backs the stream. elephc serves a zip entry from a
    // regular temp file, so S_ISREG alone called it seekable where php says false. Measured:
    // `stream_get_meta_data(fopen("zip://a.zip#f.txt","r"))["seekable"]` is `bool(false)`.
    emitter.instruction("ldr x9, [sp, #80]");                                   // the stable StreamState pointer
    emitter.instruction(&format!("ldr x9, [x9, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
    emitter.instruction(&format!(
        "cmp x9, #{}",
        super::WRAPPER_ID_ZIP
    ));                                                                         // the zip wrapper?
    emitter.instruction("csel x0, xzr, x0, eq");                                // a zip entry stream never seeks
    // ...and a DIRECTORY always does. php's plain-files directory ops carry a seek (rewinddir),
    // so `_php_stream_get_metadata` reports `seekable => true` for every `opendir()` handle,
    // while `S_ISREG` on a directory descriptor is false. This is the same "ask the ops, not the
    // descriptor" rule as the zip case just above, in the other direction.
    emitter.instruction("ldr x9, [sp, #80]");                                   // the stable StreamState pointer
    emitter.instruction(&format!("ldr x9, [x9, #{STREAM_BACKEND_KIND_OFFSET}]")); // what backs the stream
    emitter.instruction(&format!("cmp x9, #{STREAM_BACKEND_DIRECTORY}"));
    emitter.instruction("b.eq __rt_sgmd_dir_seekable");
    emitter.instruction(&format!("cmp x9, #{STREAM_BACKEND_USER_DIRECTORY}"));
    emitter.instruction("b.eq __rt_sgmd_dir_seekable");
    emitter.instruction(&format!("cmp x9, #{STREAM_BACKEND_GLOB_DIRECTORY}"));
    emitter.instruction("b.ne __rt_sgmd_dir_seek_done");
    emitter.label("__rt_sgmd_dir_seekable");
    emitter.instruction("mov x0, #1");                                          // a directory handle is seekable
    emitter.label("__rt_sgmd_dir_seek_done");
    emitter.instruction("str x0, [sp, #16]");                                   // that is the seekable flag

    // -- blocking mode + access mode: fcntl(fd, F_GETFL, 0) --
    emitter.instruction("ldr x0, [sp, #72]");                                   // reload the stream descriptor
    emitter.instruction("mov x1, #3");                                          // F_GETFL
    emitter.instruction("mov x2, #0");                                          // unused third argument
    emitter.syscall(92);
    emitter.instruction(&format!("mov x9, #{}", nonblock));                     // the O_NONBLOCK flag bit
    emitter.instruction("tst x0, x9");                                          // is the O_NONBLOCK bit set?
    emitter.instruction("cset x10, eq");                                        // blocked = 1 when O_NONBLOCK is clear
    emitter.instruction("str x10, [sp, #24]");                                  // save the blocked flag
    emitter.instruction("and x9, x0, #3");                                      // isolate the O_ACCMODE access bits
    emitter.instruction("cmp x9, #1");                                          // O_WRONLY?
    emitter.instruction("b.eq __rt_sgmd_mode_w");                               // write-only stream
    emitter.instruction("cmp x9, #2");                                          // O_RDWR?
    emitter.instruction("b.eq __rt_sgmd_mode_rw");                              // read-write stream

    abi::emit_symbol_address(emitter, "x10", "_meta_mode_r");                   // load page of the "r" literal
    emitter.instruction("mov x11, #1");                                         // length of "r"
    emitter.instruction("b __rt_sgmd_mode_done");                               // mode resolved
    emitter.label("__rt_sgmd_mode_w");
    abi::emit_symbol_address(emitter, "x10", "_meta_mode_w");                   // load page of the "w" literal
    emitter.instruction("mov x11, #1");                                         // length of "w"
    emitter.instruction("b __rt_sgmd_mode_done");                               // mode resolved
    emitter.label("__rt_sgmd_mode_rw");
    abi::emit_symbol_address(emitter, "x10", "_meta_mode_rw");                  // load page of the "r+" literal
    emitter.instruction("mov x11, #2");                                         // length of "r+"
    emitter.label("__rt_sgmd_mode_done");
    // A mode recorded at open time is what PHP reports; the derivation above is only the
    // fallback for streams that never recorded one, and it cannot spell `a`, `w+` or `rb`.
    emitter.instruction("ldr x12, [sp, #0]");                                   // the opaque stream handle
    emitter.instruction("str x10, [sp, #40]");                                  // save the derived mode pointer
    emitter.instruction("str x11, [sp, #48]");                                  // save the derived mode length
    emitter.instruction("mov x0, x12");
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_sgmd_mode_kept");                         // no state: keep the derived spelling
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_MODE_PTR_OFFSET}]"));   // the recorded mode
    emitter.instruction("cbz x9, __rt_sgmd_mode_kept");                         // nothing recorded: keep the derived spelling
    emitter.instruction("str x9, [sp, #40]");                                   // report the recorded pointer
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_MODE_LEN_OFFSET}]"));
    emitter.instruction("str x9, [sp, #48]");                                   // and its length
    emitter.label("__rt_sgmd_mode_kept");

    // -- end-of-file flag from the authoritative StreamState --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_eof_get");                              // read the state-owned EOF predicate
    emitter.instruction("str x0, [sp, #32]");                                   // save the EOF flag

    // -- create the metadata hash (capacity 16, value type = mixed) --
    emitter.instruction("mov x0, #16");                                         // initial capacity
    emitter.instruction("mov x1, #7");                                          // value type = mixed
    emitter.instruction("bl __rt_hash_new");                                    // allocate the hash; x0 = hash pointer
    emitter.instruction("str x0, [sp, #8]");                                    // save the hash pointer

    // The ORDER is php's, not an arrangement of convenience: this array is routinely dumped whole
    // with `print_r()` or `var_export()`, and a PHP array remembers insertion order, so a
    // differently-ordered array with identical contents still prints differently. php-src fills it
    // in `_php_stream_get_metadata` — the three fallback flags first, then wrapper_type,
    // stream_type, mode, unread_bytes, seekable — and finally `uri`. elephc had `unread_bytes`
    // third and `stream_type` ahead of `wrapper_type`. Measured key-by-key on `php -n` 8.5.6.
    // The three fallback flags are NOT unconditional: php-src emits them only for a stream that
    // answers `PHP_STREAM_OPTION_META_DATA_API`, and `php://temp` and `data:` do not. Measured on
    // `php -n` 8.5.6, `php://temp` reports six keys where `php://memory` reports nine.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_meta_has_api_flags");                   // does this stream answer the API?
    emitter.instruction("cbz x0, __rt_sgmd_no_api_flags");                      // it does not: skip all three
    emit_set_bool_const(emitter, "_meta_key_timed_out", 9, 0);
    emit_set_bool_slot(emitter, "_meta_key_blocked", 7, 24);
    emit_set_bool_slot(emitter, "_meta_key_eof", 3, 32);
    emitter.label("__rt_sgmd_no_api_flags");
    // A `data:` stream's own URI supplies `mediatype`, its `name=value` parameters and `base64`,
    // and php writes all of them BEFORE `wrapper_type`.
    emitter.instruction("ldr x6, [sp, #80]");                                   // the stable StreamState pointer
    emitter.instruction(&format!("ldr x7, [x6, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
    emitter.instruction(&format!("cmp x7, #{WRAPPER_ID_DATA}"));
    emitter.instruction("b.ne __rt_sgmd_not_data");                             // no other wrapper carries them
    emitter.instruction(&format!("ldr x1, [x6, #{STREAM_URI_PTR_OFFSET}]"));    // the recorded URI
    emitter.instruction(&format!("ldr x2, [x6, #{STREAM_URI_LEN_OFFSET}]"));    // and its length
    emitter.instruction("cbz x1, __rt_sgmd_not_data");                          // nothing recorded: nothing to parse
    emitter.instruction("ldr x0, [sp, #8]");                                    // the hash being filled
    emitter.instruction("bl __rt_stream_meta_data_uri_params");
    emitter.instruction("str x0, [sp, #8]");                                    // persist any post-grow hash pointer
    emitter.label("__rt_sgmd_not_data");
    // -- wrapper_data: the response header lines, for the wrappers that HAVE a response --
    // php-src's `php_stream_url_wrap_http` stores the same `zval` it publishes as
    // `$http_response_header` into `stream->wrapperdata`, and `_php_stream_get_metadata` copies
    // it under `wrapper_data` — status line first, then every header, in the order received, and
    // written BEFORE `wrapper_type`. elephc published the global and stopped there, so the key
    // was simply absent. Measured on `php -n` 8.5.6 against a local server.
    emit_set_wrapper_data_aarch64(emitter);
    // -- wrapper_type: map the StreamState wrapper id to its PHP-visible literal --
    emit_set_wrapper_type_aarch64(emitter);
    emit_set_str_slots(emitter, "_meta_key_stream_type", 11, 56, 64);
    emit_set_owned_str_slots(emitter, "_meta_key_mode", 4, 40, 48);
    emit_set_int_const(emitter, "_meta_key_unread_bytes", 12);
    emit_set_bool_slot(emitter, "_meta_key_seekable", 8, 16);
    // -- uri: read the StreamState-owned URI pointer/length pair --
    emit_set_uri_aarch64(emitter);

    // -- return the completed hash --
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the final hash pointer
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the metadata frame
    emitter.instruction("ret");                                                 // return the metadata hash pointer

    emit_stream_fd_is_regular_aarch64(emitter);
    emit_stream_meta_has_api_flags_aarch64(emitter);
    emit_data_uri_params_aarch64(emitter);
}

/// Emits `__rt_stream_meta_data_uri_params(hash, uri_ptr, uri_len) -> hash`, which adds the
/// metadata keys a `data:` URI carries in its own text.
///
/// php-src's `php_stream_url_wrap_rfc2397` builds the metadata array while it PARSES the URI, so
/// the keys come out in the order they were written: `mediatype` when the URI spells one, then
/// every `;name=value` parameter under its own name, then `base64` — which is emitted even when
/// false. Measured on `php -n` 8.5.6, `data://text/plain;charset=utf-8;foo=bar,x` yields
/// `mediatype`, `charset`, `foo`, `base64` before `wrapper_type`.
///
/// The head is everything before the first comma. `base64` is recognised only as a whole
/// parameter and only in lower case, which is the same rule the opener enforces — php answers
/// `rfc2397: illegal parameter` for `;BASE64` and for a `base64` that is not last.
///
/// All loop state lives in the frame rather than in callee-saved registers, because the helpers
/// this calls are hand-written assembly whose register discipline is not part of their contract.
fn emit_data_uri_params_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: metadata keys carried by a data: URI ---");
    emitter.label_global("__rt_stream_meta_data_uri_params");
    // Frame (96 bytes): [0]=hash [8]=seg_start [16]=comma [24]=base64 [32]=head_start
    //                   [40]=seg_end [48]=uri_end [56]=key_ptr [64]=key_len [80]=x29 [88]=x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the parser frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // the hash being filled
    emitter.instruction("mov x9, #0");
    emitter.instruction("str x9, [sp, #24]");                                   // base64 = false until a parameter says otherwise

    emitter.instruction("add x10, x1, x2");                                     // one past the last URI byte
    emitter.instruction("str x10, [sp, #48]");
    emitter.instruction("cmp x2, #5");                                          // shorter than "data:"?
    emitter.instruction("b.lo __rt_sgmdp_base");                                // nothing to parse; still emit base64
    emitter.instruction("add x11, x1, #5");                                     // skip the "data:" scheme
    // php-src special-cases this one wrapper so the `//` is optional.
    emitter.instruction("sub x12, x10, x11");                                   // bytes left after the scheme
    emitter.instruction("cmp x12, #2");
    emitter.instruction("b.lo __rt_sgmdp_noslash");
    emitter.instruction("ldrb w13, [x11, #0]");
    emitter.instruction("cmp w13, #0x2F");                                      // '/'
    emitter.instruction("b.ne __rt_sgmdp_noslash");
    emitter.instruction("ldrb w13, [x11, #1]");
    emitter.instruction("cmp w13, #0x2F");                                      // '/'
    emitter.instruction("b.ne __rt_sgmdp_noslash");
    emitter.instruction("add x11, x11, #2");                                    // skip the optional "//"
    emitter.label("__rt_sgmdp_noslash");
    emitter.instruction("str x11, [sp, #32]");                                  // head_start, which is also the media type
    emitter.instruction("str x11, [sp, #8]");                                   // the first segment starts there

    emitter.instruction("mov x12, x11");                                        // scan for the comma that ends the head
    emitter.label("__rt_sgmdp_comma");
    emitter.instruction("cmp x12, x10");
    emitter.instruction("b.hs __rt_sgmdp_comma_done");                          // no comma: the head is the whole URI
    emitter.instruction("ldrb w13, [x12]");
    emitter.instruction("cmp w13, #0x2C");                                      // ','
    emitter.instruction("b.eq __rt_sgmdp_comma_done");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_sgmdp_comma");
    emitter.label("__rt_sgmdp_comma_done");
    emitter.instruction("str x12, [sp, #16]");                                  // the head ends here

    emitter.label("__rt_sgmdp_seg");
    emitter.instruction("ldr x9, [sp, #8]");                                    // seg_start
    emitter.instruction("ldr x10, [sp, #16]");                                  // comma
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.hi __rt_sgmdp_base");                                // walked past the head: done
    emitter.instruction("mov x11, x9");                                         // scan for this segment's ';'
    emitter.label("__rt_sgmdp_semi");
    emitter.instruction("cmp x11, x10");
    emitter.instruction("b.hs __rt_sgmdp_semi_done");
    emitter.instruction("ldrb w13, [x11]");
    emitter.instruction("cmp w13, #0x3B");                                      // ';'
    emitter.instruction("b.eq __rt_sgmdp_semi_done");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_sgmdp_semi");
    emitter.label("__rt_sgmdp_semi_done");
    emitter.instruction("str x11, [sp, #40]");                                  // seg_end
    emitter.instruction("ldr x12, [sp, #32]");                                  // head_start
    emitter.instruction("cmp x9, x12");
    emitter.instruction("b.eq __rt_sgmdp_media");                               // the first segment is the media type

    // -- a `;`-separated parameter: either the bare word base64, or name=value --
    emitter.instruction("sub x13, x11, x9");                                    // segment length
    emitter.instruction("cmp x13, #6");
    emitter.instruction("b.ne __rt_sgmdp_kv");
    for (offset, byte) in b"base64".iter().enumerate() {
        emitter.instruction(&format!("ldrb w14, [x9, #{}]", offset));           // one candidate byte of "base64"
        emitter.instruction(&format!("cmp w14, #{}", byte));
        emitter.instruction("b.ne __rt_sgmdp_kv");                              // a different word is a name=value parameter
    }
    emitter.instruction("mov x14, #1");
    emitter.instruction("str x14, [sp, #24]");                                  // base64 = true
    emitter.instruction("b __rt_sgmdp_next");

    emitter.label("__rt_sgmdp_kv");
    emitter.instruction("mov x14, x9");                                         // scan for the '=' that splits name from value
    emitter.label("__rt_sgmdp_eq");
    emitter.instruction("cmp x14, x11");
    emitter.instruction("b.hs __rt_sgmdp_next");                                // no '=': php refuses such a URI outright
    emitter.instruction("ldrb w15, [x14]");
    emitter.instruction("cmp w15, #0x3D");                                      // '='
    emitter.instruction("b.eq __rt_sgmdp_eq_done");
    emitter.instruction("add x14, x14, #1");
    emitter.instruction("b __rt_sgmdp_eq");
    emitter.label("__rt_sgmdp_eq_done");
    emitter.instruction("str x9, [sp, #56]");                                   // the name starts at the segment
    emitter.instruction("sub x15, x14, x9");
    emitter.instruction("str x15, [sp, #64]");                                  // and runs up to the '='
    emitter.instruction("add x1, x14, #1");                                     // the value starts after the '='
    emitter.instruction("sub x2, x11, x1");                                     // and runs to the end of the segment
    emitter.instruction("bl __rt_str_persist");                                 // the array releases its values, so it needs its own
    emitter.instruction("mov x3, x1");                                          // value_lo
    emitter.instruction("mov x4, x2");                                          // value_hi
    emitter.instruction("mov x5, #1");                                          // value tag = string
    emitter.instruction("ldr x0, [sp, #0]");                                    // the hash
    emitter.instruction("ldr x1, [sp, #56]");                                   // the parameter name as the key
    emitter.instruction("ldr x2, [sp, #64]");
    emitter.instruction("bl __rt_hash_set");
    emitter.instruction("str x0, [sp, #0]");                                    // persist any post-grow hash pointer
    emitter.instruction("b __rt_sgmdp_next");

    emitter.label("__rt_sgmdp_media");
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.eq __rt_sgmdp_next");                                // `data:,x` spells no media type, so php emits no key
    emitter.instruction("mov x1, x9");
    emitter.instruction("sub x2, x11, x9");
    emitter.instruction("bl __rt_str_persist");                                 // give the array a copy it may release
    emitter.instruction("mov x3, x1");                                          // value_lo
    emitter.instruction("mov x4, x2");                                          // value_hi
    emitter.instruction("mov x5, #1");                                          // value tag = string
    emitter.instruction("ldr x0, [sp, #0]");                                    // the hash
    abi::emit_symbol_address(emitter, "x1", "_meta_key_mediatype");
    emitter.instruction("mov x2, #9");                                          // length of "mediatype"
    emitter.instruction("bl __rt_hash_set");
    emitter.instruction("str x0, [sp, #0]");                                    // persist any post-grow hash pointer

    emitter.label("__rt_sgmdp_next");
    emitter.instruction("ldr x11, [sp, #40]");                                  // seg_end
    emitter.instruction("add x11, x11, #1");                                    // step over its ';'
    emitter.instruction("str x11, [sp, #8]");
    emitter.instruction("b __rt_sgmdp_seg");

    emitter.label("__rt_sgmdp_base");
    emitter.instruction("ldr x3, [sp, #24]");                                   // value_lo = the base64 verdict
    emitter.instruction("mov x4, #0");                                          // value_hi unused for booleans
    emitter.instruction("mov x5, #3");                                          // value tag = bool
    emitter.instruction("ldr x0, [sp, #0]");                                    // the hash
    abi::emit_symbol_address(emitter, "x1", "_meta_key_base64");
    emitter.instruction("mov x2, #6");                                          // length of "base64"
    emitter.instruction("bl __rt_hash_set");                                    // php emits this key even when it is false
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the parser frame
    emitter.instruction("ret");                                                 // x0 = the updated hash
}

/// Emits `__rt_stream_fd_is_regular(fd) -> 1|0`, the S_ISREG probe behind `seekable`.
///
/// It needs its own frame because a `struct stat` does not fit beside the metadata helper's
/// spill slots. A descriptor that cannot be stat'd — a userspace wrapper's `-1`, a closed fd —
/// answers 0, which is the same non-seekable verdict the lseek probe used to give it.
fn emit_stream_fd_is_regular_aarch64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size();
    let frame_size = (stat_buf + 32 + 15) & !15;
    let save_offset = frame_size - 16;
    let mode_off = plat.stat_mode_offset();

    emitter.blank();
    emitter.comment("--- runtime: is the stream descriptor a regular file (S_ISREG) ---");
    emitter.label_global("__rt_stream_fd_is_regular");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // stat buffer plus saved linkage
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish the helper frame pointer
    emitter.instruction("add x1, sp, #0");                                      // second argument: the stat buffer
    emitter.syscall(339);                                                       // fstat(fd, buf)
    emitter.instruction("cmp x0, #0");                                          // did it succeed?
    emitter.instruction("b.ne __rt_sfir_no");                                   // an unstattable descriptor is not a file
    emitter.instruction(&plat.stat_mode_load_instr("w9", "sp", mode_off));      // load st_mode
    emitter.instruction("and w9, w9, #0xF000");                                 // keep only the file-type bits (S_IFMT)
    emitter.instruction("mov w10, #0x8000");                                    // S_IFREG
    emitter.instruction("cmp w9, w10");                                         // a regular file?
    emitter.instruction("cset x0, eq");                                         // that is the answer
    emitter.instruction("b __rt_sfir_ret");
    emitter.label("__rt_sfir_no");
    emitter.instruction("mov x0, #0");                                          // not a regular file
    emitter.label("__rt_sfir_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // release the frame
    emitter.instruction("ret");
}

/// Emits `__rt_stream_meta_has_api_flags(handle) -> 1|0`, which decides whether
/// `timed_out`/`blocked`/`eof` belong in the metadata array at all.
///
/// php-src emits them from `_php_stream_get_metadata` only when the stream answers
/// `PHP_STREAM_OPTION_META_DATA_API`. The `php://` wrapper answers it for `memory` but not for
/// `temp`, and `data:` never answers it. The php:// sub-wrapper is told apart by the byte after
/// `php://`, the same discriminator `__rt_stream_type_name` uses — `t` is `temp` and nothing
/// else. A stream with no recorded state keeps the flags, which is what every other wrapper does.
fn emit_stream_meta_has_api_flags_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: does this stream answer PHP_STREAM_OPTION_META_DATA_API ---");
    emitter.label_global("__rt_stream_meta_has_api_flags");
    emitter.instruction("sub sp, sp, #16");                                     // frame for the saved linkage
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_smaf_yes");                               // no state: keep the flags
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_DATA}"));
    emitter.instruction("b.eq __rt_smaf_no");                                   // data: answers nothing
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_PHP}"));
    emitter.instruction("b.ne __rt_smaf_yes");                                  // every other wrapper answers
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_URI_PTR_OFFSET}]"));   // the recorded URI
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_URI_LEN_OFFSET}]"));   // and its length
    emitter.instruction("cbz x10, __rt_smaf_yes");                              // no URI: keep the flags
    emitter.instruction("cmp x11, #7");                                         // "php://" plus the naming byte
    emitter.instruction("b.lt __rt_smaf_yes");
    emitter.instruction("ldrb w12, [x10, #6]");                                 // the first byte of the php:// name
    emitter.instruction("cmp w12, #0x74");                                      // 't' as in temp
    emitter.instruction("b.eq __rt_smaf_no");
    emitter.label("__rt_smaf_yes");
    emitter.instruction("mov x0, #1");                                          // the flags belong in the array
    emitter.instruction("b __rt_smaf_ret");
    emitter.label("__rt_smaf_no");
    emitter.instruction("mov x0, #0");                                          // php omits all three here
    emitter.label("__rt_smaf_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");
}

/// Reads the StreamState wrapper id and loads the matching wrapper-type string
/// literal into x3 (ptr) / x4 (len) / x5 (tag=1), then inserts the hash entry.
/// Fallback id 0 (unset) → "plainfile".
///
/// A socket has NO wrapper: php-src reaches every transport through
/// `php_stream_xport_create`, which never assigns `stream->wrapper`, and
/// `_php_stream_get_metadata` writes the key only `if (stream->wrapper)`. So a socket-pair end, a
/// `stream_socket_server()` and a `stream_socket_client()` all report the key as ABSENT, not as
/// some default. elephc left the id at its unset value 0, which the table below maps to
/// "plainfile", so every socket claimed to have been opened by the plain-files wrapper. Measured
/// on `php -n` 8.5.6.
fn emit_set_wrapper_type_aarch64(emitter: &mut Emitter) {
    let wrappers: &[(&str, i64)] = &[
        ("_meta_wrapper_plainfile", 9),
        ("_meta_wrapper_http", 4),
        ("_meta_wrapper_https", 5),
        ("_meta_wrapper_ftp", 3),
        ("_meta_wrapper_ftps", 4),
        ("_meta_wrapper_phar", 4),
        ("_meta_wrapper_php", 3),
        ("_meta_wrapper_data", 7),
        ("_meta_wrapper_zlib", 13),
        ("_meta_wrapper_bzip2", 14),
        ("_meta_wrapper_glob", 4),
        ("_meta_wrapper_user", 10),
        ("_meta_wrapper_zip", 11),
    ];
    emitter.instruction("ldr x6, [sp, #80]");                                   // reload the stable StreamState pointer
    emitter.instruction(&format!(
        "ldr x7, [x6, #{}]", STREAM_TRANSPORT_OFFSET
    ));                                                                         // non-zero only for a socket transport
    emitter.instruction("cbnz x7, __rt_sgmd_wtype_done");                       // a transport has no wrapper, so php omits the key
    emitter.instruction(&format!(
        "ldr x7, [x6, #{}]", STREAM_WRAPPER_ID_OFFSET
    ));                                                                         // load the handle-keyed wrapper id
    // Compare-and-branch chain: each comparison branches to a label that is
    // emitted after all comparisons, so the fall-through goes to the next
    // comparison rather than into the literal-load block.
    for (id, _) in wrappers.iter().enumerate() {
        let label = format!("__rt_sgmd_wid_{}", id);
        emitter.instruction(&format!("cmp w7, #{}", id));                       // compare wrapper id
        emitter.instruction(&format!("b.eq {}", label));                        // branch to the matching literal load
    }
    // Fallback for unknown ids: use plainfile (same as id 0).
    abi::emit_symbol_address(emitter, "x3", "_meta_wrapper_plainfile");          // fallback wrapper name
    emitter.instruction("mov x4, #9");                                          // plainfile length
    emitter.instruction("mov x5, #1");                                          // value tag = string
    emitter.instruction("b __rt_sgmd_wtype_put");                               // jump to the shared hash insert
    // Literal-load blocks (emitted after the compare chain).
    for (id, (sym, len)) in wrappers.iter().enumerate() {
        let label = format!("__rt_sgmd_wid_{}", id);
        emitter.label(&label);
        abi::emit_symbol_address(emitter, "x3", sym);                            // load the wrapper-name literal address
        emitter.instruction(&format!("mov x4, #{}", len));                      // wrapper-name length
        emitter.instruction("mov x5, #1");                                      // value tag = string
        emitter.instruction("b __rt_sgmd_wtype_put");                           // jump to the shared hash insert
    }
    emitter.label("__rt_sgmd_wtype_put");
    emit_hash_put_aarch64(emitter, "_meta_key_wrapper_type", 12);
    emitter.label("__rt_sgmd_wtype_done");                                      // a socket rejoins the caller here
}

/// Inserts `wrapper_data`, the response header lines, for the wrappers that receive a response.
///
/// The array comes from `__rt_get_http_response_headers`, the SAME source that fills
/// `$http_response_header` — php-src publishes one `zval` into both places, so the two can never
/// disagree about what the server said. It is built fresh on each call, which is what the hash
/// needs: a value under a mixed-typed key is released when the entry is replaced or the array is
/// freed, so it must not be anything else's allocation.
///
/// Only `http` and `https` reach it. Every other wrapper leaves `wrapper_data` out entirely
/// rather than reporting an empty array, which is what php does — `data:` and `php://memory`
/// have no `wrapperdata` at all.
fn emit_set_wrapper_data_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x6, [sp, #80]");                                   // the stable StreamState pointer
    emitter.instruction(&format!("ldr x7, [x6, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
    emitter.instruction(&format!("cmp x7, #{WRAPPER_ID_HTTP}"));
    emitter.instruction("b.eq __rt_sgmd_wrapper_data");
    emitter.instruction(&format!("cmp x7, #{WRAPPER_ID_HTTPS}"));
    emitter.instruction("b.ne __rt_sgmd_no_wrapper_data");                      // no response, so no key
    emitter.label("__rt_sgmd_wrapper_data");
    emitter.instruction("bl __rt_get_http_response_headers");                   // x0 = a fresh indexed array
    emitter.instruction("mov x3, x0");                                          // value_lo = the array
    emitter.instruction("mov x4, #0");                                          // value_hi unused for arrays
    emitter.instruction("mov x5, #4");                                          // value tag = indexed array
    emit_hash_put_aarch64(emitter, "_meta_key_wrapper_data", 12);
    emitter.label("__rt_sgmd_no_wrapper_data");
}

/// Reads the StreamState URI pointer/length pair and loads it into
/// x3 (ptr) / x4 (len) / x5 (tag=1), then inserts the hash entry.
///
/// A stream with NO recorded path contributes no key at all. php guards the insertion with
/// `if (stream->orig_path)` (`ext/standard/streamsfuncs.c`), so a directory handle and a socket
/// — neither of which carries an `orig_path` — answer EIGHT keys. elephc inserted
/// `["uri"] => ""` for them, which made every `count()` and every key-set assertion over
/// `stream_get_meta_data()` disagree with php on exactly those two stream kinds.
fn emit_set_uri_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x6, [sp, #80]");                                   // reload the stable StreamState pointer
    emitter.instruction(&format!(
        "ldr x3, [x6, #{}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // load the handle-keyed URI pointer
    emitter.instruction(&format!(
        "ldr x4, [x6, #{}]", STREAM_URI_LEN_OFFSET
    ));                                                                         // load the handle-keyed URI byte length
    emitter.instruction("cbz x3, __rt_sgmd_uri_absent");                        // no recorded path → php emits no `uri` key
    emitter.instruction("cbz x4, __rt_sgmd_uri_absent");                        // a zero-length path is no path either
    // The array releases its string values, so handing it the StreamState's own URI allocation
    // freed the state's copy: the third `stream_get_meta_data()` on a stream then read a block
    // that intervening hash keys had already reused. Give the array a duplicate it can own.
    emitter.instruction("mov x1, x3");                                          // duplicate the URI bytes
    emitter.instruction("mov x2, x4");                                          // with their length
    emitter.instruction("bl __rt_str_persist");                                 // into storage the array may release
    emitter.instruction("mov x3, x1");                                          // value_lo = the owned duplicate
    emitter.instruction("mov x4, x2");                                          // value_hi = its length
    emitter.instruction("mov x5, #1");                                          // value tag = string
    emit_hash_put_aarch64(emitter, "_meta_key_uri", 3);
    emitter.label("__rt_sgmd_uri_absent");                                      // pathless stream: the key is simply not there
}

/// Emit one `__rt_hash_set` with the value already staged in x3/x4/x5.
fn emit_hash_put_aarch64(emitter: &mut Emitter, key_sym: &str, key_len: i64) {
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    abi::emit_symbol_address(emitter, "x1", key_sym);                           // load page of the key literal
    emitter.instruction(&format!("mov x2, #{}", key_len));                      // key length
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry; x0 = updated hash
    emitter.instruction("str x0, [sp, #8]");                                    // persist any post-grow hash pointer
}

/// Emits the set bool const stream runtime helper.
fn emit_set_bool_const(emitter: &mut Emitter, key_sym: &str, key_len: i64, value: i64) {
    emitter.instruction(&format!("mov x3, #{}", value));                        // value_lo = boolean payload
    emitter.instruction("mov x4, #0");                                          // value_hi unused for booleans
    emitter.instruction("mov x5, #3");                                          // value tag = bool
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the set bool slot stream runtime helper.
fn emit_set_bool_slot(emitter: &mut Emitter, key_sym: &str, key_len: i64, slot: i64) {
    emitter.instruction(&format!("ldr x3, [sp, #{}]", slot));                   // value_lo = computed boolean
    emitter.instruction("mov x4, #0");                                          // value_hi unused for booleans
    emitter.instruction("mov x5, #3");                                          // value tag = bool
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the set int const stream runtime helper.
fn emit_set_int_const(emitter: &mut Emitter, key_sym: &str, key_len: i64) {
    emitter.instruction("mov x3, #0");                                          // value_lo = 0 (elephc keeps no read buffer)
    emitter.instruction("mov x4, #0");                                          // value_hi unused for integers
    emitter.instruction("mov x5, #0");                                          // value tag = int
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the set str slots stream runtime helper.
fn emit_set_str_slots(emitter: &mut Emitter, key_sym: &str, key_len: i64, ptr_slot: i64, len_slot: i64) {
    emitter.instruction(&format!("ldr x3, [sp, #{}]", ptr_slot));               // value_lo = string pointer
    emitter.instruction(&format!("ldr x4, [sp, #{}]", len_slot));               // value_hi = string length
    emitter.instruction("mov x5, #1");                                          // value tag = string
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits one `__rt_hash_set` whose string value is duplicated first.
///
/// See the URI insertion: a value the array may release must not be the StreamState's own
/// allocation. A rodata fallback survives either way, so this is uniform for both.
fn emit_set_owned_str_slots(
    emitter: &mut Emitter,
    key_sym: &str,
    key_len: i64,
    ptr_slot: i64,
    len_slot: i64,
) {
    emitter.instruction(&format!("ldr x1, [sp, #{}]", ptr_slot));               // duplicate the recorded bytes
    emitter.instruction(&format!("ldr x2, [sp, #{}]", len_slot));               // with their length
    emitter.instruction("bl __rt_str_persist");                                 // into storage the array may release
    emitter.instruction("mov x3, x1");                                          // value_lo = the owned duplicate
    emitter.instruction("mov x4, x2");                                          // value_hi = its length
    emitter.instruction("mov x5, #1");                                          // value tag = string
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the Linux x86_64 stream runtime helper for stream get meta data.
fn emit_stream_get_meta_data_linux_x86_64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let nonblock = plat.o_nonblock();

    emitter.blank();
    emitter.comment("--- runtime: stream_get_meta_data ---");
    emitter.label_global("__rt_stream_get_meta_data");

    // Frame (rbp-relative): [-8]=handle [-16]=hash [-24]=seekable [-32]=blocked
    //                       [-40]=eof [-48]=mode_ptr [-56]=mode_len
    //                       [-64]=stype_ptr [-72]=stype_len [-80]=backend fd
    //                       [-88]=StreamState
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 96");                                         // reserve aligned metadata spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // resolve handle-keyed metadata and backend state
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // preserve StreamState across metadata hash construction
    emitter.instruction(&format!(
        "mov rdi, QWORD PTR [rax + {}]", STREAM_FD_OFFSET
    ));                                                                         // load the backend descriptor for native probes
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // preserve the descriptor for native metadata probes

    // -- seekability: lseek(fd, 0, SEEK_CUR) --
    emitter.instruction("xor esi, esi");                                        // offset 0
    emitter.instruction("mov edx, 1");                                          // SEEK_CUR
    emitter.instruction("mov eax, 8");                                          // Linux x86_64 syscall 8 = lseek
    emitter.instruction("syscall");                                             // probe whether the descriptor is seekable
    emitter.instruction("test rax, rax");                                       // did lseek fail with a negative result?
    emitter.instruction("jns __rt_sgmd_seekable_x86");                          // lseek ok: the stream is seekable

    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // seekable = false
    abi::emit_symbol_address(emitter, "r10", "_meta_stype_socket");             // address of the "tcp_socket" literal
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save the stream_type pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], 10");                        // save the stream_type length
    emitter.instruction("jmp __rt_sgmd_seek_done_x86");                         // skip the seekable branch

    emitter.label("__rt_sgmd_seekable_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // seekable = true
    abi::emit_symbol_address(emitter, "r10", "_meta_stype_stdio");              // address of the "STDIO" literal
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save the stream_type pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], 5");                         // save the stream_type length
    emitter.label("__rt_sgmd_seek_done_x86");

    // See the AArch64 counterpart: a recorded identity outranks the seekability-derived name.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_type_name");                          // rax = name or 0, rdx = length
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_sgmd_stype_kept_x86");                         // nothing recorded: keep the derived name
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // report the recorded name
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // and its length
    emitter.label("__rt_sgmd_stype_kept_x86");
    // See the AArch64 counterpart: `seekable` is S_ISREG, not whether lseek worked.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // the backend descriptor
    emitter.instruction("call __rt_stream_fd_is_regular");                      // S_ISREG, the question php asks
    // See the AArch64 counterpart: `ext/zip`'s entry ops have no `seek`, so php reports the
    // stream as unseekable however regular the file elephc serves it from happens to be.
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // the stable StreamState pointer
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [r10 + {STREAM_WRAPPER_ID_OFFSET}]"
    ));                                                                         // which wrapper opened it
    emitter.instruction(&format!("cmp r10, {}", super::WRAPPER_ID_ZIP));        // the zip wrapper?
    emitter.instruction("mov r11d, 0");                                         // the unseekable answer
    emitter.instruction("cmove rax, r11");                                      // a zip entry stream never seeks
    // ...and a DIRECTORY always does — see the AArch64 arm for php's "ask the ops, not the
    // descriptor" rule, which this case applies in the other direction.
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // the stable StreamState pointer
    emitter.instruction(&format!("mov r10, QWORD PTR [r10 + {STREAM_BACKEND_KIND_OFFSET}]")); // what backs the stream
    emitter.instruction(&format!("cmp r10, {STREAM_BACKEND_DIRECTORY}"));
    emitter.instruction("je __rt_sgmd_dir_seekable_x86");
    emitter.instruction(&format!("cmp r10, {STREAM_BACKEND_USER_DIRECTORY}"));
    emitter.instruction("je __rt_sgmd_dir_seekable_x86");
    emitter.instruction(&format!("cmp r10, {STREAM_BACKEND_GLOB_DIRECTORY}"));
    emitter.instruction("jne __rt_sgmd_dir_seek_done_x86");
    emitter.label("__rt_sgmd_dir_seekable_x86");
    emitter.instruction("mov rax, 1");                                          // a directory handle is seekable
    emitter.label("__rt_sgmd_dir_seek_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // that is the seekable flag

    // -- blocking mode + access mode: fcntl(fd, F_GETFL, 0) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // reload the stream descriptor
    emitter.instruction("mov esi, 3");                                          // F_GETFL
    emitter.instruction("xor edx, edx");                                        // unused third argument
    emitter.instruction("mov eax, 72");                                         // Linux x86_64 syscall 72 = fcntl
    emitter.instruction("syscall");                                             // read the descriptor flags
    emitter.instruction(&format!("mov r9d, {}", nonblock));                     // the O_NONBLOCK flag bit
    emitter.instruction("test rax, r9");                                        // is the O_NONBLOCK bit set?
    emitter.instruction("sete r10b");                                           // blocked = 1 when O_NONBLOCK is clear
    emitter.instruction("movzx r10, r10b");                                     // widen the blocked flag to a full word
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the blocked flag
    emitter.instruction("and rax, 3");                                          // isolate the O_ACCMODE access bits
    emitter.instruction("cmp rax, 1");                                          // O_WRONLY?
    emitter.instruction("je __rt_sgmd_mode_w_x86");                             // write-only stream
    emitter.instruction("cmp rax, 2");                                          // O_RDWR?
    emitter.instruction("je __rt_sgmd_mode_rw_x86");                            // read-write stream

    abi::emit_symbol_address(emitter, "r10", "_meta_mode_r");                   // address of the "r" literal
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the mode pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // save the mode length
    emitter.instruction("jmp __rt_sgmd_mode_done_x86");                         // mode resolved
    emitter.label("__rt_sgmd_mode_w_x86");
    abi::emit_symbol_address(emitter, "r10", "_meta_mode_w");                   // address of the "w" literal
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the mode pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // save the mode length
    emitter.instruction("jmp __rt_sgmd_mode_done_x86");                         // mode resolved
    emitter.label("__rt_sgmd_mode_rw_x86");
    abi::emit_symbol_address(emitter, "r10", "_meta_mode_rw");                  // address of the "r+" literal
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the mode pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 2");                         // save the mode length
    emitter.label("__rt_sgmd_mode_done_x86");
    // See the AArch64 counterpart: a mode recorded at open time is what PHP reports, and the
    // derivation above is only the fallback for streams that never recorded one.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_sgmd_mode_kept_x86");                          // no state: keep the derived spelling
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_MODE_PTR_OFFSET}]"
    ));                                                                         // the recorded mode
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_sgmd_mode_kept_x86");                          // nothing recorded: keep the derived spelling
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // report the recorded pointer
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_MODE_LEN_OFFSET}]"
    ));
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // and its length
    emitter.label("__rt_sgmd_mode_kept_x86");

    // -- end-of-file flag from the authoritative StreamState --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_eof_get");                            // read the state-owned EOF predicate
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the EOF flag

    // -- create the metadata hash (capacity 16, value type = mixed) --
    emitter.instruction("mov rdi, 16");                                         // initial capacity
    emitter.instruction("mov rsi, 7");                                          // value type = mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the hash; rax = hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the hash pointer

    // See the AArch64 counterpart: the insertion ORDER is php's, and it is observable.
    // See the AArch64 counterpart: `php://temp` and `data:` answer no metadata API, so php emits
    // none of the three fallback flags for them.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_meta_has_api_flags");                 // does this stream answer the API?
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_sgmd_no_api_flags_x86");                       // it does not: skip all three
    emit_set_bool_const_x86(emitter, "_meta_key_timed_out", 9, 0);
    emit_set_bool_slot_x86(emitter, "_meta_key_blocked", 7, 32);
    emit_set_bool_slot_x86(emitter, "_meta_key_eof", 3, 40);
    emitter.label("__rt_sgmd_no_api_flags_x86");
    // See the AArch64 counterpart: a `data:` URI's own keys come BEFORE `wrapper_type`.
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // the stable StreamState pointer
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [r10 + {STREAM_WRAPPER_ID_OFFSET}]"
    ));                                                                         // which wrapper opened it
    emitter.instruction(&format!("cmp r11, {WRAPPER_ID_DATA}"));
    emitter.instruction("jne __rt_sgmd_not_data_x86");                          // no other wrapper carries them
    emitter.instruction(&format!(
        "mov rsi, QWORD PTR [r10 + {STREAM_URI_PTR_OFFSET}]"
    ));                                                                         // the recorded URI
    emitter.instruction(&format!(
        "mov rdx, QWORD PTR [r10 + {STREAM_URI_LEN_OFFSET}]"
    ));                                                                         // and its length
    emitter.instruction("test rsi, rsi");
    emitter.instruction("jz __rt_sgmd_not_data_x86");                           // nothing recorded: nothing to parse
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the hash being filled
    emitter.instruction("call __rt_stream_meta_data_uri_params");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // persist any post-grow hash pointer
    emitter.label("__rt_sgmd_not_data_x86");
    // See the AArch64 counterpart: `wrapper_data` carries the response headers, before
    // `wrapper_type` and only for a wrapper that had a response.
    emit_set_wrapper_data_x86(emitter);
    // -- wrapper_type: map the StreamState wrapper id to its PHP-visible literal --
    emit_set_wrapper_type_x86(emitter);
    emit_set_str_slots_x86(emitter, "_meta_key_stream_type", 11, 64, 72);
    emit_set_owned_str_slots_x86(emitter, "_meta_key_mode", 4, 48, 56);
    emit_set_int_const_x86(emitter, "_meta_key_unread_bytes", 12);
    emit_set_bool_slot_x86(emitter, "_meta_key_seekable", 8, 24);
    // -- uri: read the StreamState-owned URI pointer/length pair --
    emit_set_uri_x86(emitter);

    // -- return the completed hash --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the final hash pointer
    emitter.instruction("add rsp, 96");                                         // release the metadata spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the metadata hash pointer
}

/// The x86_64 counterpart of [`emit_set_wrapper_data_aarch64`].
fn emit_set_wrapper_data_x86(emitter: &mut Emitter) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // the stable StreamState pointer
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [r10 + {STREAM_WRAPPER_ID_OFFSET}]"
    ));                                                                         // which wrapper opened it
    emitter.instruction(&format!("cmp r11, {WRAPPER_ID_HTTP}"));
    emitter.instruction("je __rt_sgmd_wrapper_data_x86");
    emitter.instruction(&format!("cmp r11, {WRAPPER_ID_HTTPS}"));
    emitter.instruction("jne __rt_sgmd_no_wrapper_data_x86");                   // no response, so no key
    emitter.label("__rt_sgmd_wrapper_data_x86");
    emitter.instruction("call __rt_get_http_response_headers");                 // rax = a fresh indexed array
    emitter.instruction("mov rcx, rax");                                        // value_lo = the array
    emitter.instruction("xor r8d, r8d");                                        // value_hi unused for arrays
    emitter.instruction("mov r9, 4");                                           // value tag = indexed array
    emit_hash_put_x86(emitter, "_meta_key_wrapper_data", 12);
    emitter.label("__rt_sgmd_no_wrapper_data_x86");
}

/// Emit one `__rt_hash_set` with the value already staged in rcx/r8/r9.
fn emit_hash_put_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64) {
    abi::emit_symbol_address(emitter, "rsi", key_sym);                          // key pointer
    emitter.instruction(&format!("mov rdx, {}", key_len));                      // key length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // hash pointer (first argument)
    emitter.instruction("call __rt_hash_set");                                  // insert the entry; rax = updated hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // persist any post-grow hash pointer
}

/// Emits the set bool const x86 stream runtime helper.
fn emit_set_bool_const_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64, value: i64) {
    emitter.instruction(&format!("mov rcx, {}", value));                        // value_lo = boolean payload
    emitter.instruction("xor r8d, r8d");                                        // value_hi unused for booleans
    emitter.instruction("mov r9, 3");                                           // value tag = bool
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Emits the set bool slot x86 stream runtime helper.
fn emit_set_bool_slot_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64, slot: i64) {
    emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {}]", slot));       // value_lo = computed boolean
    emitter.instruction("xor r8d, r8d");                                        // value_hi unused for booleans
    emitter.instruction("mov r9, 3");                                           // value tag = bool
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Emits the set int const x86 stream runtime helper.
fn emit_set_int_const_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64) {
    emitter.instruction("xor ecx, ecx");                                        // value_lo = 0 (elephc keeps no read buffer)
    emitter.instruction("xor r8d, r8d");                                        // value_hi unused for integers
    emitter.instruction("xor r9d, r9d");                                        // value tag = int
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Reads the StreamState wrapper id and loads the matching wrapper-type string
/// literal into rcx (ptr) / r8 (len) / r9 (tag=1), then inserts the hash entry.
/// Fallback id 0 (unset) → "plainfile".
///
/// A socket has NO wrapper: php-src reaches every transport through
/// `php_stream_xport_create`, which never assigns `stream->wrapper`, and
/// `_php_stream_get_metadata` writes the key only `if (stream->wrapper)`. So a socket-pair end, a
/// `stream_socket_server()` and a `stream_socket_client()` all report the key as ABSENT, not as
/// some default. elephc left the id at its unset value 0, which the table below maps to
/// "plainfile", so every socket claimed to have been opened by the plain-files wrapper. Measured
/// on `php -n` 8.5.6.
fn emit_set_wrapper_type_x86(emitter: &mut Emitter) {
    let wrappers: &[(&str, i64)] = &[
        ("_meta_wrapper_plainfile", 9),
        ("_meta_wrapper_http", 4),
        ("_meta_wrapper_https", 5),
        ("_meta_wrapper_ftp", 3),
        ("_meta_wrapper_ftps", 4),
        ("_meta_wrapper_phar", 4),
        ("_meta_wrapper_php", 3),
        ("_meta_wrapper_data", 7),
        ("_meta_wrapper_zlib", 13),
        ("_meta_wrapper_bzip2", 14),
        ("_meta_wrapper_glob", 4),
        ("_meta_wrapper_user", 10),
        ("_meta_wrapper_zip", 11),
    ];
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // reload the stable StreamState pointer
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r10 + {}]", STREAM_TRANSPORT_OFFSET
    ));                                                                         // non-zero only for a socket transport
    emitter.instruction("test rax, rax");
    emitter.instruction("jne __rt_sgmd_wtype_done_x");                          // a transport has no wrapper, so php omits the key
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r10 + {}]", STREAM_WRAPPER_ID_OFFSET
    ));                                                                         // load the handle-keyed wrapper id
    for (id, _) in wrappers.iter().enumerate() {
        let label = format!("__rt_sgmd_wid_{}_x", id);
        emitter.instruction(&format!("cmp eax, {}", id));                       // compare wrapper id
        emitter.instruction(&format!("je {}", label));                          // branch to the matching literal load
    }
    // Fallback for unknown ids: use plainfile (same as id 0).
    abi::emit_symbol_address(emitter, "rcx", "_meta_wrapper_plainfile");          // fallback wrapper name
    emitter.instruction("mov r8, 9");                                           // plainfile length
    emitter.instruction("mov r9, 1");                                           // value tag = string
    emitter.instruction("jmp __rt_sgmd_wtype_put_x");                           // jump to the shared hash insert
    for (id, (sym, len)) in wrappers.iter().enumerate() {
        let label = format!("__rt_sgmd_wid_{}_x", id);
        emitter.label(&label);
        abi::emit_symbol_address(emitter, "rcx", sym);                            // load the wrapper-name literal address
        emitter.instruction(&format!("mov r8, {}", len));                       // wrapper-name length
        emitter.instruction("mov r9, 1");                                       // value tag = string
        emitter.instruction("jmp __rt_sgmd_wtype_put_x");                       // jump to the shared hash insert
    }
    emitter.label("__rt_sgmd_wtype_put_x");
    emit_hash_put_x86(emitter, "_meta_key_wrapper_type", 12);
    emitter.label("__rt_sgmd_wtype_done_x");                                    // a socket rejoins the caller here
}

/// Reads the StreamState URI pointer/length pair and loads it into
/// rcx (ptr) / r8 (len) / r9 (tag=1), then inserts the hash entry.
/// Fallback (ptr == 0) → empty string.
fn emit_set_uri_x86(emitter: &mut Emitter) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // reload the stable StreamState pointer
    emitter.instruction(&format!(
        "mov rcx, QWORD PTR [r10 + {}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // load the handle-keyed URI pointer
    emitter.instruction(&format!(
        "mov r8, QWORD PTR [r10 + {}]", STREAM_URI_LEN_OFFSET
    ));                                                                         // load the handle-keyed URI byte length
    // A pathless stream contributes no key at all — see the AArch64 counterpart for php's
    // `if (stream->orig_path)` guard and the eight-vs-nine key divergence it caused.
    emitter.instruction("test rcx, rcx");                                       // no recorded path?
    emitter.instruction("jz __rt_sgmd_uri_absent_x");                           // → php emits no `uri` key
    emitter.instruction("test r8, r8");                                         // a zero-length path is no path either
    emitter.instruction("jz __rt_sgmd_uri_absent_x");
    // See the AArch64 counterpart: the array releases its string values, so it needs a duplicate
    // rather than the StreamState's own URI allocation.
    emitter.instruction("mov rax, rcx");                                        // duplicate the URI bytes
    emitter.instruction("mov rdx, r8");                                         // with their length
    emitter.instruction("call __rt_str_persist");                               // into storage the array may release
    emitter.instruction("mov rcx, rax");                                        // value_lo = the owned duplicate
    emitter.instruction("mov r8, rdx");                                         // value_hi = its length
    emitter.instruction("mov r9, 1");                                           // value tag = string
    emit_hash_put_x86(emitter, "_meta_key_uri", 3);
    emitter.label("__rt_sgmd_uri_absent_x");                                    // pathless stream: the key is simply not there
}

/// Emits one x86_64 `__rt_hash_set` whose string value is duplicated first.
fn emit_set_owned_str_slots_x86(
    emitter: &mut Emitter,
    key_sym: &str,
    key_len: i64,
    ptr_slot: i64,
    len_slot: i64,
) {
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", ptr_slot));   // duplicate the recorded bytes
    emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", len_slot));   // with their length
    emitter.instruction("call __rt_str_persist");                               // into storage the array may release
    emitter.instruction("mov rcx, rax");                                        // value_lo = the owned duplicate
    emitter.instruction("mov r8, rdx");                                         // value_hi = its length
    emitter.instruction("mov r9, 1");                                           // value tag = string
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Emits the set str slots x86 stream runtime helper.
fn emit_set_str_slots_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64, ptr_slot: i64, len_slot: i64) {
    emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {}]", ptr_slot));   // value_lo = string pointer
    emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {}]", len_slot));    // value_hi = string length
    emitter.instruction("mov r9, 1");                                           // value tag = string
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Emits the x86_64 `__rt_stream_fd_is_regular(fd) -> 1|0`.
///
/// See the AArch64 counterpart. The `struct stat` layout is the x86_64 one — `st_mode` lives at
/// byte 24, not the 16 the AArch64 Linux struct puts it at — so the offsets are the same ones
/// `__rt_fstat_array` uses on this arch rather than `Platform::stat_mode_offset`.
fn emit_stream_fd_is_regular_x86_64(emitter: &mut Emitter) {
    let stat_buf = 144usize;
    let mode_off = 24usize;
    let frame = (stat_buf + 16 + 15) & !15;
    let buf_neg = stat_buf as i64;

    emitter.blank();
    emitter.comment("--- runtime: is the stream descriptor a regular file (S_ISREG) ---");
    emitter.label_global("__rt_stream_fd_is_regular");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction(&format!("sub rsp, {}", frame));                        // reserve the stat buffer
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_neg));              // second libc fstat() argument
    emitter.instruction("call fstat");                                          // fstat(fd, buf)
    emitter.instruction("cmp eax, 0");                                          // did it succeed?
    emitter.instruction("jne __rt_sfir_no_x86");                                // an unstattable descriptor is not a file
    emitter.instruction(&format!(
        "mov eax, DWORD PTR [rbp - {}]",
        buf_neg - mode_off as i64
    ));                                                                         // load st_mode
    emitter.instruction("and eax, 0xF000");                                     // keep only the file-type bits (S_IFMT)
    emitter.instruction("cmp eax, 0x8000");                                     // S_IFREG?
    emitter.instruction("sete al");                                             // that is the answer
    emitter.instruction("movzx eax, al");                                       // widen it to a full word
    emitter.instruction("jmp __rt_sfir_ret_x86");
    emitter.label("__rt_sfir_no_x86");
    emitter.instruction("xor eax, eax");                                        // not a regular file
    emitter.label("__rt_sfir_ret_x86");
    emitter.instruction(&format!("add rsp, {}", frame));                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// Emits the x86_64 `__rt_stream_meta_data_uri_params(hash, uri_ptr, uri_len) -> hash`.
///
/// See the AArch64 counterpart for the parse php-src performs and the key order it produces.
fn emit_data_uri_params_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: metadata keys carried by a data: URI ---");
    emitter.label_global("__rt_stream_meta_data_uri_params");
    // Frame (rbp-relative): [-8]=hash [-16]=seg_start [-24]=comma [-32]=base64 [-40]=head_start
    //                       [-48]=seg_end [-56]=uri_end [-64]=key_ptr [-72]=key_len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the parser frame
    emitter.instruction("sub rsp, 80");                                         // reserve the parser slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the hash being filled
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // base64 = false until a parameter says otherwise

    emitter.instruction("mov r10, rsi");
    emitter.instruction("add r10, rdx");                                        // one past the last URI byte
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");
    emitter.instruction("cmp rdx, 5");                                          // shorter than "data:"?
    emitter.instruction("jb __rt_sgmdp_base_x86");                              // nothing to parse; still emit base64
    emitter.instruction("mov r11, rsi");
    emitter.instruction("add r11, 5");                                          // skip the "data:" scheme
    emitter.instruction("mov rax, r10");
    emitter.instruction("sub rax, r11");                                        // bytes left after the scheme
    emitter.instruction("cmp rax, 2");
    emitter.instruction("jb __rt_sgmdp_noslash_x86");
    emitter.instruction("cmp BYTE PTR [r11], 0x2F");                            // '/'
    emitter.instruction("jne __rt_sgmdp_noslash_x86");
    emitter.instruction("cmp BYTE PTR [r11 + 1], 0x2F");                        // '/'
    emitter.instruction("jne __rt_sgmdp_noslash_x86");
    emitter.instruction("add r11, 2");                                          // skip the optional "//"
    emitter.label("__rt_sgmdp_noslash_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // head_start, which is also the media type
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // the first segment starts there

    emitter.instruction("mov rax, r11");                                        // scan for the comma that ends the head
    emitter.label("__rt_sgmdp_comma_x86");
    emitter.instruction("cmp rax, r10");
    emitter.instruction("jae __rt_sgmdp_comma_done_x86");                       // no comma: the head is the whole URI
    emitter.instruction("cmp BYTE PTR [rax], 0x2C");                            // ','
    emitter.instruction("je __rt_sgmdp_comma_done_x86");
    emitter.instruction("add rax, 1");
    emitter.instruction("jmp __rt_sgmdp_comma_x86");
    emitter.label("__rt_sgmdp_comma_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the head ends here

    emitter.label("__rt_sgmdp_seg_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // seg_start
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // comma
    emitter.instruction("cmp r9, r10");
    emitter.instruction("ja __rt_sgmdp_base_x86");                              // walked past the head: done
    emitter.instruction("mov r11, r9");                                         // scan for this segment's ';'
    emitter.label("__rt_sgmdp_semi_x86");
    emitter.instruction("cmp r11, r10");
    emitter.instruction("jae __rt_sgmdp_semi_done_x86");
    emitter.instruction("cmp BYTE PTR [r11], 0x3B");                            // ';'
    emitter.instruction("je __rt_sgmdp_semi_done_x86");
    emitter.instruction("add r11, 1");
    emitter.instruction("jmp __rt_sgmdp_semi_x86");
    emitter.label("__rt_sgmdp_semi_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // seg_end
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // head_start
    emitter.instruction("cmp r9, rax");
    emitter.instruction("je __rt_sgmdp_media_x86");                             // the first segment is the media type

    // -- a `;`-separated parameter: either the bare word base64, or name=value --
    emitter.instruction("mov rax, r11");
    emitter.instruction("sub rax, r9");                                         // segment length
    emitter.instruction("cmp rax, 6");
    emitter.instruction("jne __rt_sgmdp_kv_x86");
    for (offset, byte) in b"base64".iter().enumerate() {
        emitter.instruction(&format!("cmp BYTE PTR [r9 + {}], {}", offset, byte)); // one candidate byte of "base64"
        emitter.instruction("jne __rt_sgmdp_kv_x86");                           // a different word is a name=value parameter
    }
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // base64 = true
    emitter.instruction("jmp __rt_sgmdp_next_x86");

    emitter.label("__rt_sgmdp_kv_x86");
    emitter.instruction("mov rax, r9");                                         // scan for the '=' that splits name from value
    emitter.label("__rt_sgmdp_eq_x86");
    emitter.instruction("cmp rax, r11");
    emitter.instruction("jae __rt_sgmdp_next_x86");                             // no '=': php refuses such a URI outright
    emitter.instruction("cmp BYTE PTR [rax], 0x3D");                            // '='
    emitter.instruction("je __rt_sgmdp_eq_done_x86");
    emitter.instruction("add rax, 1");
    emitter.instruction("jmp __rt_sgmdp_eq_x86");
    emitter.label("__rt_sgmdp_eq_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // the name starts at the segment
    emitter.instruction("mov rcx, rax");
    emitter.instruction("sub rcx, r9");
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // and runs up to the '='
    emitter.instruction("add rax, 1");                                          // the value starts after the '='
    emitter.instruction("mov rdx, r11");
    emitter.instruction("sub rdx, rax");                                        // and runs to the end of the segment
    emitter.instruction("call __rt_str_persist");                               // the array releases its values, so it needs its own
    emitter.instruction("mov rcx, rax");                                        // value_lo
    emitter.instruction("mov r8, rdx");                                         // value_hi
    emitter.instruction("mov r9, 1");                                           // value tag = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // the parameter name as the key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");
    emitter.instruction("call __rt_hash_set");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // persist any post-grow hash pointer
    emitter.instruction("jmp __rt_sgmdp_next_x86");

    emitter.label("__rt_sgmdp_media_x86");
    emitter.instruction("cmp r9, r11");
    emitter.instruction("je __rt_sgmdp_next_x86");                              // `data:,x` spells no media type, so php emits no key
    emitter.instruction("mov rax, r9");
    emitter.instruction("mov rdx, r11");
    emitter.instruction("sub rdx, r9");
    emitter.instruction("call __rt_str_persist");                               // give the array a copy it may release
    emitter.instruction("mov rcx, rax");                                        // value_lo
    emitter.instruction("mov r8, rdx");                                         // value_hi
    emitter.instruction("mov r9, 1");                                           // value tag = string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the hash
    abi::emit_symbol_address(emitter, "rsi", "_meta_key_mediatype");
    emitter.instruction("mov rdx, 9");                                          // length of "mediatype"
    emitter.instruction("call __rt_hash_set");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // persist any post-grow hash pointer

    emitter.label("__rt_sgmdp_next_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // seg_end
    emitter.instruction("add r11, 1");                                          // step over its ';'
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");
    emitter.instruction("jmp __rt_sgmdp_seg_x86");

    emitter.label("__rt_sgmdp_base_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value_lo = the base64 verdict
    emitter.instruction("xor r8d, r8d");                                        // value_hi unused for booleans
    emitter.instruction("mov r9, 3");                                           // value tag = bool
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the hash
    abi::emit_symbol_address(emitter, "rsi", "_meta_key_base64");
    emitter.instruction("mov rdx, 6");                                          // length of "base64"
    emitter.instruction("call __rt_hash_set");                                  // php emits this key even when it is false
    emitter.instruction("mov rsp, rbp");                                        // release the parser frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // rax = the updated hash
}

/// Emits the x86_64 `__rt_stream_meta_has_api_flags(handle) -> 1|0`.
///
/// See the AArch64 counterpart for why `php://temp` and `data:` answer 0.
fn emit_stream_meta_has_api_flags_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: does this stream answer PHP_STREAM_OPTION_META_DATA_API ---");
    emitter.label_global("__rt_stream_meta_has_api_flags");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_smaf_yes_x86");                                // no state: keep the flags
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_WRAPPER_ID_OFFSET}]"
    ));                                                                         // which wrapper opened it
    emitter.instruction(&format!("cmp r10, {WRAPPER_ID_DATA}"));
    emitter.instruction("je __rt_smaf_no_x86");                                 // data: answers nothing
    emitter.instruction(&format!("cmp r10, {WRAPPER_ID_PHP}"));
    emitter.instruction("jne __rt_smaf_yes_x86");                               // every other wrapper answers
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_URI_PTR_OFFSET}]"
    ));                                                                         // the recorded URI
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_URI_LEN_OFFSET}]"
    ));                                                                         // and its length
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_smaf_yes_x86");                                // no URI: keep the flags
    emitter.instruction("cmp r11, 7");                                          // "php://" plus the naming byte
    emitter.instruction("jl __rt_smaf_yes_x86");
    emitter.instruction("movzx eax, BYTE PTR [r10 + 6]");                       // the first byte of the php:// name
    emitter.instruction("cmp al, 0x74");                                        // 't' as in temp
    emitter.instruction("je __rt_smaf_no_x86");
    emitter.label("__rt_smaf_yes_x86");
    emitter.instruction("mov rax, 1");                                          // the flags belong in the array
    emitter.instruction("jmp __rt_smaf_ret_x86");
    emitter.label("__rt_smaf_no_x86");
    emitter.instruction("xor eax, eax");                                        // php omits all three here
    emitter.label("__rt_smaf_ret_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
