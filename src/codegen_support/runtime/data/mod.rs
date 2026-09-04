//! Purpose:
//! Collects runtime data-section emitters and shared diagnostic string constants.
//! The module separates cacheable fixed data from user-program metadata emitted during compilation.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emit_runtime_data_fixed()` and `crate::codegen_support::runtime::emit_runtime_data_user()`.
//!
//! Key details:
//! - Symbol names and table layouts are link-time ABI shared with generated code and runtime helper labels.

mod fixed;
/// Also home of `escaped_bytes()`, the crate's single assembler-string escaper:
/// reachable outside this module so non-runtime emitters (`crate::debug_info`)
/// escape quoted directive operands the same way.
pub(crate) mod instanceof;
mod user;

pub(crate) use fixed::emit_runtime_data_fixed;
/// Emit fixed runtime data section (heap globals, fatal/assertion messages, lookup tables, builtin callable metadata).
pub(crate) use user::emit_runtime_data_user;
pub(crate) use user::{
    is_user_filter_contract_method, is_user_wrapper_contract_method, is_user_wrapper_marker_method,
};
pub(crate) use user::USER_WRAPPER_VTABLE_BOXED_MASK_OFFSET;
pub(crate) use user::USER_WRAPPER_VTABLE_CONTEXT_OFFSET;
pub(crate) use user::USER_WRAPPER_VTABLE_CTOR_OFFSET;

/// Fatal error message when `php_uname()` receives a `$mode` argument whose length is not exactly 1.
pub(crate) const PHP_UNAME_MODE_LEN_MSG: &str =
    "Fatal error: php_uname(): Argument #1 ($mode) must be a single character\n";
/// Fatal error message when `php_uname()` receives a `$mode` argument that is not one of the supported single-character values.
pub(crate) const PHP_UNAME_MODE_VALUE_MSG: &str =
    "Fatal error: php_uname(): Argument #1 ($mode) must be one of \"a\", \"m\", \"n\", \"r\", \"s\", or \"v\"\n";
/// Fatal error message when `dirname()` receives a `$levels` argument less than 1.
/// ob_* PHP-parity diagnostics shared by the fixed data section and the
/// output-buffering runtime emitters (which need the exact byte lengths).
/// PHP's Notice when `stream_wrapper_restore()` is given a built-in scheme that was never
/// unregistered. Completed at run time with the scheme name and the suffix below.
pub(crate) const SWR_NTC_PREFIX: &str = "Notice: stream_wrapper_restore(): ";

/// PHP's Warning when `stream_wrapper_restore()` is given a scheme that never existed.
pub(crate) const SWR_WRN_PREFIX: &str = "Warning: stream_wrapper_restore(): ";

/// Tail of the never-unregistered Notice, after the scheme name.
pub(crate) const SWR_NEVER_CHANGED: &str = ":// was never changed, nothing to restore\n";

/// Tail of the unknown-scheme Warning, after the scheme name.
pub(crate) const SWR_NEVER_EXISTED: &str = ":// never existed, nothing to restore\n";

/// PHP's Warning when a socket builtin cannot open its endpoint. The address and the reason are
/// only known at run time, so each of these is a prefix the runtime completes.
/// PHP uses the same "Unable to connect to" wording for a failed bind, which reads oddly but is
/// what `stream_socket_server()` prints.
pub(crate) const SOCKET_FAILED_CLIENT_PREFIX: &str = "Warning: stream_socket_client(): ";

/// The `stream_socket_server()` form of [`SOCKET_FAILED_CLIENT_PREFIX`].
pub(crate) const SOCKET_FAILED_SERVER_PREFIX: &str = "Warning: stream_socket_server(): ";

/// The `fsockopen()` form of [`SOCKET_FAILED_CLIENT_PREFIX`]. Its address is a bare host, so the
/// runtime appends `:` and the port to reach PHP's `host:port` spelling.
pub(crate) const SOCKET_FAILED_FSOCKOPEN_PREFIX: &str = "Warning: fsockopen(): ";

/// Bytes reserved for the diagnostic line buffer.
///
/// One php diagnostic, composed from however many pieces the emitting helper writes. The longest
/// in this compiler names a filter, a wrapper class and a method, and stays well inside this; a
/// message that would overflow is truncated rather than allowed to run past the buffer, because a
/// clipped warning is a far better outcome than a corrupted heap.
pub(crate) const RT_DIAG_BUF_BYTES: usize = 4096;

/// The whole warning php raises when `glob()` is handed a flag it does not expose.
///
/// php validates `$flags` against `GLOB_AVAILABLE_FLAGS` and answers `false`. Measured on `php -n`
/// 8.5.6: `glob("*", 64)`, `glob("*", 1024)` — glibc's own `GLOB_BRACE` — and `glob("*", -1)` all
/// take this path, because php 8.5 ships its own glob and none of those are php's bits.
pub(crate) const GLOB_INVALID_FLAGS_WARNING: &str =
    "Warning: glob(): At least one of the passed flags is invalid or not supported on this platform\n";

/// Head of the connect-failure line, after the `Warning: <fn>(): ` the three prefixes carry.
pub(crate) const SOCKET_FAILED_UNABLE: &str = "Unable to connect to ";

/// Head of the first warning php raises when a built-in filter cannot read its `$params`.
///
/// Only the four `convert.*` filters parse `$params`; the rest never look at it and accept
/// anything. Measured on `php -n` 8.5.6: passing `null` explicitly raises BOTH this and
/// [`FILTER_PARAM_CREATE_APPEND_HEAD`], and the call answers `false`.
pub(crate) const FILTER_PARAM_INVALID_APPEND_HEAD: &str =
    "Warning: stream_filter_append(): Stream filter (";

/// The `stream_filter_prepend()` form of [`FILTER_PARAM_INVALID_APPEND_HEAD`].
pub(crate) const FILTER_PARAM_INVALID_PREPEND_HEAD: &str =
    "Warning: stream_filter_prepend(): Stream filter (";

