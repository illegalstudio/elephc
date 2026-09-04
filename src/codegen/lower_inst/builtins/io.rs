//! Purpose:
//! Lowers filesystem metadata builtins for the EIR backend.
//! Reuses the shared runtime stat helpers instead of duplicating platform logic.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - Path operands are already evaluated by EIR and are materialized into the
//!   string result registers expected by the shared runtime helpers.

use crate::codegen::{abi, callable_descriptor, emit_box_current_value_as_mixed, NULL_SENTINEL};
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::codegen_support::runtime::resources::layout::{
    CONTEXT_NOTIFIER_OFFSET, CONTEXT_OPTIONS_OFFSET, STREAM_BACKEND_AUX_OFFSET,
    STREAM_READ_FILTER_HEAD_OFFSET, STREAM_WRITE_FILTER_HEAD_OFFSET,
    STREAM_BACKEND_KIND_OFFSET, STREAM_BACKEND_POPEN, STREAM_OWNERSHIP_FLAGS_OFFSET,
    STREAM_STATE_FLAG_IS_URL,
};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};
use super::super::{resolve_int_operand_to_result, resolve_nullable_int_operand_to_result};

const STREAM_METADATA_SLOT: usize = 14;
const STREAM_WRAPPER_UNLINK_SLOT: usize = 15;
const STREAM_WRAPPER_MKDIR_SLOT: usize = 17;
const STREAM_WRAPPER_RMDIR_SLOT: usize = 18;
const STREAM_META_TOUCH: usize = 1;
const STREAM_META_OWNER_NAME: usize = 2;
const STREAM_META_OWNER: usize = 3;
const STREAM_META_GROUP_NAME: usize = 4;
const STREAM_META_GROUP: usize = 5;
const STREAM_META_ACCESS: usize = 6;
const STREAM_OPTION_BLOCKING: usize = 1;
const STREAM_OPTION_READ_BUFFER: usize = 2;
const STREAM_OPTION_WRITE_BUFFER: usize = 3;
const STREAM_OPTION_READ_TIMEOUT: usize = 4;
/// `PHP_STREAM_BUFFER_NONE`, the mode php sends for a requested size of zero.
const STREAM_BUFFER_NONE: usize = 0;
/// `PHP_STREAM_BUFFER_FULL`, the mode php sends for any non-zero size.
const STREAM_BUFFER_FULL: usize = 2;
/// The chunk size php passes as `$arg2` when the requested buffer size is zero.
const DEFAULT_CHUNK_SIZE: usize = 1024;
use crate::codegen_support::runtime::data::{
    STAT_FAILED_TAIL, LSTAT_FAILED_TAIL,
    WRAPPER_MISSING_HOOK_HEAD_CHGRP, WRAPPER_MISSING_HOOK_HEAD_CHMOD,
    WRAPPER_MISSING_HOOK_HEAD_CHOWN, WRAPPER_MISSING_HOOK_HEAD_MKDIR,
    WRAPPER_MISSING_HOOK_HEAD_RENAME, WRAPPER_MISSING_HOOK_HEAD_RMDIR,
    WRAPPER_MISSING_HOOK_HEAD_TOUCH, WRAPPER_MISSING_HOOK_HEAD_UNLINK,
    WRAPPER_MISSING_HOOK_HEAD_FILESIZE, WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS,
    WRAPPER_MISSING_HOOK_HEAD_IS_FILE, WRAPPER_MISSING_HOOK_HEAD_IS_DIR,
    WRAPPER_MISSING_HOOK_HEAD_IS_LINK, WRAPPER_MISSING_HOOK_HEAD_IS_READABLE,
    WRAPPER_MISSING_HOOK_HEAD_IS_WRITABLE, WRAPPER_MISSING_HOOK_HEAD_IS_WRITEABLE,
    WRAPPER_MISSING_HOOK_HEAD_IS_EXECUTABLE, WRAPPER_MISSING_HOOK_HEAD_FILEMTIME,
    WRAPPER_MISSING_HOOK_HEAD_FILEATIME, WRAPPER_MISSING_HOOK_HEAD_FILECTIME,
    WRAPPER_MISSING_HOOK_HEAD_FILETYPE, WRAPPER_MISSING_HOOK_HEAD_FILEPERMS,
    WRAPPER_MISSING_HOOK_HEAD_FILEOWNER, WRAPPER_MISSING_HOOK_HEAD_FILEGROUP,
    WRAPPER_MISSING_HOOK_HEAD_FILEINODE, WRAPPER_MISSING_HOOK_HEAD_STAT,
    WRAPPER_MISSING_HOOK_HEAD_LSTAT,
    WRAPPER_MISSING_HOOK_TAIL_METADATA, WRAPPER_MISSING_HOOK_TAIL_MKDIR,
    WRAPPER_MISSING_HOOK_TAIL_RENAME, WRAPPER_MISSING_HOOK_TAIL_RMDIR,
    WRAPPER_MISSING_HOOK_TAIL_UNLINK, WRAPPER_MISSING_HOOK_TAIL_URL_STAT,
};

