//! Purpose:
//! Emits `__rt_stream_type_name`, which names a stream the way `stream_get_meta_data()` does.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_stream_get_meta_data`, after the descriptor-derived fallback has been settled.
//!
//! Key details:
//! - `stream_type` is a wrapper and backend identity in php-src, not a descriptor property.
//!   Deriving it from seekability — which is what the metadata helper does without this — called
//!   `php://memory` STDIO, `php://output` a socket, and a `popen()` pipe a socket too, because the
//!   only question it asked was whether `lseek` worked.
//! - The php:// sub-wrapper is told apart by the byte after `php://`, the same one the recorded
//!   mode uses: memory, temp, output and input each have their own name and everything else —
//!   `stdin`, `stdout`, `stderr`, `fd`, `filter` — is STDIO.
//! - A stream with no recorded identity answers nothing, and the caller keeps its fallback. That is
//!   what leaves sockets on the descriptor-derived name until they record a transport of their own.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_BACKEND_DIRECTORY, STREAM_BACKEND_GLOB_DIRECTORY, STREAM_BACKEND_KIND_OFFSET,
    STREAM_BACKEND_POPEN, STREAM_BACKEND_USER_DIRECTORY, STREAM_TRANSPORT_GENERIC,
    STREAM_TRANSPORT_OFFSET, STREAM_TRANSPORT_TCP, STREAM_TRANSPORT_UDP, STREAM_TRANSPORT_UNIX,
    STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET, STREAM_WRAPPER_ID_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Wrapper id 6 is `php://`, whose sub-wrappers carry most of the distinct names.
const WRAPPER_ID_PHP: u64 = 6;

/// Wrapper id 7 is `data:`, which php-src names after the RFC that defines it.
const WRAPPER_ID_DATA: u64 = 7;

/// Wrapper id 12 is `zip://`, php's twelfth and last built-in wrapper.
///
/// Measured on `php -n` 8.5.6: `stream_get_meta_data(fopen("zip://a.zip#f.txt","r"))`
/// reports `wrapper_type` `"zip wrapper"` and `stream_type` `"zip"`. The id is the
/// entry's position in the built-in wrapper-name table, so it is fixed by
/// `STREAM_WRAPPERS` and by the `_meta_wrapper_*` list this file's sibling walks.
pub(crate) const WRAPPER_ID_ZIP: u64 = 12;

/// Emits `__rt_stream_type_name(handle) -> ptr, len`, or `0` when the stream has no recorded name.
pub fn emit_stream_type_name(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_stream_type_name_aarch64(emitter),
        Arch::X86_64 => emit_stream_type_name_x86_64(emitter),
    }
    emit_stream_seek_unsupported(emitter);
}

/// Emits `__rt_stream_seek_unsupported(handle) -> 1` when the WRAPPER has no seek op.
///
/// php-src builds a stream from a set of ops, and a wrapper that leaves `seek` NULL makes
/// `php_stream_seek` refuse before any descriptor is touched. `ext/zip`'s entry ops are such a
/// set, so on `php -n` 8.5.6:
///
/// ```text
/// $h = fopen("zip://a.zip#f.txt", "r"); fread($h, 3);
/// rewind($h);          => Warning: rewind(): Stream does not support seeking   + bool(false)
/// ftell($h);           => int(3)      (the refused seek moved nothing)
/// fseek($h, 0, SEEK_SET) => Warning: fseek(): Stream does not support seeking  + int(-1)
/// ```
///
/// elephc serves a zip entry from a regular temp file, which seeks perfectly well, so the
/// refusal has to come from the recorded wrapper identity rather than from the backend.
fn emit_stream_seek_unsupported(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: does this stream's WRAPPER refuse to seek at all ---");
    emitter.label_global("__rt_stream_seek_unsupported");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // frame for the saved linkage
            emitter.instruction("stp x29, x30, [sp, #0]");                      // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the helper frame pointer
            emitter.instruction("bl __rt_stream_state");                        // resolve the owning stream state
            emitter.instruction("cbz x0, __rt_ssu_no");                         // a stale handle refuses nothing here
            emitter.instruction(&format!(
                "ldr x9, [x0, #{STREAM_WRAPPER_ID_OFFSET}]"
            ));                                                                 // which wrapper opened it
            emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_ZIP}"));         // the zip wrapper, whose ops have no seek?
            emitter.instruction("cset x0, eq");                                 // 1 only for a wrapper that cannot seek
            emitter.instruction("b __rt_ssu_ret");                              // return the verdict
            emitter.label("__rt_ssu_no");
            emitter.instruction("mov x0, #0");                                  // every other stream seeks normally
            emitter.label("__rt_ssu_ret");
            emitter.instruction("ldp x29, x30, [sp, #0]");                      // restore frame pointer and return address
            emitter.instruction("add sp, sp, #16");                             // release the helper frame
            emitter.instruction("ret");                                         // report whether seeking is refused
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame pointer
            emitter.instruction("call __rt_stream_state");                      // resolve the owning stream state
            emitter.instruction("test rax, rax");                               // a stale handle refuses nothing here
            emitter.instruction("jz __rt_ssu_no_x86");                          // branch when the checked value is zero or equal
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rax + {STREAM_WRAPPER_ID_OFFSET}]"
            ));                                                                 // which wrapper opened it
            emitter.instruction(&format!("cmp r10, {WRAPPER_ID_ZIP}"));         // the zip wrapper, whose ops have no seek?
            emitter.instruction("sete al");                                     // 1 only for a wrapper that cannot seek
            emitter.instruction("movzx rax, al");                               // widen the verdict to a full word
            emitter.instruction("jmp __rt_ssu_ret_x86");                        // return the verdict
            emitter.label("__rt_ssu_no_x86");
            emitter.instruction("xor eax, eax");                                // every other stream seeks normally
            emitter.label("__rt_ssu_ret_x86");
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // report whether seeking is refused
        }
    }
}