/// Tail of that first warning, after the filter name.
pub(crate) const FILTER_PARAM_INVALID_TAIL: &str = "): invalid filter parameter\n";

/// Head of the second warning, which reports the attach itself as having failed.
///
/// The verb differs from the unknown-name wording: php says "create or locate" when the filter
/// EXISTS but refused its parameters, and plain "locate" when the name resolves to nothing.
pub(crate) const FILTER_PARAM_CREATE_APPEND_HEAD: &str =
    "Warning: stream_filter_append(): Unable to create or locate filter \"";

/// The `stream_filter_prepend()` form of [`FILTER_PARAM_CREATE_APPEND_HEAD`].
pub(crate) const FILTER_PARAM_CREATE_PREPEND_HEAD: &str =
    "Warning: stream_filter_prepend(): Unable to create or locate filter \"";

/// Tail of the second warning, after the filter name.
pub(crate) const FILTER_PARAM_CREATE_TAIL: &str = "\"\n";

/// Head of the message PHP reports when a socket address names a host that does not resolve.
///
/// php-src composes this itself rather than using an `errno`, which is why the `&$error_code` of
/// such a failure is `0` and only `&$error_message` says anything.
pub(crate) const GAI_MSG_PREFIX: &str = "php_network_getaddresses: getaddrinfo for ";

/// Middle of the unresolvable-host message, between the host name and the resolver's own text.
pub(crate) const GAI_MSG_MIDDLE: &str = " failed: ";

/// Bytes reserved for the composed unresolvable-host message.
///
/// Prefix, a host clamped to the 255 bytes a DNS name can hold, the middle, and the resolver text
/// clamped to 200 all fit. The buffer is static so the message the caller receives needs no
/// allocation and cannot dangle; a second failed resolution overwrites it.
pub(crate) const SOCKET_GAI_MSG_CAPACITY: usize = 512;

/// Longest host name copied into the composed message.
pub(crate) const SOCKET_GAI_HOST_CLAMP: i64 = 255;

/// Longest resolver text copied into the composed message.
pub(crate) const SOCKET_GAI_REASON_CLAMP: i64 = 200;

/// Separator between the socket warning's address and the reason for the failure.
pub(crate) const SOCKET_FAILED_REASON_OPEN: &str = " (";

/// What the warning says when the failure carries no reason at all.
///
/// php-src prints `errstr == NULL ? "Unknown error" : errstr`, so a failure no syscall described —
/// a datagram transport asked to listen, for one — still names something inside the parentheses.
/// Only the warning substitutes it: the caller's `&$error_message` stays the empty string PHP
/// leaves there, so this cannot come from `__rt_socket_strerror`, which feeds both.
pub(crate) const SOCKET_FAILED_REASON_UNKNOWN: &str = "Unknown error";

/// Tail of the socket warning, after the reason.
pub(crate) const SOCKET_FAILED_REASON_CLOSE: &str = ")\n";

/// The `count()` TypeError texts, indexed the way `__rt_count_reject_index` answers.
///
/// PHP names the type with the VALUE's own spelling — a boolean reports `true` or `false`, not
/// `bool` — so these are read off `php -n` 8.5.6 rather than derived from the tag names. A
/// non-`Countable` object is absent on purpose: PHP names the class there, which needs a lookup
/// and a composed string.
pub(crate) const COUNT_TYPE_ERROR_MESSAGES: [&str; 7] = [
    "count(): Argument #1 ($value) must be of type Countable|array, int given",
    "count(): Argument #1 ($value) must be of type Countable|array, string given",
    "count(): Argument #1 ($value) must be of type Countable|array, float given",
    "count(): Argument #1 ($value) must be of type Countable|array, true given",
    "count(): Argument #1 ($value) must be of type Countable|array, false given",
    "count(): Argument #1 ($value) must be of type Countable|array, null given",
    "count(): Argument #1 ($value) must be of type Countable|array, resource given",
];

/// Head of PHP 8.2's deprecation for a property it has to invent.
///
/// A stream wrapper that declares no `$context` still receives one: PHP assigns it and, since
/// 8.2, deprecates the assignment. The class name sits between the two fragments.
pub(crate) const DYNAMIC_PROP_DEPRECATED_HEAD: &str =
    "Deprecated: Creation of dynamic property ";

/// Tail of [`DYNAMIC_PROP_DEPRECATED_HEAD`], after the class name.
pub(crate) const DYNAMIC_PROP_DEPRECATED_TAIL: &str = "::$context is deprecated\n";

/// A lone newline, for a diagnostic written out in fragments.
pub(crate) const DIAG_NEWLINE: &str = "\n";

/// Head of the warning `disk_free_space()` prints for a path it cannot stat.
///
/// PHP names the function and the reason and nothing else here — no path, unlike the failed-open
/// warning — so the runtime only has to append `strerror` and a newline.
pub(crate) const DISK_FREE_SPACE_WARNING: &str = "Warning: disk_free_space(): ";

/// The `disk_total_space()` form of [`DISK_FREE_SPACE_WARNING`].
pub(crate) const DISK_TOTAL_SPACE_WARNING: &str = "Warning: disk_total_space(): ";

/// Tail of the warning a failed run-time `php://filter` read composes.
///
/// The wrapper reports the generic `operation failed` rather than the inner opener's errno,
/// and php names `file_get_contents` and the WHOLE URL ahead of it; the inner opener's own
/// warning — which would name `fopen` and the bare resource path — is suppressed around the
/// open.
pub(crate) const FGC_FILTER_FAIL_TAIL: &str = "): Failed to open stream: operation failed\n";