/// `stream_set_write_buffer()`'s answer for a stream that is not a userspace wrapper.
const NATIVE_WRITE_BUFFER_RESULT: i64 = -1;
/// `stream_set_read_buffer()`'s answer for a stream that is not a userspace wrapper.
const NATIVE_READ_BUFFER_RESULT: i64 = 0;
/// `STREAM_REPORT_ERRORS`, the `$options` bit php sets on every wrapper path operation.
const STREAM_REPORT_ERRORS: usize = 8;
/// `mkdir()`'s documented default permissions, `0777`.
const MKDIR_DEFAULT_MODE: usize = 0o777;
const TOUCH_ATIME_NOW: u8 = 1;
const TOUCH_MTIME_NOW: u8 = 2;
const TOUCH_BOTH_NOW: u8 = TOUCH_ATIME_NOW | TOUCH_MTIME_NOW;

mod phar_read;
mod fopen_core;
mod fopen_data;
mod fopen_network;
mod fopen_phar;
mod stream_context;
mod stream_copy_queries;
mod stream_filters;
mod stream_buckets;
mod stream_options;
mod stream_sockets;
mod stream_file_ops;
mod stream_file_dispatch;
mod host_directory_process;
mod phar_write;
mod phar_metadata;
mod filesystem_ops;
mod filesystem_principals;
mod touch_pathinfo;
mod stat_ops;
mod stat_wrapper_dispatch;
mod wrapper_dispatch;
pub(in crate::codegen::lower_inst::builtins) mod path_call_helpers;
pub(in crate::codegen::lower_inst::builtins) mod stream_dispatch_helpers;
mod filter_helpers;
mod stream_bucket_arch;
mod close_crypto_arch;
mod stream_read_helpers;
mod context_result_helpers;
mod resource_handles;
mod seek_hash_arch;
mod boxing_helpers;
mod string_validation;
#[cfg(test)]
mod resource_tests;

use phar_read::*;
use fopen_data::*;
use fopen_network::*;
use fopen_phar::*;
use stream_filters::*;
use stream_file_dispatch::*;
use phar_write::*;
use filesystem_principals::*;
use touch_pathinfo::*;
use wrapper_dispatch::*;
use stat_wrapper_dispatch::*;
use path_call_helpers::*;
use stream_dispatch_helpers::*;
use filter_helpers::*;
use stream_bucket_arch::*;
use close_crypto_arch::*;
use stream_read_helpers::*;
use context_result_helpers::*;
use stream_context::*;
use fopen_core::{begin_fopen_context_scope, emit_literal_php_filter_fopen_result,
    emit_request_default_stream_context_handle, emit_static_diag_warning,
    finish_fopen_context_scope, LiteralOpenMode, DYNAMIC_COMPRESS_WRAPPERS};
use resource_handles::*;
use seek_hash_arch::*;
use boxing_helpers::*;
use string_validation::*;