/// Emits the AArch64 namer.
fn emit_stream_type_name_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: name a stream the way stream_get_meta_data does ---");
    emitter.label_global("__rt_stream_type_name");
    emitter.instruction("sub sp, sp, #16");                                     // frame for the saved linkage
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_stn_unknown");                            // a stale handle has no name to give

    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_BACKEND_KIND_OFFSET}]")); // what backs the stream
    emitter.instruction(&format!("cmp x9, #{STREAM_BACKEND_DIRECTORY}"));
    emitter.instruction("b.eq __rt_stn_dir");
    emitter.instruction(&format!("cmp x9, #{STREAM_BACKEND_USER_DIRECTORY}"));
    emitter.instruction("b.eq __rt_stn_dir");                                   // a user directory reads as a directory
    emitter.instruction(&format!("cmp x9, #{STREAM_BACKEND_GLOB_DIRECTORY}"));
    emitter.instruction("b.eq __rt_stn_glob");
    emitter.instruction(&format!("cmp x9, #{STREAM_BACKEND_POPEN}"));
    emitter.instruction("b.eq __rt_stn_stdio");                                 // php-src calls a process pipe STDIO

    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_TRANSPORT_OFFSET}]"));  // which transport, if this is a socket
    emitter.instruction(&format!("cmp x9, #{STREAM_TRANSPORT_TCP}"));
    emitter.instruction("b.eq __rt_stn_tcp");
    emitter.instruction(&format!("cmp x9, #{STREAM_TRANSPORT_UDP}"));
    emitter.instruction("b.eq __rt_stn_udp");
    emitter.instruction(&format!("cmp x9, #{STREAM_TRANSPORT_UNIX}"));
    emitter.instruction("b.eq __rt_stn_unix");
    emitter.instruction(&format!("cmp x9, #{STREAM_TRANSPORT_GENERIC}"));
    emitter.instruction("b.eq __rt_stn_generic");

    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_DATA}"));
    emitter.instruction("b.eq __rt_stn_rfc2397");
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_ZIP}"));
    emitter.instruction("b.eq __rt_stn_zip");                                   // php names a zip entry stream "zip"
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_PHP}"));
    emitter.instruction("b.ne __rt_stn_unknown");                               // every other wrapper keeps the fallback

    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_URI_PTR_OFFSET}]"));   // the recorded URI
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_URI_LEN_OFFSET}]"));   // and its length
    emitter.instruction("cbz x10, __rt_stn_stdio");                             // a php:// stream with no URI reads as STDIO
    emitter.instruction("cmp x11, #7");                                         // "php://" plus the byte that names the wrapper
    emitter.instruction("b.lt __rt_stn_stdio");
    emitter.instruction("ldrb w12, [x10, #6]");                                 // the first byte of the php:// wrapper name
    emitter.instruction("cmp w12, #0x6D");                                      // 'm' as in memory
    emitter.instruction("b.eq __rt_stn_memory");
    emitter.instruction("cmp w12, #0x74");                                      // 't' as in temp
    emitter.instruction("b.eq __rt_stn_temp");
    emitter.instruction("cmp w12, #0x6F");                                      // 'o' as in output
    emitter.instruction("b.eq __rt_stn_output");
    emitter.instruction("cmp w12, #0x69");                                      // 'i' as in input
    emitter.instruction("b.eq __rt_stn_input");
    emitter.instruction("b __rt_stn_stdio");                                    // stdin, stdout, stderr, fd and filter

    emit_named_type_aarch64(emitter, "__rt_stn_memory", "_meta_stype_memory", 6);
    emit_named_type_aarch64(emitter, "__rt_stn_temp", "_meta_stype_temp", 4);
    emit_named_type_aarch64(emitter, "__rt_stn_output", "_meta_stype_output", 6);
    emit_named_type_aarch64(emitter, "__rt_stn_input", "_meta_stype_input", 5);
    emit_named_type_aarch64(emitter, "__rt_stn_dir", "_meta_stype_dir", 3);
    emit_named_type_aarch64(emitter, "__rt_stn_glob", "_meta_stype_glob", 4);
    emit_named_type_aarch64(emitter, "__rt_stn_rfc2397", "_meta_wrapper_data", 7);
    emit_named_type_aarch64(emitter, "__rt_stn_zip", "_meta_stype_zip", 3);
    emit_named_type_aarch64(emitter, "__rt_stn_stdio", "_meta_stype_stdio", 5);
    emit_named_type_aarch64(emitter, "__rt_stn_tcp", "_meta_stype_tcp", 14);
    emit_named_type_aarch64(emitter, "__rt_stn_udp", "_meta_stype_udp", 10);
    emit_named_type_aarch64(emitter, "__rt_stn_unix", "_meta_stype_unix", 11);
    emit_named_type_aarch64(emitter, "__rt_stn_generic", "_meta_stype_generic", 14);

    emitter.label("__rt_stn_unknown");
    emitter.instruction("mov x0, #0");                                          // no recorded name: the caller keeps its fallback
    emitter.instruction("mov x1, #0");
    emitter.label("__rt_stn_return");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");
}