/// The fragments a run-time `php://filter` diagnostic is composed from.
///
/// php names the CALLING function in every one of these — `fopen(): Unable to locate filter
/// "x"`, `file_get_contents(): Unable to create filter (x)` — and the name is the only part
/// that varies, so the surrounding text is interned once and the caller supplies the middle.
/// Both lines exist because php-src prints one from `php_stream_filter_create`
/// (main/streams/filter.c) and the next from `php_stream_apply_filter_list`, and neither
/// cancels the open.
pub(crate) const PF_WARN_HEAD: &str = "Warning: ";
/// Between the callee name and the name php could not resolve.
pub(crate) const PF_WARN_LOCATE_MID: &str = "(): Unable to locate filter \"";
/// Closes the `locate` line.
pub(crate) const PF_WARN_LOCATE_END: &str = "\"\n";
/// Between the callee name and the name php could not create a filter from.
pub(crate) const PF_WARN_CREATE_MID: &str = "(): Unable to create filter (";
/// Closes the `create` line.
pub(crate) const PF_WARN_CREATE_END: &str = ")\n";
/// Opens the failed-open line, between the callee name and the URL it names.
pub(crate) const PF_WARN_OPEN_MID: &str = "(";

/// The halves php wraps around a wrapper class's name when a `streamWrapper` hook is missing.
///
/// php warns on every call into a hook the registered class does not implement, naming the CALLER
/// and then the class and method: `Warning: fwrite(): M::stream_write is not implemented!`. Only
/// the class name is a run-time value, so each site hands
/// `__rt_wrapper_missing_hook_warning` the fixed text before and after it. All measured against
/// php 8.5.6 — note `feof()`'s tail, which alone says what php assumed instead.
///
/// `stream_read` is deliberately absent: php 8.5.6 emits NO warning for a missing `stream_read`,
/// measured with `stream_eof` present so the read is really attempted. The audit that prompted
/// this listed one; the measurement refuses it.
/// How long a path the wrapper stat cache will answer for.
///
/// The slot is a fixed buffer rather than an allocation because nothing owns the lifetime of a
/// cached path: the caller's pointer can be freed the moment it returns. A path that does not fit
/// is not cached, which costs a repeated `url_stat()` call and nothing else.
pub(crate) const US_CACHE_PATH_CAP: usize = 1024;

pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FWRITE: &str = "Warning: fwrite(): ";

/// `feof()`'s head; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FEOF: &str = "Warning: feof(): ";

/// `fstat()`'s head; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FSTAT: &str = "Warning: fstat(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FSTAT`]. php names the CALLER, and the whole-file readers
/// stat the stream themselves, so a wrapper without `stream_stat` is told which of its own
/// callers noticed. MEASURED: `file_get_contents(): K::stream_stat is not implemented!`.
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILE_GET_CONTENTS: &str =
    "Warning: file_get_contents(): ";

/// `stream_get_contents()`'s head; it stats the stream for the same reason
/// [`WRAPPER_MISSING_HOOK_HEAD_FILE_GET_CONTENTS`] gives, and only when it is reading to EOF.
/// MEASURED: `stream_get_contents(): NoStat::stream_stat is not implemented!`.
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_STREAM_GET_CONTENTS: &str =
    "Warning: stream_get_contents(): ";

/// `flock()`'s head; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FLOCK: &str = "Warning: flock(): ";

/// `fseek()`'s head; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FSEEK: &str = "Warning: fseek(): ";

/// `rewind()`'s head; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_REWIND: &str = "Warning: rewind(): ";

/// `stream_tell`'s tail; see [`WRAPPER_MISSING_HOOK_TAIL_WRITE`].
///
/// php reconciles its own position after every successful `stream_seek` by calling `stream_tell`,
/// and a wrapper that does not define one can therefore never seek: the seek is REFUSED, with this
/// warning, and the position is left where it was. MEASURED on `php -n` 8.5.6.
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_TELL: &str = "::stream_tell is not implemented!\n";

/// `stream_write`'s tail; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_WRITE: &str = "::stream_write is not implemented!\n";

/// `stream_select()`'s head; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_SELECT: &str = "Warning: stream_select(): ";

/// `stream_cast()`'s tail; see [`WRAPPER_MISSING_HOOK_TAIL_WRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_CAST: &str = "::stream_cast is not implemented!\n";

/// The second warning php raises for a stream it cannot `select()` on.
///
/// It follows the missing-hook one when the class defines no `stream_cast()`, and stands alone when
/// the method exists and answers `false`. Measured on `php -n` 8.5.6 with both shapes.
pub(crate) const SELECT_CAST_UNREPRESENTABLE: &str = concat!(
    "Warning: stream_select(): Cannot represent a stream of type user-space",
    " as a select()able descriptor\n"
);

/// The warning php raises for a `php://memory` stream handed to `stream_select()`.
///
/// A MEMORY stream has no operating-system descriptor to poll, so php refuses it outright and
/// names the TYPE in the text. `php://temp` is not refused — it is backed by a real file once it
/// exists — and neither are `data:`, a plain file, or the standard streams. Measured on `php -n`
/// 8.5.6, where the refusal leaves nothing selectable and the `ValueError` follows.
pub(crate) const SELECT_CAST_UNREPRESENTABLE_MEMORY: &str = concat!(
    "Warning: stream_select(): Cannot represent a stream of type MEMORY",
    " as a select()able descriptor\n"
);

/// What `stream_copy_to_stream()` says when the source has no way to seek AT ALL.
///
/// This one comes from `_php_stream_seek` and not from the copier: a userspace wrapper whose class
/// declares no `stream_seek` makes php mark the stream unseekable and fall through to the refusal,
/// which names whichever userland function is running — here, the copier. A wrapper that DOES
/// declare the method and answers `false` never reaches it. MEASURED on `php -n` 8.5.6 with two
/// wrappers differing in nothing else.
pub(crate) const STREAM_COPY_NO_SEEK: &str =
    "Warning: stream_copy_to_stream(): Stream does not support seeking\n";

/// The head of the copier's own refusal, before the offset it could not reach.
///
/// Unconditional on any failed seek, so it is the only line a wrapper refusing from inside its own
/// `stream_seek` produces, and the second of the two the wrapper without one produces.
pub(crate) const STREAM_COPY_SEEK_FAILED_HEAD: &str =
    "Warning: stream_copy_to_stream(): Failed to seek to position ";