pub(crate) use phar_read::{
    lower_file_get_contents, lower_hash_file, lower_readfile, lower_readline,
};
pub(crate) use fopen_core::{
    lower_fopen,
};
pub(crate) use stream_context::{
    lower_stream_wrapper_register, lower_stream_wrapper_unregister, lower_stream_wrapper_restore, lower_stream_context_create,
    lower_stream_context_get_default, lower_stream_context_set_default, lower_stream_context_set_option, StreamContextSetOptionSpelling, lower_stream_context_set_params,
    lower_stream_context_get_options, lower_stream_context_get_params, lower_stream_get_contents,
};
pub(crate) use stream_copy_queries::{
    lower_stream_copy_to_stream, lower_stream_get_line, lower_stream_get_meta_data, lower_stream_get_wrappers,
    lower_stream_get_transports, lower_stream_get_filters,
    lower_http_get_last_response_headers, lower_http_clear_last_response_headers,
};
pub(crate) use stream_filters::{
    lower_stream_filter_register, lower_stream_filter_attach, lower_stream_filter_remove,
};
pub(crate) use stream_buckets::{
    lower_stream_bucket_new, lower_stream_bucket_make_writeable, lower_stream_bucket_append, lower_stream_bucket_prepend,
    lower_stream_is_local, lower_stream_supports_lock,
};
pub(crate) use stream_options::{
    lower_stream_isatty, lower_stream_set_blocking, lower_stream_set_chunk_size,
    lower_stream_set_read_buffer, lower_stream_set_write_buffer,
    lower_stream_set_timeout, lower_stream_select, lower_stream_resolve_include_path,
};
pub(crate) use stream_sockets::{
    lower_stream_socket_server, lower_stream_socket_client, lower_stream_socket_accept, lower_stream_socket_pair,
    lower_stream_socket_get_name, lower_stream_socket_shutdown, lower_stream_socket_enable_crypto, lower_stream_socket_recvfrom,
    lower_stream_socket_sendto,
};
pub(crate) use stream_file_ops::{
    lower_fclose, lower_fread, lower_fwrite, lower_fprintf,
    lower_vfprintf, lower_fgets, lower_fgetc,
    lower_fgetcsv, lower_fputcsv, lower_str_getcsv, lower_fpassthru,
};
pub(crate) use stream_file_dispatch::{
    lower_feof, lower_ftell, lower_fseek, lower_rewind,
    lower_ftruncate, lower_fsync, lower_fflush, lower_fdatasync,
    lower_flock,
};
pub(crate) use host_directory_process::{
    lower_disk_free_space, lower_disk_total_space, lower_gethostname, lower_gethostbyname,
    lower_gethostbyaddr, lower_getprotobyname, lower_getprotobynumber, lower_getservbyname,
    lower_getservbyport, lower_opendir, lower_readdir, lower_closedir,
    lower_rewinddir, lower_popen, lower_pclose, lower_fsockopen,
    lower_file, lower_realpath, lower_realpath_cache_get, lower_realpath_cache_size,
};
pub(crate) use phar_write::{
    lower_file_put_contents, lower_elephc_phar_set_compression, lower_elephc_phar_get_metadata, lower_elephc_phar_get_stub,
    lower_elephc_phar_set_metadata, lower_elephc_phar_set_stub,
};
pub(crate) use phar_metadata::{
    lower_elephc_phar_get_file_metadata, lower_elephc_phar_set_file_metadata, lower_elephc_phar_gzip_archive, lower_elephc_phar_bzip2_archive,
    lower_elephc_phar_decompress_archive, lower_elephc_phar_sign_openssl, lower_elephc_phar_sign_hash, lower_elephc_phar_set_zip_password,
    lower_elephc_phar_get_signature_hash, lower_elephc_phar_get_signature_type, lower_elephc_phar_list_entries,
    lower_elephc_zip_stat_entries,
};
pub(crate) use filesystem_ops::{
    lower_file_exists, lower_unlink, lower_mkdir, lower_rmdir,
    lower_chdir, lower_copy, lower_rename, lower_tempnam,
    lower_scandir, lower_glob, lower_chmod, lower_chown,
    lower_chgrp, lower_lchown, lower_lchgrp, lower_umask,
    lower_touch, lower_basename, lower_dirname, lower_fnmatch,
    lower_pathinfo,
};
pub(crate) use stat_ops::{
    lower_getcwd, lower_sys_get_temp_dir, lower_tmpfile, lower_filesize,
    lower_filemtime, lower_linkinfo, lower_symlink, lower_link,
    lower_readlink, lower_fileatime, lower_filectime, lower_fileperms,
    lower_fileowner, lower_filegroup, lower_fileinode, lower_filetype,
    lower_stat, lower_lstat, lower_fstat, lower_clearstatcache,
    lower_is_file, lower_is_dir, lower_is_readable, lower_is_writable,
    lower_is_writeable, lower_is_executable, lower_is_link,
};
pub(super) use boxing_helpers::box_owned_string_or_false_result;
pub(super) use resource_handles::{
    load_resource_payload_to_result, load_stream_fd_to_result, load_stream_handle_to_result,
};
pub(super) use string_validation::load_string_to_result;

/// Emits a literal `file_get_contents("phar://...")` payload through compile-time PHAR extraction.
///
/// The extracted bytes live in read-only `.data`, so a following `$offset`/`$length` window — which
/// trims its input in place and frees a failed read — would move and free a rodata pointer.
/// `persist` therefore copies the entry into an owned heap string before the window runs.
/// Emits a literal `php://filter/…` read: open, read to the end, close.
///
/// `fopen()` on the same URI already worked — it parses the URL, opens the wrapped resource
/// and stamps the filter chain — so this reuses that opener rather than teaching the
/// filesystem helper about filters. The bytes are taken into owned storage BEFORE the close,
/// because the read answers with the stream's own buffer and closing it takes that away.
/// Reads a literal URI whose scheme belongs to no built-in wrapper through `fopen()`.
///
/// php-src has no separate reader here: `file_get_contents` is `php_stream_open_wrapper` followed
/// by `_php_stream_copy_to_mem`, so every scheme the opener knows is readable by definition. The
/// hand-rolled scheme ladder underneath knows `http`, `https` and `ftp` and then falls back to a
/// filename, which is why a registered user wrapper answered `Failed to open stream` here while
/// `fopen()` on the same URI worked. The opener already scans the wrapper registry and reports an
/// unknown scheme the way php does, so delegating covers both outcomes.
/// php-src's `CHECK_NULL_PATH` wording for an empty filename reaching a stream open.
/// Whether this URL names one of php's OWN sub-streams, the ones only the opener can serve.
///
/// `php://filter/` is deliberately absent: it needs a filter chain attached to the stream it
/// wraps, and both one-shot builtins have their own route for it. Everything else under the
/// scheme — `memory`, `temp` (with or without `/maxmemory:N`), `input`, `output`, `stdin`,
/// `stdout`, `stderr`, `fd/N` — is an ordinary open followed by one read or one write, which is
/// exactly what php-src does for them.
pub(super) fn is_php_substream_uri(uri: &str) -> bool {
    uri.starts_with("php://") && !uri.starts_with("php://filter/")
}

pub(super) const EMPTY_PATH_MESSAGE: &str = "Path must not be empty";

/// `scandir()`'s own empty-directory wording, which is NOT the shared one.
///
/// MEASURED: php prints `scandir(): Argument #1 ($directory) must not be empty` here, while the
/// openers print the bare `Path must not be empty`. Reusing the shared message would be a
/// plausible-looking lie that no test comparing against php would forgive.
pub(super) const SCANDIR_EMPTY_PATH_MESSAGE: &str =
    "scandir(): Argument #1 ($directory) must not be empty";