/// Emits one AArch64 arm that answers a fixed stream-type name.
fn emit_named_type_aarch64(emitter: &mut Emitter, label: &str, symbol: &str, len: i64) {
    emitter.label(label);
    abi::emit_symbol_address(emitter, "x0", symbol);
    emitter.instruction(&format!("mov x1, #{}", len));                          // the name and its byte length
    emitter.instruction("b __rt_stn_return");
}

/// Emits the x86_64 namer.
fn emit_stream_type_name_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: name a stream the way stream_get_meta_data does ---");
    emitter.label_global("__rt_stream_type_name");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_stn_unknown_x86");                             // a stale handle has no name to give

    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_BACKEND_KIND_OFFSET}]"
    ));                                                                         // what backs the stream
    emitter.instruction(&format!("cmp r10, {STREAM_BACKEND_DIRECTORY}"));
    emitter.instruction("je __rt_stn_dir_x86");
    emitter.instruction(&format!("cmp r10, {STREAM_BACKEND_USER_DIRECTORY}"));
    emitter.instruction("je __rt_stn_dir_x86");                                 // a user directory reads as a directory
    emitter.instruction(&format!("cmp r10, {STREAM_BACKEND_GLOB_DIRECTORY}"));
    emitter.instruction("je __rt_stn_glob_x86");
    emitter.instruction(&format!("cmp r10, {STREAM_BACKEND_POPEN}"));
    emitter.instruction("je __rt_stn_stdio_x86");                               // php-src calls a process pipe STDIO

    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_TRANSPORT_OFFSET}]"
    ));                                                                         // which transport, if this is a socket
    emitter.instruction(&format!("cmp r10, {STREAM_TRANSPORT_TCP}"));
    emitter.instruction("je __rt_stn_tcp_x86");
    emitter.instruction(&format!("cmp r10, {STREAM_TRANSPORT_UDP}"));
    emitter.instruction("je __rt_stn_udp_x86");
    emitter.instruction(&format!("cmp r10, {STREAM_TRANSPORT_UNIX}"));
    emitter.instruction("je __rt_stn_unix_x86");
    emitter.instruction(&format!("cmp r10, {STREAM_TRANSPORT_GENERIC}"));
    emitter.instruction("je __rt_stn_generic_x86");

    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_WRAPPER_ID_OFFSET}]"
    ));                                                                         // which wrapper opened it
    emitter.instruction(&format!("cmp r10, {WRAPPER_ID_DATA}"));
    emitter.instruction("je __rt_stn_rfc2397_x86");
    emitter.instruction(&format!("cmp r10, {WRAPPER_ID_ZIP}"));
    emitter.instruction("je __rt_stn_zip_x86");                                 // php names a zip entry stream "zip"
    emitter.instruction(&format!("cmp r10, {WRAPPER_ID_PHP}"));
    emitter.instruction("jne __rt_stn_unknown_x86");                            // every other wrapper keeps the fallback

    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_URI_PTR_OFFSET}]"
    ));                                                                         // the recorded URI
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_URI_LEN_OFFSET}]"
    ));                                                                         // and its length
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_stn_stdio_x86");                               // a php:// stream with no URI reads as STDIO
    emitter.instruction("cmp r11, 7");                                          // "php://" plus the byte that names the wrapper
    emitter.instruction("jl __rt_stn_stdio_x86");
    emitter.instruction("movzx eax, BYTE PTR [r10 + 6]");                       // the first byte of the php:// wrapper name
    emitter.instruction("cmp al, 0x6D");                                        // 'm' as in memory
    emitter.instruction("je __rt_stn_memory_x86");
    emitter.instruction("cmp al, 0x74");                                        // 't' as in temp
    emitter.instruction("je __rt_stn_temp_x86");
    emitter.instruction("cmp al, 0x6F");                                        // 'o' as in output
    emitter.instruction("je __rt_stn_output_x86");
    emitter.instruction("cmp al, 0x69");                                        // 'i' as in input
    emitter.instruction("je __rt_stn_input_x86");
    emitter.instruction("jmp __rt_stn_stdio_x86");                              // stdin, stdout, stderr, fd and filter

    emit_named_type_x86_64(emitter, "__rt_stn_zip_x86", "_meta_stype_zip", 3);
    emit_named_type_x86_64(emitter, "__rt_stn_memory_x86", "_meta_stype_memory", 6);
    emit_named_type_x86_64(emitter, "__rt_stn_temp_x86", "_meta_stype_temp", 4);
    emit_named_type_x86_64(emitter, "__rt_stn_output_x86", "_meta_stype_output", 6);
    emit_named_type_x86_64(emitter, "__rt_stn_input_x86", "_meta_stype_input", 5);
    emit_named_type_x86_64(emitter, "__rt_stn_dir_x86", "_meta_stype_dir", 3);
    emit_named_type_x86_64(emitter, "__rt_stn_glob_x86", "_meta_stype_glob", 4);
    emit_named_type_x86_64(emitter, "__rt_stn_rfc2397_x86", "_meta_wrapper_data", 7);
    emit_named_type_x86_64(emitter, "__rt_stn_stdio_x86", "_meta_stype_stdio", 5);
    emit_named_type_x86_64(emitter, "__rt_stn_tcp_x86", "_meta_stype_tcp", 14);
    emit_named_type_x86_64(emitter, "__rt_stn_udp_x86", "_meta_stype_udp", 10);
    emit_named_type_x86_64(emitter, "__rt_stn_unix_x86", "_meta_stype_unix", 11);
    emit_named_type_x86_64(emitter, "__rt_stn_generic_x86", "_meta_stype_generic", 14);

    emitter.label("__rt_stn_unknown_x86");
    emitter.instruction("xor eax, eax");                                        // no recorded name: the caller keeps its fallback
    emitter.instruction("xor edx, edx");
    emitter.label("__rt_stn_return_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// Emits one x86_64 arm that answers a fixed stream-type name.
fn emit_named_type_x86_64(emitter: &mut Emitter, label: &str, symbol: &str, len: i64) {
    emitter.label(label);
    abi::emit_symbol_address(emitter, "rax", symbol);
    emitter.instruction(&format!("mov rdx, {}", len));                          // the name and its byte length
    emitter.instruction("jmp __rt_stn_return_x86");
}