/// The tail of that refusal, after the offset.
pub(crate) const STREAM_COPY_SEEK_FAILED_TAIL: &str = " in the stream\n";

/// `stream_eof`'s tail, the only one that also reports what php assumed.
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_EOF: &str =
    "::stream_eof is not implemented! Assuming EOF\n";

/// `stream_stat`'s tail; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_STAT: &str = "::stream_stat is not implemented!\n";

/// `stream_lock`'s tail; see [`WRAPPER_MISSING_HOOK_HEAD_FWRITE`].
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_LOCK: &str = "::stream_lock is not implemented!\n";

/// The path-operation heads, one per PHP function that can reach a wrapper path hook.
///
/// These differ from the stream-instance heads only in that ONE runtime helper serves them all:
/// `__rt_user_wrapper_path_op` cannot know which builtin called it, so the lowering publishes the
/// pair into `_uwmh_head`/`_uwmh_tail` before dispatching. Note `chmod`/`touch`/`chown`/`chgrp`
/// all name `stream_metadata`, not a method of their own — measured on php 8.5.6.
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_UNLINK: &str = "Warning: unlink(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_RENAME: &str = "Warning: rename(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_MKDIR: &str = "Warning: mkdir(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_RMDIR: &str = "Warning: rmdir(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_CHMOD: &str = "Warning: chmod(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_TOUCH: &str = "Warning: touch(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_CHOWN: &str = "Warning: chown(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_CHGRP: &str = "Warning: chgrp(): ";

/// `unlink`'s tail; see [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_UNLINK: &str = "::unlink is not implemented!\n";

/// `rename`'s tail; see [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_RENAME: &str = "::rename is not implemented!\n";

/// `mkdir`'s tail; see [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_MKDIR: &str = "::mkdir is not implemented!\n";

/// `rmdir`'s tail; see [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`].
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_RMDIR: &str = "::rmdir is not implemented!\n";

/// `stream_metadata`'s tail, shared by chmod/touch/chown/chgrp.
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_METADATA: &str =
    "::stream_metadata is not implemented!\n";

/// See [`WRAPPER_MISSING_HOOK_HEAD_UNLINK`]; the stat callers publish their names the same way.
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS: &str = "Warning: file_exists(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILESIZE: &str = "Warning: filesize(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_IS_FILE: &str = "Warning: is_file(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_IS_DIR: &str = "Warning: is_dir(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_IS_LINK: &str = "Warning: is_link(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_IS_READABLE: &str = "Warning: is_readable(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_IS_WRITABLE: &str = "Warning: is_writable(): ";

/// php names the ALIAS the program called, not the canonical spelling; measured on php 8.5.6.
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_IS_WRITEABLE: &str = "Warning: is_writeable(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_IS_EXECUTABLE: &str = "Warning: is_executable(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILEMTIME: &str = "Warning: filemtime(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILEATIME: &str = "Warning: fileatime(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILECTIME: &str = "Warning: filectime(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILETYPE: &str = "Warning: filetype(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILEPERMS: &str = "Warning: fileperms(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILEOWNER: &str = "Warning: fileowner(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILEGROUP: &str = "Warning: filegroup(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_FILEINODE: &str = "Warning: fileinode(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_STAT: &str = "Warning: stat(): ";

/// See [`WRAPPER_MISSING_HOOK_HEAD_FILE_EXISTS`].
pub(crate) const WRAPPER_MISSING_HOOK_HEAD_LSTAT: &str = "Warning: lstat(): ";

/// `url_stat`'s tail, shared by every builtin that stats a path through a wrapper.
pub(crate) const WRAPPER_MISSING_HOOK_TAIL_URL_STAT: &str = "::url_stat is not implemented!\n";

/// Middle of the SECOND line every VALUE reader prints whenever its stat fails.
///
/// php emits it for any failure, not just a wrapper's: an absent ordinary file gets it too, and a
/// wrapper with no `url_stat()` gets this line after the missing-hook one. The caller's own
/// `_uwmh_head_*` symbol comes first and the path follows, then a newline; `_diag_newline` already
/// serves as the tail. Measured on php 8.5.6 for `filesize`, `filemtime`, `fileatime`, `filectime`,
/// `fileperms`, `fileowner`, `filegroup`, `fileinode` and `stat`. The PREDICATES have no
/// counterpart — `file_exists()`, `is_file()`, `is_dir()`, `is_link()` and the access checks all
/// fail silently.
pub(crate) const STAT_FAILED_TAIL: &str = "stat failed for ";

/// Head of the notice `filetype()` prints for an `S_IFMT` it does not name.
///
/// Unreachable through a real filesystem, but a userspace wrapper can report any mode it likes and
/// php then says `Notice: filetype(): Unknown file type (12288)` before answering `"unknown"`. The
/// number is the mode MASKED to its file-type bits — `0030644` is reported as `12288`, not `12708`.
/// Measured on php 8.5.6.
pub(crate) const FILETYPE_UNKNOWN_HEAD: &str = "Notice: filetype(): Unknown file type (";

/// Tail of [`FILETYPE_UNKNOWN_HEAD`], after the masked mode.
pub(crate) const FILETYPE_UNKNOWN_TAIL: &str = ")\n";

/// The same line for the two readers that do NOT follow the last symlink.
///
/// php capitalizes it — `filetype(): Lstat failed for /x` and `lstat(): Lstat failed for /x` —
/// which is the only wording difference between the two families. Measured on php 8.5.6.
pub(crate) const LSTAT_FAILED_TAIL: &str = "Lstat failed for ";

/// Head of the first warning `scandir()` prints for a directory it cannot open.
///
/// php-src writes TWO lines for one failure — `scandir(/no/such): Failed to open directory: No
/// such file or directory` then `scandir(): (errno 2): No such file or directory` — and elephc
/// wrote neither, answering an empty listing in complete silence. Neither line needs a composer
/// of its own: `__rt_errno_warning` already appends `strerror` and the newline, so it serves as
/// the TAIL of both and only the beginning differs.
pub(crate) const SCANDIR_OPEN_WARNING_HEAD: &str = "Warning: scandir(";

/// The text between the path and the reason in [`SCANDIR_OPEN_WARNING_HEAD`]'s line.
pub(crate) const SCANDIR_OPEN_WARNING_MIDDLE: &str = "): Failed to open directory: ";

/// The wording php prints when a filesystem PATH OPERATION fails.
///
/// MEASURED on php 8.5.6, and the shapes do not agree with each other: `unlink()` and `rmdir()`
/// name the path in the parentheses, `opendir()` adds a sentence of its own before the reason,
/// and `mkdir()`, `chmod()` and `touch()` leave the parentheses EMPTY — `touch()` putting the
/// path in the middle of a sentence instead. elephc printed none of these lines at all.
pub(crate) const UNLINK_WARNING_HEAD: &str = "Warning: unlink(";

/// See [`UNLINK_WARNING_HEAD`].
pub(crate) const RMDIR_WARNING_HEAD: &str = "Warning: rmdir(";

/// See [`UNLINK_WARNING_HEAD`]. `rename()` names BOTH paths, comma-separated, with no space.
pub(crate) const RENAME_WARNING_HEAD: &str = "Warning: rename(";

/// See [`UNLINK_WARNING_HEAD`]. The parentheses stay empty even though a path was passed.
pub(crate) const MKDIR_WARNING_HEAD: &str = "Warning: mkdir(): ";

/// See [`UNLINK_WARNING_HEAD`]. The parentheses stay empty even though a path was passed.
pub(crate) const CHMOD_WARNING_HEAD: &str = "Warning: chmod(): ";

/// See [`UNLINK_WARNING_HEAD`]. The ownership builtins keep the parentheses empty too, and each
/// one names ITSELF — MEASURED on `php -n` 8.5.6, `chgrp()` on a missing path reports
/// `Warning: chgrp(): No such file or directory`, not the `chown()` the syscall belongs to.
/// elephc printed none of these lines at all: a failing ownership change answered `false` in
/// silence.
pub(crate) const CHOWN_WARNING_HEAD: &str = "Warning: chown(): ";

/// See [`CHOWN_WARNING_HEAD`].
pub(crate) const CHGRP_WARNING_HEAD: &str = "Warning: chgrp(): ";

/// See [`CHOWN_WARNING_HEAD`].
pub(crate) const LCHOWN_WARNING_HEAD: &str = "Warning: lchown(): ";

/// See [`CHOWN_WARNING_HEAD`].
pub(crate) const LCHGRP_WARNING_HEAD: &str = "Warning: lchgrp(): ";

/// The head of the line php prints when the PRINCIPAL NAME does not resolve.
///
/// A different failure from the syscall's, and worded differently: no `errno` is involved, the
/// name itself is quoted, and php says `uid` for an owner and `gid` for a group. MEASURED:
/// `Warning: chown(): Unable to find uid for nosuchuser`.
pub(crate) const CHOWN_UNKNOWN_PRINCIPAL_HEAD: &str = "Warning: chown(): Unable to find uid for ";

/// See [`CHOWN_UNKNOWN_PRINCIPAL_HEAD`].
pub(crate) const CHGRP_UNKNOWN_PRINCIPAL_HEAD: &str = "Warning: chgrp(): Unable to find gid for ";

/// See [`CHOWN_UNKNOWN_PRINCIPAL_HEAD`].
pub(crate) const LCHOWN_UNKNOWN_PRINCIPAL_HEAD: &str =
    "Warning: lchown(): Unable to find uid for ";

/// See [`CHOWN_UNKNOWN_PRINCIPAL_HEAD`].
pub(crate) const LCHGRP_UNKNOWN_PRINCIPAL_HEAD: &str =
    "Warning: lchgrp(): Unable to find gid for ";

/// See [`UNLINK_WARNING_HEAD`]. The path sits inside a sentence rather than in parentheses.
pub(crate) const TOUCH_WARNING_HEAD: &str = "Warning: touch(): Unable to create file ";

/// The text between `touch()`'s path and its reason.
pub(crate) const TOUCH_WARNING_MIDDLE: &str = " because ";

/// See [`UNLINK_WARNING_HEAD`]. `opendir()` words its failure like `scandir()`.
pub(crate) const OPENDIR_WARNING_HEAD: &str = "Warning: opendir(";

/// The text between a path and its reason in the ordinary `name(path): reason` shape.
pub(crate) const PATH_WARNING_MIDDLE: &str = "): ";

/// `touch()`'s head when the path was a `file://` URL.
///
/// php reaches the plain-files wrapper's METADATA hook for a URL, and that hook's diagnostic
/// names the path in the parentheses where the direct call names nothing. MEASURED:
/// `Warning: touch(/no/such/x.txt): Unable to create file /no/such/x.txt because No such file or
/// directory` — the path really does appear twice.
pub(crate) const TOUCH_URL_WARNING_HEAD: &str = "Warning: touch(";

/// The text between `touch()`'s parenthesised path and the sentence that names it again.
pub(crate) const TOUCH_URL_WARNING_MIDDLE: &str = "): Unable to create file ";

/// `chmod()`'s head when the path was a `file://` URL. See [`TOUCH_URL_WARNING_HEAD`].
pub(crate) const CHMOD_URL_WARNING_HEAD: &str = "Warning: chmod(";

/// The wrapper hook's own wording, which the direct call does not use. MEASURED:
/// `Warning: chmod(/no/such/x.txt): Operation failed: No such file or directory`.
pub(crate) const CHMOD_URL_WARNING_MIDDLE: &str = "): Operation failed: ";

/// The separator between `rename()`'s two paths — no space, measured.
pub(crate) const RENAME_WARNING_SEPARATOR: &str = ",";

/// Head of the second warning, which names the error NUMBER rather than the path.
pub(crate) const SCANDIR_ERRNO_WARNING_HEAD: &str = "Warning: scandir(): (errno ";

/// The text between the error number and the reason.
pub(crate) const SCANDIR_ERRNO_WARNING_MIDDLE: &str = "): ";

/// The text between the caller's name and the scheme in PHP's unknown-wrapper warning.
///
/// PHP emits TWO warnings for `fopen("bogus://x", "r")`: this one naming the scheme, then the
/// ordinary failed-open warning. elephc emitted only the second, which reports "No such file or
/// directory" — true of the path but silent about the actual cause, a wrapper that is not there.
pub(crate) const UNKNOWN_WRAPPER_HEAD: &str = "Warning: ";
/// The fixed text between the caller's name and the scheme.
pub(crate) const UNKNOWN_WRAPPER_MIDDLE: &str = "(): Unable to find the wrapper \"";
/// The fixed text after the scheme, copied from php-src verbatim.
/// The halves of php's refusal to unregister a protocol nobody registered, either side of the
/// name: `Warning: stream_wrapper_unregister(): Unable to unregister protocol nope://`.
///
/// MEASURED on `php -n` 8.5.6. It is a warning, not a silent false, and `@` suppresses it like any
/// other.
pub(crate) const STREAM_WRAPPER_UNREGISTER_HEAD: &str =
    "Warning: stream_wrapper_unregister(): Unable to unregister protocol ";

/// The tail of that refusal; see [`STREAM_WRAPPER_UNREGISTER_HEAD`].
pub(crate) const STREAM_WRAPPER_UNREGISTER_TAIL: &str = "://\n";

pub(crate) const UNKNOWN_WRAPPER_TAIL: &str =
    "\" - did you forget to enable it when you configured PHP?\n";

pub(crate) const OB_NTC_NO_END_FLUSH: &str =
    "Notice: ob_end_flush(): Failed to delete and flush buffer. No buffer to delete or flush\n";
/// ob_get_flush() no-buffer notice line.
pub(crate) const OB_NTC_NO_GET_FLUSH: &str =
    "Notice: ob_get_flush(): Failed to delete and flush buffer. No buffer to delete or flush\n";
/// ob_end_clean() no-buffer notice line.
pub(crate) const OB_NTC_NO_END_CLEAN: &str =
    "Notice: ob_end_clean(): Failed to delete buffer. No buffer to delete\n";
/// ob_flush() no-buffer notice line.
pub(crate) const OB_NTC_NO_FLUSH: &str =
    "Notice: ob_flush(): Failed to flush buffer. No buffer to flush\n";
/// ob_clean() no-buffer notice line.
pub(crate) const OB_NTC_NO_CLEAN: &str =
    "Notice: ob_clean(): Failed to delete buffer. No buffer to delete\n";
/// ob_clean() flags-gated notice prefix (completed with "NAME (LEVEL)\n").
pub(crate) const OB_NTC_G_CLEAN: &str = "Notice: ob_clean(): Failed to delete buffer of ";
/// ob_flush() flags-gated notice prefix.
pub(crate) const OB_NTC_G_FLUSH: &str = "Notice: ob_flush(): Failed to flush buffer of ";
/// ob_end_clean() flags-gated notice prefix.
pub(crate) const OB_NTC_G_END_CLEAN: &str =
    "Notice: ob_end_clean(): Failed to discard buffer of ";
/// ob_get_clean() flags-gated notice prefix.
pub(crate) const OB_NTC_G_GET_CLEAN: &str =
    "Notice: ob_get_clean(): Failed to discard buffer of ";
/// ob_end_flush() flags-gated notice prefix.
pub(crate) const OB_NTC_G_END_FLUSH: &str =
    "Notice: ob_end_flush(): Failed to send buffer of ";
/// ob_get_flush() flags-gated notice prefix.
pub(crate) const OB_NTC_G_GET_FLUSH: &str =
    "Notice: ob_get_flush(): Failed to send buffer of ";
/// ob_start() invalid-callback warning prefix (completed with the name + suffix).
/// `curl_setopt()`'s unsupported-option warning, split around the decimal option number.
///
/// An unsupported option must answer
/// `false` AND say so, rather than returning an inert `true`. The wording follows PHP's
/// own `php_error_docref(..., E_WARNING, ...)` rendering (`Warning: <fn>(): <message>`),
/// and the split lets `__rt_curl_warn_unsupported_option` format the option number
/// between the two halves through the shared `__rt_itoa`.
pub(crate) const CURL_SETOPT_UNSUPPORTED_PREFIX: &str = "Warning: curl_setopt(): Option ";
/// The `curl_multi_setopt()` half of the same warning. A SEPARATE STRING because PHP names
/// the function that refused the option, and the multi interface is a different function.
pub(crate) const CURL_MULTI_SETOPT_UNSUPPORTED_PREFIX: &str =
    "Warning: curl_multi_setopt(): Option ";
/// The half after the option number. See [`CURL_SETOPT_UNSUPPORTED_PREFIX`].
pub(crate) const CURL_SETOPT_UNSUPPORTED_SUFFIX: &str =
    " is not supported by this build\n";

pub(crate) const OB_WARN_BAD_CALLBACK_PREFIX: &str = "Warning: ob_start(): function \"";
/// ob_start() invalid-callback warning suffix.
pub(crate) const OB_WARN_BAD_CALLBACK_SUFFIX: &str =
    "\" not found or invalid function name\n";
/// ob_start() invalid-callback warning for non-string, non-callable values.
pub(crate) const OB_WARN_BAD_CALLBACK_GENERIC: &str =
    "Warning: ob_start(): no array or string given\n";
/// ob_start() failed-create notice line.
pub(crate) const OB_NTC_CREATE_FAIL: &str = "Notice: ob_start(): Failed to create buffer\n";
/// ob_start()-inside-a-handler fatal line.
pub(crate) const OB_FATAL_IN_HANDLER: &str =
    "Fatal error: ob_start(): Cannot use output buffering in output buffering display handlers\n";
/// PHP's default output-handler display name.
pub(crate) const OB_DEFAULT_HANDLER_NAME: &str = "default output handler";
/// PHP's closure / first-class-callable handler display name.
pub(crate) const OB_CLOSURE_INVOKE_NAME: &str = "Closure::__invoke";

pub(crate) const DIRNAME_LEVELS_MSG: &str =
    "Fatal error: dirname(): Argument #2 ($levels) must be greater than or equal to 1\n";
/// Fatal error message written by `__rt_stack_overflow` when a function prologue finds the
/// stack pointer below `_stack_limit`. PHP 8.3+ reports the same condition as
/// `Fatal error: Uncaught Error: Maximum call stack size of N bytes
/// (zend.max_allowed_stack_size - zend.reserved_stack_size) reached. Infinite recursion?`;
/// elephc has no per-call-site context in the fatal path (it is entered with almost no
/// stack left), so it reports the same condition without the byte count and location.
pub(crate) const STACK_OVERFLOW_MSG: &str =
    "Fatal error: Maximum call stack size reached. Infinite recursion?\n";
/// The warning php raises at EVERY array-to-string conversion, whatever produced it: `echo`,
/// `print`, `.`, `.=`, interpolation, a heredoc, `(string)`, `strval()`, a `%s` conversion. The
/// value is always the literal `Array`, so the two travel together and this message is published
/// once for the whole program rather than interned per conversion site.
///
/// The only array-meets-string case php leaves silent is a loose COMPARISON, which never converts.
pub(crate) const ARRAY_TO_STRING_MSG: &str = "Warning: Array to string conversion\n";
/// php's own refusal when `copy()` is handed a DIRECTORY to read.
///
/// php-src checks this before it opens anything, so the destination is never touched and the
/// answer is `false`. Without the check the read failed instead — and on macOS a failed `read(2)`
/// answers the errno in the result register, so `EISDIR` became a 21-byte string of uninitialised
/// heap that `copy()` cheerfully wrote out and called a success.
/// Head of php's Notice when a read fails after the file was successfully opened.
///
/// php names the function, the number of bytes it ASKED for — the size `stat` reported — the
/// `errno`, and the system's own text for it. Reading a directory is how a program meets this
/// one: `Notice: file_get_contents(): Read of 156864 bytes failed with errno=21 Is a directory`.
pub(crate) const FGC_READ_FAILED_HEAD: &str = "Notice: file_get_contents(): Read of ";

/// Middle of that Notice, between the byte count and the error number.
pub(crate) const FGC_READ_FAILED_MID: &str = " bytes failed with errno=";

/// php's own refusal when `copy()` is handed a DIRECTORY to read.
pub(crate) const COPY_SOURCE_IS_DIR_MSG: &str =
    "Warning: copy(): The first argument to copy() function cannot be a directory\n";
/// Fatal error message when an array allocation request cannot be sized safely,
/// i.e. `capacity * elem_size` does not fit in the machine word. PHP reports the
/// same class of failure as a `ValueError` naming the offending argument
/// (`array_fill(): Argument #2 ($count) is too large`); elephc's runtime has no
/// per-call-site context inside `__rt_array_new`, so it reports the shared cause.
pub(crate) const ARRAY_ALLOC_SIZE_MSG: &str =
    "Fatal error: requested array size exceeds the maximum allowed array size\n";
/// Fatal error message when `range()` cannot represent the requested interval,
/// because `end - start + 1` overflows a signed 64-bit element count. Matches
/// PHP's `ValueError: The supplied range exceeds the maximum array size`.
pub(crate) const RANGE_SIZE_MSG: &str =
    "Fatal error: The supplied range exceeds the maximum array size\n";
/// Fatal error message when `buffer_new<T>()` receives a negative length or a
/// length whose `len * stride` payload size does not fit in the machine word.
/// `buffer_new` is an elephc extension with no PHP equivalent, so the wording is
/// elephc's own rather than a PHP parity string.
pub(crate) const BUFFER_ALLOC_SIZE_MSG: &str =
    "Fatal error: buffer_new() length is negative or exceeds the maximum buffer size\n";
/// Fatal error message when a runtime string producer is asked for a result whose byte
/// count cannot be allocated: either the size computation itself wrapped (`str_repeat()`'s
/// `len * times`, an encoder's `2 * len` / `3 * len` expansion) or the requested size
/// exceeds the configured heap capacity. PHP reports the same class of failure as
/// `Fatal error: Possible integer overflow in memory allocation (...)`; elephc's runtime
/// has no per-call-site operand context, so it reports the shared cause.
pub(crate) const ALLOC_OVERFLOW_MSG: &str =
    "Fatal error: Possible integer overflow in memory allocation\n";
/// Fatal error message when `str_repeat()` receives a `$times` argument less than 0.
pub(crate) const STR_REPEAT_TIMES_MSG: &str =
    "Fatal error: str_repeat(): Argument #2 ($times) must be greater than or equal to 0\n";
/// Prefix for a catchable TypeError naming a non-array `unserialize()` options argument.
pub(crate) const UNSER_OPTIONS_TYPE_PREFIX: &str =
    "unserialize(): Argument #2 ($options) must be of type array, ";
/// Prefix for a catchable TypeError naming an invalid `allowed_classes` policy value.
pub(crate) const UNSER_ALLOWED_CLASSES_POLICY_PREFIX: &str =
    "unserialize(): Option \"allowed_classes\" must be of type array|bool, ";
/// Prefix for a catchable TypeError naming an invalid `allowed_classes` list entry.
pub(crate) const UNSER_ALLOWED_CLASSES_ENTRY_PREFIX: &str =
    "unserialize(): Option \"allowed_classes\" must be an array of class names, ";
/// Prefix for PHP's catchable Error when an object is indexed like an array.
///
/// `$o["k"]` on anything that is not `ArrayAccess` stops the program in PHP — including a plain
/// `stdClass`, and including the quiet contexts `isset`, `??` and `empty`, all measured against
/// 8.5. The class is only known at run time when the value arrives boxed, so the message is
/// composed from these two fragments around the name.
pub(crate) const OBJECT_NOT_ARRAY_PREFIX: &str = "Cannot use object of type ";
/// Suffix for PHP's catchable Error when an object is indexed like an array.
pub(crate) const OBJECT_NOT_ARRAY_SUFFIX: &str = " as array";
/// Prefix for PHP's catchable object-to-string conversion Error in an allowed-class list.
pub(crate) const UNSER_OBJECT_STRING_ERROR_PREFIX: &str = "Object of class ";
/// Suffix for PHP's catchable object-to-string conversion Error in an allowed-class list.
pub(crate) const UNSER_OBJECT_STRING_ERROR_SUFFIX: &str =
    " could not be converted to string";
/// PHP warning emitted when a printf-family `%s` conversion stringifies an array.
pub(crate) const SPRINTF_ARRAY_TO_STRING_WARNING: &str =
    "Warning: Array to string conversion\n";
/// Prefix for printf-family object-to-number coercion warnings.
pub(crate) const SPRINTF_OBJECT_NUMERIC_WARNING_PREFIX: &str =
    "Warning: Object of class ";
/// Suffix for printf-family object-to-integer coercion warnings.
pub(crate) const SPRINTF_OBJECT_TO_INT_WARNING_SUFFIX: &str =
    " could not be converted to int\n";
/// Suffix for printf-family object-to-float coercion warnings.
pub(crate) const SPRINTF_OBJECT_TO_FLOAT_WARNING_SUFFIX: &str =
    " could not be converted to float\n";
/// Suffix shared by PHP's runtime unserialize TypeError diagnostics.
pub(crate) const UNSER_TYPE_GIVEN_SUFFIX: &str = " given";
/// Fatal error message when a `printf`-family conversion requests a field width outside
/// PHP's accepted range. PHP raises `ValueError: Width must be between 0 and 2147483647`;
/// elephc has no catchable-error path inside `__rt_sprintf`, so it reports the same text
/// as a controlled fatal instead of writing past the conversion buffer.
pub(crate) const SPRINTF_WIDTH_MSG: &str =
    "Fatal error: Uncaught ValueError: Width must be between 0 and 2147483647\n";
/// Fatal error message when a `printf`-family conversion would write past the shared
/// 64 KiB `_concat_buf` result arena. PHP grows its result buffer on the heap; elephc's
/// formatted results live in the fixed concat arena, so an oversized result is reported
/// instead of overrunning the arena.
pub(crate) const SPRINTF_OVERFLOW_MSG: &str =
    "Fatal error: sprintf(): formatted result exceeds the 65536-byte string buffer\n";
/// Fatal error message when a `printf`-family format string consumes more arguments than
/// were supplied. PHP raises `ArgumentCountError`; elephc reports the same class of error
/// as a controlled fatal because the alternative is reading past the pushed argument records.
pub(crate) const SPRINTF_ARGCOUNT_MSG: &str =
    "Fatal error: Uncaught ArgumentCountError: sprintf(): too few arguments\n";
/// Fatal error message when a `printf`-family format string uses a conversion character
/// PHP does not define. The runtime never forwards an unrecognized conversion to libc
/// `snprintf` (that would expose `%n` and friends), so it reports PHP's `ValueError` instead.
pub(crate) const SPRINTF_UNKNOWN_SPEC_MSG: &str =
    "Fatal error: Uncaught ValueError: Unknown format specifier\n";
/// Catchable `\ValueError` message when `iconv_strpos()` receives an out-of-range offset.
pub(crate) const ICONV_STRPOS_OFFSET_MSG: &str =
    "iconv_strpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)";
/// Catchable `\ValueError` message when `hash()` receives an unknown algorithm name.
pub(crate) const HASH_UNKNOWN_ALGO_MSG: &str =
    "hash(): Argument #1 ($algo) must be a valid hashing algorithm";
/// Catchable `\ValueError` message when `hash_init()` receives an unknown algorithm name.
pub(crate) const HASH_INIT_UNKNOWN_ALGO_MSG: &str =
    "hash_init(): Argument #1 ($algo) must be a valid hashing algorithm";
/// Catchable `\ValueError` message when `hash_hmac()` receives an unknown algorithm
/// name or a non-cryptographic checksum (PHP rejects HMAC over crc32/adler/fnv/joaat).
pub(crate) const HASH_HMAC_UNKNOWN_ALGO_MSG: &str =
    "hash_hmac(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm";
/// Catchable `\TypeError` message when `hash_update()` is handed a HashContext that
/// `hash_final()` already consumed. Captured verbatim from PHP 8.5.6
/// (`php -d xdebug.mode=off`); PHP words all three as one sentence naming the callee.
pub(crate) const HASH_UPDATE_FINALIZED_CTX_MSG: &str =
    "hash_update(): Argument #1 ($context) must be a valid, non-finalized HashContext";
/// Catchable `\TypeError` message for a second `hash_final()` on the same context.
pub(crate) const HASH_FINAL_FINALIZED_CTX_MSG: &str =
    "hash_final(): Argument #1 ($context) must be a valid, non-finalized HashContext";
/// Catchable `\TypeError` message for `hash_copy()` of an already-finalized context.
pub(crate) const HASH_COPY_FINALIZED_CTX_MSG: &str =
    "hash_copy(): Argument #1 ($context) must be a valid, non-finalized HashContext";
/// Catchable `\ValueError` message when `mb_strlen()` receives an unknown encoding name.
pub(crate) const MB_STRLEN_UNKNOWN_ENCODING_MSG: &str =
    "mb_strlen(): Argument #2 ($encoding) must be a valid encoding";