/// Throws php's `ValueError` when the filename is empty, before anything tries to open it.
///
/// MEASURED on `php -n` 8.5.6: `fopen("")`, `file_get_contents("")`, `file_put_contents("", "x")`,
/// `file("")`, `readfile("")`, `copy("", "x")` and `hash_file("md5", "")` all throw
/// `ValueError: Path must not be empty`, while `unlink("")`, `mkdir("")`, `is_file("")` and the
/// rest of the path-taking surface answer plain `false`. The guard therefore belongs to the
/// OPENERS, not to every builtin holding a path — the twenty that answer `false` already agree
/// with php, and widening this would break them.
///
/// A literal empty path is refused during lowering, so the throw costs nothing at run time for
/// every other program; an assembled filename is checked on its byte length, where php checks it.
pub(super) fn emit_empty_path_value_error(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    message: &str,
) -> Result<()> {
    if let Some(literal) = optional_const_string_operand(ctx, path)? {
        if literal.is_empty() {
            super::exceptions::emit_value_error(ctx, message);
        }
        return Ok(());
    }
    load_string_to_result(ctx, path, "path")?;
    let (_, len_reg) = abi::string_result_regs(ctx.emitter);
    super::exceptions::emit_value_error_unless(
        ctx,
        super::exceptions::ValueGuard::SignedAtLeast(len_reg, 1),
        message,
    );
    Ok(())
}

fn emit_literal_wrapper_file_get_contents_bytes(
    ctx: &mut FunctionContext<'_>,
    path: &str,
    callee: &str,
) -> Result<()> {
    // php opens its OWN streams in BINARY mode — a userspace wrapper sees `rb` from
    // `file_get_contents()`, `file()`, `readfile()` and `copy()`, and `wb` from
    // `file_put_contents()`, while an explicit `fopen($p, "r")` passes the string the
    // caller wrote. MEASURED on `php -n` 8.5.6. The `b` is a no-op on POSIX, so this only
    // matters to a wrapper that inspects its $mode — which is exactly what it is given for.
    fopen_core::emit_literal_fopen_result(ctx, LiteralOpenMode::Fixed("rb"), path, callee)?;
    emit_open_read_close_tail(ctx, "fgc_wrapper")
}

fn emit_literal_php_filter_file_get_contents_bytes(
    ctx: &mut FunctionContext<'_>,
    path: &str,
    callee: &str,
) -> Result<()> {
    // php names `file_get_contents` in BOTH diagnostics a literal filter URL can print — the
    // failed-open line and the two lines an unresolvable filter name earns — so the callee
    // travels into the shared emitter instead of this route composing a second copy. Wrapping
    // that emitter in suppression, which is what this route used to do, silenced the
    // unresolvable-name warnings along with the inner opener's: php prints them, and elephc
    // turned a typo in a filter name into a silently unfiltered read.
    // See above: php's own opens are binary.
    emit_literal_php_filter_fopen_result(ctx, LiteralOpenMode::Fixed("rb"), path, callee)?;
    emit_open_read_close_tail(ctx, "fgc_filter")
}

/// Gives a RUN-TIME `php://filter/...` filename the read `fopen()` already performs.
///
/// The literal spelling is resolved during lowering above; a URL assembled at run time reached
/// the plain byte reader, which never creates a stream, so the filter chain had nowhere to
/// attach and the read failed loudly naming the whole URL as a path. This route mirrors the
/// dynamic `fopen()` shape instead: the parse publishes the filter list and swaps the path to
/// the RESOURCE, the resource is opened read-only through the same runtime openers `fopen()`
/// dispatches to, the pending chain is attached once the stream is boxed, and the bytes come
/// back through the shared open-read-close tail.
///
/// Emits everything up to and including the fall-through label; the caller places the returned
/// done-label AFTER its plain reader, so the streamed result skips it. A URL that names no
/// usable filter falls through WITH the swapped resource, which is the unfiltered open php
/// performs for the same URL.
fn emit_dynamic_php_filter_read_route(
    ctx: &mut FunctionContext<'_>,
    prefix_symbol: &str,
    prefix_text: &str,
    callee: &str,
) -> Result<String> {
    let fall_through = ctx.next_label("fgc_dynf_plain");
    let done = ctx.next_label("fgc_dynf_done");
    let try_data = ctx.next_label("fgc_dynf_try_data");
    let try_http = ctx.next_label("fgc_dynf_try_http");
    let open_file = ctx.next_label("fgc_dynf_file");
    let boxed = ctx.next_label("fgc_dynf_boxed");
    // The full URL is what php names in the failure warning, and the parse is about to swap the
    // registers to the RESOURCE, so the pair is saved first. Every exit releases it.
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg_pair(ctx.emitter, "x1", "x2"),
        Arch::X86_64 => abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx"),
    }
    // These readers open read-only, so a prefix-less filter list is applied exactly once.
    fopen_core::emit_dynamic_php_filter_swap(ctx, fopen_core::DynamicFilterMode::Fixed(1));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // The gate is the URL the parse published, NOT the pending mode: that one reads 0
            // exactly when the URL IS a filter URL whose every name failed to resolve, which
            // sent the one case php has the most to say about down the plain reader — naming
            // the swapped RESOURCE on failure and skipping the names in silence on success.
            abi::emit_symbol_address(ctx.emitter, "x9", "_php_filter_url_ptr");
            ctx.emitter.instruction("ldr x9, [x9]");                            // did the parse see a filter URL at all?
            ctx.emitter.instruction(&format!("cbz x9, {}", fall_through));      // no: the plain reader handles the path
            // The openers name themselves and the RESOURCE when they fail; php names
            // `file_get_contents` and the whole URL, so their warnings are suppressed and the
            // php-worded one is composed below once the outcome is known. Through the FILTER
            // counter, not `@`'s: the resource may be a user wrapper whose `stream_open` is PHP,
            // and php prints what that PHP warns. `__rt_fopen` stands this scope down for the
            // length of the dispatch, which it can only do to a counter `@` does not share.
            abi::emit_call_label(ctx.emitter, "__rt_diag_push_filter_suppression");
            // The resource may be a user wrapper, whose `stream_open` is PHP and can `fopen()` a
            // filter URL of its own — which republishes the hand-off this route is holding.
            fopen_core::emit_dynamic_php_filter_save(ctx);
            // -- the resource decides the opener, exactly as it does for fopen() --
            ctx.emitter.instruction("cmp x2, #6");                              // long enough for php://?
            ctx.emitter.instruction(&format!("b.lt {}", try_data));
            for (offset, byte) in b"php://".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset)); // one candidate scheme byte
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));
                ctx.emitter.instruction(&format!("b.ne {}", try_data));
            }
            ctx.emitter.instruction("mov x0, x1");                              // the wrapper opener takes ptr/len in x0/x1
            ctx.emitter.instruction("mov x1, x2");
            abi::emit_call_label(ctx.emitter, "__rt_php_wrapper_open");
            ctx.emitter.instruction(&format!("b {}", boxed));
            ctx.emitter.label(&try_data);
            // `data:` is the whole scheme — RFC 2397 has no `//`, and php makes it optional, so
            // the canonical spelling php's own tests use is `data:,abc` and
            // `data:text/plain;base64,...`. Matching `data://` here accepted only the rarer
            // form, so `file_get_contents("data:,abc")` fell through to the FILE opener and
            // answered false with "No such file or directory" — while `fopen()` on the same
            // URL, which tests the five-byte scheme, read it. `data://` still matches: it
            // starts with `data:` too.
            ctx.emitter.instruction("cmp x2, #6");                              // `data:` plus at least a comma
            ctx.emitter.instruction(&format!("b.lt {}", try_http));
            for (offset, byte) in b"data:".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset));
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));
                ctx.emitter.instruction(&format!("b.ne {}", try_http));
            }
            ctx.emitter.instruction("mov x0, x1");                              // the data decoder takes ptr/len in x0/x1
            ctx.emitter.instruction("mov x1, x2");
            abi::emit_call_label(ctx.emitter, "__rt_data_stream_dynamic");
            ctx.emitter.instruction(&format!("b {}", boxed));
            ctx.emitter.label(&try_http);
            ctx.emitter.instruction("cmp x2, #7");                              // long enough for http://?
            ctx.emitter.instruction(&format!("b.lt {}", open_file));
            for (offset, byte) in b"http://".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset));
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));
                ctx.emitter.instruction(&format!("b.ne {}", open_file));
            }
            abi::emit_call_label(ctx.emitter, "__rt_http_open_url");            // takes the pair in x1/x2 as staged
            ctx.emitter.instruction(&format!("b {}", boxed));
            ctx.emitter.label(&open_file);
            abi::emit_symbol_address(ctx.emitter, "x3", "_fgc_mode_r");         // file_get_contents opens read-only
            ctx.emitter.instruction("mov x4, #1");                              // one mode byte
            abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
        }
        Arch::X86_64 => {
            // See the AArch64 counterpart: the URL flag, not the pending mode.
            abi::emit_symbol_address(ctx.emitter, "r9", "_php_filter_url_ptr");
            ctx.emitter.instruction("mov r9, QWORD PTR [r9]");                  // did the parse see a filter URL at all?
            ctx.emitter.instruction("test r9, r9");
            ctx.emitter.instruction(&format!("jz {}", fall_through));           // no: the plain reader handles the path
            // See the AArch64 counterpart: the openers' own failure warnings are suppressed,
            // through the filter counter so a user wrapper's `stream_open` still warns.
            abi::emit_call_label(ctx.emitter, "__rt_diag_push_filter_suppression");
            // See the AArch64 counterpart: a user wrapper's `stream_open` republishes the hand-off.
            fopen_core::emit_dynamic_php_filter_save(ctx);
            ctx.emitter.instruction("cmp rdx, 6");                              // long enough for php://?
            ctx.emitter.instruction(&format!("jl {}", try_data));
            for (offset, byte) in b"php://".iter().enumerate() {
                ctx.emitter.instruction(&format!("cmp BYTE PTR [rax + {}], {}", offset, byte));
                ctx.emitter.instruction(&format!("jne {}", try_data));
            }
            ctx.emitter.instruction("mov rdi, rax");                            // the wrapper opener takes ptr/len in rdi/rsi
            ctx.emitter.instruction("mov rsi, rdx");
            abi::emit_call_label(ctx.emitter, "__rt_php_wrapper_open");
            ctx.emitter.instruction(&format!("jmp {}", boxed));
            ctx.emitter.label(&try_data);
            // See the AArch64 arm: the scheme is `data:`, and the `//` is optional in php.
            ctx.emitter.instruction("cmp rdx, 6");                              // `data:` plus at least a comma
            ctx.emitter.instruction(&format!("jl {}", try_http));
            for (offset, byte) in b"data:".iter().enumerate() {
                ctx.emitter.instruction(&format!("cmp BYTE PTR [rax + {}], {}", offset, byte));
                ctx.emitter.instruction(&format!("jne {}", try_http));
            }
            ctx.emitter.instruction("mov rdi, rax");                            // the data decoder takes ptr/len in rdi/rsi
            ctx.emitter.instruction("mov rsi, rdx");
            abi::emit_call_label(ctx.emitter, "__rt_data_stream_dynamic");
            ctx.emitter.instruction(&format!("jmp {}", boxed));
            ctx.emitter.label(&try_http);
            ctx.emitter.instruction("cmp rdx, 7");                              // long enough for http://?
            ctx.emitter.instruction(&format!("jl {}", open_file));
            for (offset, byte) in b"http://".iter().enumerate() {
                ctx.emitter.instruction(&format!("cmp BYTE PTR [rax + {}], {}", offset, byte));
                ctx.emitter.instruction(&format!("jne {}", open_file));
            }
            abi::emit_call_label(ctx.emitter, "__rt_http_open_url");            // takes the pair in rax/rdx as staged
            ctx.emitter.instruction(&format!("jmp {}", boxed));
            ctx.emitter.label(&open_file);
            abi::emit_symbol_address(ctx.emitter, "rdi", "_fgc_mode_r");        // file_get_contents opens read-only
            ctx.emitter.instruction("mov rsi, 1");                              // one mode byte
            abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
        }
    }
    ctx.emitter.label(&boxed);
    box_stream_fd_or_false_result(ctx, "fgc_dynf");
    abi::emit_call_label(ctx.emitter, "__rt_diag_pop_filter_suppression");      // preserves the boxed result: x9/x10 (r10) only
    fopen_core::emit_dynamic_php_filter_restore(ctx);                           // this route's own hand-off, not a nested open's
    abi::emit_call_label(ctx.emitter, "__rt_php_filter_attach_pending");        // the parked chain, now the stream exists
    // php warns twice for every name that named no filter, in the CALLING function's words, and
    // keeps the stream. A failed open never reaches the filters, and the report checks the tag.
    fopen_core::emit_php_filter_unknown_report(ctx, callee);
    // A failed filtered open warns in php's own words: the function, the WHOLE URL, and the
    // wrapper's generic reason — `file_get_contents(<url>): Failed to open stream: operation
    // failed` — not the inner opener's name and the bare resource path.
    let opened = ctx.next_label("fgc_dynf_opened");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed open result tag
            ctx.emitter.instruction("cmp x9, #9");                              // a resource has nothing to warn about
            ctx.emitter.instruction(&format!("b.eq {}", opened));
            abi::emit_push_reg(ctx.emitter, "x0");                              // hold the boxed false across the fragments
            abi::emit_symbol_address(ctx.emitter, "x1", prefix_symbol);
            ctx.emitter.instruction(&format!("mov x2, #{}", prefix_text.len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // the saved full URL
            ctx.emitter.instruction("ldr x2, [sp, #24]");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "x1", "_fgc_filter_fail_tail");
            ctx.emitter.instruction(&format!(
                "mov x2, #{}",
                crate::codegen_support::runtime::data::FGC_FILTER_FAIL_TAIL.len()
            ));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // the boxed open result tag
            ctx.emitter.instruction("cmp r9, 9");                               // a resource has nothing to warn about
            ctx.emitter.instruction(&format!("je {}", opened));
            abi::emit_push_reg(ctx.emitter, "rax");                             // hold the boxed false across the fragments
            abi::emit_symbol_address(ctx.emitter, "rdi", prefix_symbol);
            ctx.emitter.instruction(&format!("mov rsi, {}", prefix_text.len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // the saved full URL
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "rdi", "_fgc_filter_fail_tail");
            ctx.emitter.instruction(&format!(
                "mov rsi, {}",
                crate::codegen_support::runtime::data::FGC_FILTER_FAIL_TAIL.len()
            ));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&opened);
    abi::emit_release_temporary_stack(ctx.emitter, 16);                         // drop the saved URL
    emit_open_read_close_tail(ctx, "fgc_dynf")?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fall_through);
    abi::emit_release_temporary_stack(ctx.emitter, 16);                         // the plain path drops the saved URL too
    Ok(done)
}

/// Reads a RUN-TIME `compress.zlib://` / `compress.bzip2://` URL through the opener `fopen()` uses.
///
/// The literal spelling is resolved during lowering; a URL assembled at run time — a filename from
/// config, a path built with `sys_get_temp_dir()`, or the concatenation any wrapper-forwarding
/// helper writes — reached the plain byte reader and was opened as a FILENAME, so
/// `file_get_contents("compress.zlib://" . $p)` answered `Failed to open stream: No such file or
/// directory` while `fopen()` on the identical string decompressed. Measured against `php -n`
/// 8.5.6, which reads it.
///
/// The prefix probe is given its own copy of the staged filename each time and the pair is
/// restored on the way out, so a URL none of the wrappers claims leaves the registers exactly as
/// it found them for the routes that follow. A claimed URL runs the same open the literal path
/// runs and ends in the shared open-read-close tail, then branches to `bytes_ready`.
///
/// The landing label comes FROM the caller because the three readers do different things with the
/// bytes: `file_get_contents` answers them, `file()` splits them into lines and `readfile()`
/// writes and counts them. Each already has an entry that takes bytes in the string registers —
/// the one a filter chain feeds — and that is the label to pass.
fn emit_dynamic_compress_read_route(
    ctx: &mut FunctionContext<'_>,
    filename: ValueId,
    callee: &str,
    bytes_ready: &str,
) -> Result<()> {
    for (prefix, kind) in DYNAMIC_COMPRESS_WRAPPERS.iter().copied() {
        let next = ctx.next_label("fgc_dyn_compress_next");
        let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
        load_string_to_result(ctx, filename, callee)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                // `__rt_str_starts_with` reads the haystack from x1/x2, where the load left it.
                abi::emit_symbol_address(ctx.emitter, "x3", &prefix_label);
                ctx.emitter.instruction(&format!("mov x4, #{prefix_len}"));
                abi::emit_call_label(ctx.emitter, "__rt_str_starts_with");
                ctx.emitter.instruction(&format!("cbz x0, {}", next));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // the url pointer
                ctx.emitter.instruction("mov rsi, rdx");                        // and its byte length
                ctx.emitter
                    .instruction(&format!("lea rdx, [rip + {prefix_label}]"));  // the wrapper prefix
                ctx.emitter.instruction(&format!("mov rcx, {prefix_len}"));
                abi::emit_call_label(ctx.emitter, "__rt_str_starts_with");
                ctx.emitter.instruction("test rax, rax");
                ctx.emitter.instruction(&format!("je {}", next));
            }
        }
        // The probe consumed the staged pair; the opener reads the url from it to step past the
        // prefix it just matched, so it is loaded again.
        load_string_to_result(ctx, filename, callee)?;
        emit_literal_compress_wrapper_fopen_result(
            ctx,
            CompressUnderlying::Staged { prefix_len },
            prefix,
            kind,
            "r",
        )?;
        emit_open_read_close_tail(ctx, "fgc_dyn_compress")?;
        abi::emit_jump(ctx.emitter, bytes_ready);
        ctx.emitter.label(&next);
    }
    // Nothing claimed it: hand the following routes the filename they expect in the registers.
    load_string_to_result(ctx, filename, callee)?;
    Ok(())
}

/// Turns a boxed `fopen()` result already in the result register into the bytes it holds.
///
/// Reads the whole stream, takes ownership of the bytes before the close reclaims the buffer,
/// closes the stream php opened on the caller's behalf, and leaves the pair in the string result
/// registers. A failed open leaves a null pointer, which the boxer reads as PHP `false`.
fn emit_open_read_close_tail(ctx: &mut FunctionContext<'_>, label_prefix: &str) -> Result<()> {
    let fail = ctx.next_label(&format!("{label_prefix}_failed"));
    let done = ctx.next_label(&format!("{label_prefix}_done"));
    let stat_done = ctx.next_label(&format!("{label_prefix}_stat_done"));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_reserve_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed open result tag
            ctx.emitter.instruction("cmp x9, #9");                              // did it answer a resource?
            ctx.emitter.instruction(&format!("b.ne {}", fail));                 // a failed open reads as PHP false
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // the opaque stream handle
            ctx.emitter.instruction("str x0, [sp, #0]");                        // keep it for the close
            // php STATS the stream it just opened, before reading it: `file_get_contents()` and
            // `file()` size their buffer from `stream_stat()`, and a userspace wrapper sees that
            // call. MEASURED on `php -n` 8.5.6 against a wrapper that traces its own calls.
            // elephc reads in chunks and does not need the size, but the call is part of the
            // protocol the wrapper is written against, so it is made and its answer released.
            //
            // The value this tail carries is an OPAQUE REGISTRY HANDLE; `__rt_user_wrapper_fstat`
            // indexes the handle TABLE by synthetic fd. Passing the one where the other belongs
            // resolved a garbage object, and `blr` on its garbage vtable slot was a SIGBUS —
            // twice, before `__rt_stream_fd` was put between them.
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");                // x0 = the descriptor behind the handle
            ctx.emitter.instruction("mov x9, #0x40000000");                     // USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("cmp x0, x9");
            ctx.emitter.instruction(&format!("b.lt {}", stat_done));            // a plain file has no wrapper to ask
            abi::emit_symbol_address(ctx.emitter, "x1", "_uwmh_head_fgc");      // php names the CALLER, not fstat()
            ctx.emitter.instruction(&format!(
                "mov x2, #{}",
                crate::codegen_support::runtime::data::WRAPPER_MISSING_HOOK_HEAD_FILE_GET_CONTENTS.len()
            ));
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fstat");       // stream_stat($this)
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");               // the answer may be a TAGGED value, not a cell
            ctx.emitter.label(&stat_done);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the handle again, for the read
            ctx.emitter.instruction("mov x1, #0");                              // ask for the helper's default read chunk
            abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents");      // x1/x2 = the filtered bytes
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");              // own them before the close reclaims the buffer
            ctx.emitter.instruction("str x1, [sp, #8]");
            ctx.emitter.instruction("str x2, [sp, #16]");
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
                                                                        // php closes the stream it opened for us,
                                                                        // and CLOSING it is what runs a userspace
                                                                        // wrapper's stream_flush/stream_close:
                                                                        // marking it closed skipped both.
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // the owned bytes are the result
            ctx.emitter.instruction("ldr x2, [sp, #16]");
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("mov x1, #0");                              // null string pointer asks the boxer for PHP false
            ctx.emitter.instruction("mov x2, #0");
            ctx.emitter.label(&done);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
        Arch::X86_64 => {
            abi::emit_reserve_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // the boxed open result tag
            ctx.emitter.instruction("cmp r9, 9");                               // did it answer a resource?
            ctx.emitter.instruction(&format!("jne {}", fail));                  // a failed open reads as PHP false
            ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");            // the opaque stream handle
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // keep it for the close
            // See the AArch64 arm, including why `__rt_stream_fd` has to come first.
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");                // rax = the descriptor behind the handle
            ctx.emitter.instruction("mov r9, 0x40000000");                      // USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("cmp rax, r9");
            ctx.emitter.instruction(&format!("jl {}", stat_done));              // a plain file has no wrapper to ask
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_symbol_address(ctx.emitter, "rsi", "_uwmh_head_fgc");     // php names the CALLER, not fstat()
            ctx.emitter.instruction(&format!(
                "mov rdx, {}",
                crate::codegen_support::runtime::data::WRAPPER_MISSING_HOOK_HEAD_FILE_GET_CONTENTS.len()
            ));
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fstat");       // stream_stat($this)
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");               // the answer may be a TAGGED value, not a cell
            ctx.emitter.label(&stat_done);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // the handle again, for the read
            ctx.emitter.instruction("mov rdi, rax");
            ctx.emitter.instruction("xor esi, esi");                            // ask for the helper's default read chunk
            abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents");      // rax/rdx = the filtered bytes
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");              // own them before the close reclaims the buffer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdx");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");     // see the AArch64 counterpart
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // the owned bytes are the result
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                            // null string pointer asks the boxer for PHP false
            ctx.emitter.instruction("xor edx, edx");
            ctx.emitter.label(&done);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
    }
    Ok(())
}

/// Emits the decoded payload of a literal `data://` URI as the read's bytes.
///
/// `fopen("data://…")` already decodes these at compile time; `file_get_contents()` went
/// through the filesystem helper instead and answered false with "No such file or directory",
/// naming a path that was never meant to be one. A malformed URI keeps that answer, which is
/// what php does with it too.
fn emit_literal_data_uri_file_get_contents_bytes(
    ctx: &mut FunctionContext<'_>,
    path: &str,
    persist: bool,
) {
    match decode_data_uri_for_fopen(path) {
        Some(payload) => {
            let (symbol, len) = ctx.data.add_string(&payload);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x1", &symbol);
                    ctx.emitter.instruction(&format!("mov x2, #{}", len));      // decoded data:// payload byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rax", &symbol);
                    ctx.emitter.instruction(&format!("mov rdx, {}", len));      // decoded data:// payload byte length
                }
            }
            if persist {
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            }
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x1, #0");                          // null string pointer asks the boxer for PHP false
                ctx.emitter.instruction("mov x2, #0");                          // clear the unused failure length
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor eax, eax");                        // null string pointer asks the boxer for PHP false
                ctx.emitter.instruction("xor edx, edx");                        // clear the unused failure length
            }
        },
    }
}

fn emit_literal_phar_file_get_contents_bytes(
    ctx: &mut FunctionContext<'_>,
    path: &str,
    persist: bool,
) {
    match crate::codegen::phar_stream::extract_phar_entry(path) {
        Some(payload) => {
            let (symbol, len) = ctx.data.add_string(&payload);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x1", &symbol);
                    ctx.emitter.instruction(&format!("mov x2, #{}", len));      // embedded phar entry byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rax", &symbol);
                    ctx.emitter.instruction(&format!("mov rdx, {}", len));      // embedded phar entry byte length
                }
            }
            if persist {
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            }
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x1, #0");                          // null string pointer asks the boxer for PHP false
                ctx.emitter.instruction("mov x2, #0");                          // clear the unused failure length
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor eax, eax");                        // null string pointer asks the boxer for PHP false
                ctx.emitter.instruction("xor edx, edx");                        // clear the unused failure length
            }
        },
    }
}
