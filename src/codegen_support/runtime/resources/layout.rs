//! Purpose:
//! Defines the target-independent memory layout and discriminants for opaque PHP resources.
//! The emitted AArch64 and x86_64 helpers share these exact offsets.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::registry`.
//! - `crate::codegen_support::runtime::resources::stream`.
//!
//! Key details:
//! - Resource handles pack `generation << 32 | (slot_index + 1)`.
//! - Every registry slot is 64 bytes and every stream state is 320 bytes.
//! - Context states are 32-byte owned aggregates whose child references must
//!   be released before the state allocation itself.

/// Number of bits occupied by the one-based registry slot in an opaque handle.
pub(crate) const HANDLE_INDEX_BITS: u32 = 32;

/// Initial number of registry slots, including the three persistent standard streams.
pub(crate) const INITIAL_REGISTRY_CAPACITY: u64 = 8;

/// Size in bytes of one resource-registry slot.
pub(crate) const RESOURCE_SLOT_SIZE: u64 = 64;

/// Byte offset of the generation word in a registry slot.
pub(crate) const SLOT_GENERATION_OFFSET: i64 = 0;

/// Byte offset of the resource-kind word in a registry slot.
pub(crate) const SLOT_KIND_OFFSET: i64 = 8;

/// Byte offset of the lifecycle-status word in a registry slot.
pub(crate) const SLOT_STATUS_OFFSET: i64 = 16;

/// Byte offset of the strong-reference count in a registry slot.
pub(crate) const SLOT_REFS_OFFSET: i64 = 24;

/// Byte offset of the PHP-visible resource id in a registry slot.
pub(crate) const SLOT_PHP_ID_OFFSET: i64 = 32;

/// Byte offset of the stable resource-state pointer in a registry slot.
pub(crate) const SLOT_STATE_PTR_OFFSET: i64 = 40;

/// Byte offset of the one-based next-free index in a registry slot.
///
/// Live slots reuse this word as their request epoch because a slot cannot be
/// both live and linked into the free list.
pub(crate) const SLOT_NEXT_FREE_OFFSET: i64 = 48;

/// Byte offset of the request epoch that owns a live registry slot.
pub(crate) const SLOT_REQUEST_EPOCH_OFFSET: i64 = SLOT_NEXT_FREE_OFFSET;

/// Byte offset of the resource ownership and persistence flags in a registry slot.
pub(crate) const SLOT_FLAGS_OFFSET: i64 = 56;

/// Empty/free registry-slot kind.
pub(crate) const RESOURCE_KIND_FREE: u64 = 0;

/// Stream registry-slot kind.
pub(crate) const RESOURCE_KIND_STREAM: u64 = 1;

/// Stream-context registry-slot kind.
pub(crate) const RESOURCE_KIND_CONTEXT: u64 = 2;

/// Stream-filter registry-slot kind.
///
/// A filter is a PHP-visible resource with its own lifetime: `stream_filter_append()`
/// hands one back, `stream_filter_remove()` closes it, and closing the owning stream
/// closes every filter still attached. Modelling it as a registry resource is what
/// lets `is_resource()` observe that invalidation.
pub(crate) const RESOURCE_KIND_FILTER: u64 = 3;

/// Live resource lifecycle state.
pub(crate) const RESOURCE_STATUS_LIVE: u64 = 1;

/// Closing resource lifecycle state, published before re-entrant cleanup.
pub(crate) const RESOURCE_STATUS_CLOSING: u64 = 2;

/// Closed resource lifecycle state.
pub(crate) const RESOURCE_STATUS_CLOSED: u64 = 3;

/// Marks registry slots whose state storage is owned by the registry.
pub(crate) const RESOURCE_FLAG_OWNS_STATE: u64 = 1;

/// Marks process-persistent registry slots such as STDIN, STDOUT, and STDERR.
pub(crate) const RESOURCE_FLAG_PERSISTENT: u64 = 2;

/// Reference-count sentinel used by persistent standard-stream slots.
pub(crate) const RESOURCE_REFS_IMMORTAL: i64 = -1;

/// Number of persistent standard-stream registry slots.
pub(crate) const STANDARD_STREAM_COUNT: u64 = 3;

/// Size in bytes of one stable stream-state allocation.
pub(crate) const STREAM_STATE_SIZE: u64 = 320;

/// Byte offset of the stream backend kind.
pub(crate) const STREAM_BACKEND_KIND_OFFSET: i64 = 0;

/// Byte offset of the PHP stream-wrapper identity.
pub(crate) const STREAM_WRAPPER_ID_OFFSET: i64 = 8;

/// Byte offset of the optional OS descriptor.
pub(crate) const STREAM_FD_OFFSET: i64 = 16;

/// Byte offset of the owned PHP-visible stream URI pointer.
pub(crate) const STREAM_URI_PTR_OFFSET: i64 = 24;

/// Byte offset of the PHP-visible stream URI byte length.
pub(crate) const STREAM_URI_LEN_OFFSET: i64 = 32;

/// Byte offset of the owned transport-host pointer used by TLS defaults.
pub(crate) const STREAM_CONNECT_HOST_PTR_OFFSET: i64 = 40;

/// Byte offset of the transport-host byte length used by TLS defaults.
pub(crate) const STREAM_CONNECT_HOST_LEN_OFFSET: i64 = 48;

/// Byte offset of the PHP-visible end-of-file state.
pub(crate) const STREAM_EOF_OFFSET: i64 = 56;

/// Byte offset reserved for backend-specific auxiliary state.
pub(crate) const STREAM_BACKEND_AUX_OFFSET: i64 = 64;

/// Byte offset of the retained stream-context registry handle.
pub(crate) const STREAM_CONTEXT_HANDLE_OFFSET: i64 = 80;

/// Byte offset of the read-direction filter chain head (filter handle, 0 = no filters).
///
/// The chain is a doubly linked list of filter resources, so `stream_filter_remove()`
/// can unlink a middle node without disturbing its neighbours. The previous design
/// stored two filter-id bytes per descriptor, which could not represent a third
/// filter at all, let alone removal from the middle.
pub(crate) const STREAM_READ_FILTER_HEAD_OFFSET: i64 = 88;

/// Byte offset of the write-direction filter chain head (filter handle, 0 = no filters).
pub(crate) const STREAM_WRITE_FILTER_HEAD_OFFSET: i64 = 96;

/// Byte offset of the attached TLS session handle (0 = plain, unencrypted transport).
///
/// This used to live in `_tls_sessions`, a fixed 2048-byte table indexed by raw
/// descriptor. That table capped a program at 256 concurrent TLS streams, and
/// because the kernel reuses descriptor numbers, a freshly opened socket
/// inherited the session of a closed one. Keeping the session on the StreamState
/// ties it to the generation-checked handle instead, so neither limit applies.
pub(crate) const STREAM_TLS_SESSION_OFFSET: i64 = 104;

/// Whether this stream has been WRITTEN since it was opened or last flushed.
///
/// php flushes a userspace wrapper on close only when there is something to flush: MEASURED on
/// `php -n` 8.5.6, `fclose()` calls `stream_flush()` after a write and not otherwise, and an
/// explicit `fflush()` clears the debt so the close does not flush again. It is not the MODE —
/// a `w` stream that was never written is not flushed either.
pub(crate) const STREAM_WRITTEN_SINCE_FLUSH_OFFSET: i64 = 112;

/// Byte offset of the PHP-visible stream chunk size, or zero for the 8192-byte default.
pub(crate) const STREAM_CHUNK_SIZE_OFFSET: i64 = 144;

/// Byte offset of the filtered-read buffer: heap pointer, or 0 before the first filtered read.
///
/// A read filter does not produce one output byte per input byte, so what it emits cannot be
/// handed straight back to `fread($h, $n)`: PHP caps the result at `$n` and keeps the remainder
/// on the stream for the next read. This buffer is that remainder. Without it an expanding
/// filter returned its whole output at once — more bytes than the caller asked for — and a
/// filter that answered `PSFS_FEED_ME` had no place to accumulate, which is why the read path
/// passed its raw, unfiltered input through instead.
pub(crate) const STREAM_FILTERED_BUF_PTR_OFFSET: i64 = 152;

/// Byte offset of the number of filtered bytes currently held in the buffer.
pub(crate) const STREAM_FILTERED_BUF_LEN_OFFSET: i64 = 160;

/// Byte offset of the filtered buffer's allocated capacity.
pub(crate) const STREAM_FILTERED_BUF_CAP_OFFSET: i64 = 168;

/// Byte offset of the read cursor into the filtered buffer.
pub(crate) const STREAM_FILTERED_BUF_POS_OFFSET: i64 = 176;

/// Byte offset of the "closing flush already ran" marker.
///
/// PHP gives a read filter one final `filter(..., $closing = true)` dispatch when the stream
/// ends, and whatever that dispatch emits reaches the reader. The marker keeps it to exactly one
/// dispatch however many times the caller reads past EOF.
pub(crate) const STREAM_FILTERED_FLUSHED_OFFSET: i64 = 184;

/// Byte offset of the PHP mode string `stream_get_meta_data()` reports, or 0 when unrecorded.
///
/// PHP echoes the mode the caller passed — `rb` stays `rb`, `a` stays `a` — while `php://memory`
/// and `php://temp` report a spelling of php-src's own. Deriving it from the descriptor's
/// `F_GETFL` access bits cannot produce either: it collapses `a` to `w`, `w+` to `r+`, and drops
/// the `b`. The bytes are persisted into owned storage, like the URI above and released beside it,
/// so a mode built at run time cannot dangle.
pub(crate) const STREAM_MODE_PTR_OFFSET: i64 = 192;

/// Byte offset of the recorded mode string's length.
pub(crate) const STREAM_MODE_LEN_OFFSET: i64 = 200;

/// Byte offset of the socket transport a stream was opened on, or 0 when it is not a socket.
///
/// `stream_get_meta_data()['stream_type']` names the transport, and the name is not derivable
/// from the descriptor: a TCP, UDP, Unix and socketpair endpoint are all non-seekable sockets,
/// and php-src calls them `tcp_socket/ssl`, `udp_socket`, `unix_socket` and `generic_socket`.
pub(crate) const STREAM_TRANSPORT_OFFSET: i64 = 208;

/// Byte offset of the bytes `O_APPEND` has moved the descriptor past, for an append stream.
///
/// PHP reports a position it maintains itself: `fopen($f, 'a')` on a four-byte file answers `1`
/// after writing one byte, not `5`. The descriptor really is at 5, because `O_APPEND` puts every
/// write at the end, so `ftell()` answers `lseek(SEEK_CUR) - this`. Accumulating what was jumped
/// over — rather than tracking the position itself — is what keeps reads out of it: a read moves
/// the descriptor and PHP's position by the same amount, so it leaves this untouched. `fseek()`
/// clears it, since a seek puts both back in agreement.
pub(crate) const STREAM_APPEND_SKIP_OFFSET: i64 = 216;

/// Byte offset of the position PHP keeps for a userspace-wrapper stream.
///
/// php-src has no tell op for these: `main/streams/userspace.c` calls `stream_tell` only from
/// inside `php_userstreamop_seek`, to reconcile after a seek. The position itself is PHP's own,
/// advanced by the bytes each read and write moved — which is why `ftell()` answers `7` after
/// reading seven bytes even when the wrapper's `stream_tell()` is written to return something
/// else entirely. Asking the method on every `ftell()`, as this used to, reports whatever the
/// wrapper feels like rather than where the stream is.
/// php's default stream chunk size, the amount a read asks its source for when nothing has
/// called `stream_set_chunk_size()`.
///
/// `php_stream_set_chunk_size()` reports it: `stream_set_chunk_size($h, 100)` on a fresh stream
/// answers `int(8192)`. It is observable through a user wrapper too — that is the `$count` its
/// `stream_read()` receives — which is how the read-loop fallback being 4096 showed up: `fgets()`
/// asked for 4096 where php asks for 8192.
pub(crate) const STREAM_DEFAULT_CHUNK_SIZE: i64 = 8192;

pub(crate) const STREAM_WRAPPER_POS_OFFSET: i64 = 224;

/// Byte offset of the position PHP reports for a stream whose reads go through a read filter.
///
/// php maintains `stream->position` itself and advances it by the bytes each read RETURNED TO THE
/// CALLER — never by the bytes it pulled from the descriptor. A filtered read pulls ahead: the
/// buffered wrapper reads whole 8192-byte chunks so the filter has something to work on, caps the
/// result at what `fread($h, $n)` asked for, and parks the rest. Reporting `lseek(SEEK_CUR)` there
/// answered where the READ-AHEAD stopped: two `fread($f, 3)` through `string.toupper` on a 26-byte
/// file answered `26` and `26` where php answers `3` and `6`. An expanding filter settles what the
/// number counts: through `convert.base64-encode`, two `fread($f, 4)` answer `4` and `8` — the
/// FILTERED bytes handed out, not the source bytes consumed to make them.
///
/// The field is only authoritative once the buffered path has actually run, which
/// `STREAM_FILTERED_BUF_PTR_OFFSET` reports: a filtered stream read only through `fgets()` never
/// engages the buffer, and its descriptor offset is still the right answer.
pub(crate) const STREAM_FILTERED_POS_OFFSET: i64 = 232;

/// Transport value for a TCP endpoint, which php-src names after the ssl-capable transport.
pub(crate) const STREAM_TRANSPORT_TCP: u64 = 1;

/// Transport value for a UDP endpoint.
pub(crate) const STREAM_TRANSPORT_UDP: u64 = 2;

/// Transport value for a Unix-domain endpoint, stream or datagram.
pub(crate) const STREAM_TRANSPORT_UNIX: u64 = 3;

/// Transport value for a socket pair, which belongs to no named transport.
pub(crate) const STREAM_TRANSPORT_GENERIC: u64 = 4;

/// Byte offset of stream ownership flags.
/// Byte offset of the heap block holding bytes a read consumed but must not hand back yet.
///
/// php's `stream_get_line()` answers `false` when it finds neither the delimiter nor the length cap
/// and the stream is not at EOF, and the bytes it read stay ON the stream: measured on `php -n`
/// 8.5.6 over a non-blocking socket pair, "abc" with no newline answers `false`, and once "def\n"
/// arrives the next call answers "abcdef". elephc consumed the "abc" and handed it back as a line
/// php never breaks. Zero when nothing is held, which is every stream that has not hit that case.
pub(crate) const STREAM_PENDING_PTR_OFFSET: i64 = 240;

/// Byte offset of the number of bytes in that block.
pub(crate) const STREAM_PENDING_LEN_OFFSET: i64 = 248;

/// Byte offset of the read cursor into that block.
pub(crate) const STREAM_PENDING_POS_OFFSET: i64 = 256;

pub(crate) const STREAM_OWNERSHIP_FLAGS_OFFSET: i64 = 296;

/// Marks a stream instance whose wrapper definition declared `STREAM_IS_URL`.
pub(crate) const STREAM_STATE_FLAG_IS_URL: u64 = 1 << 8;

/// Marks a stream whose recorded URI must be unlinked when its descriptor closes.
///
/// This is what `tmpfile()` is: php keeps the file it created LINKED for as long as the handle
/// lives — `file_exists()`, `filesize()` and a second `fopen()` all reach it — and removes it on
/// close. Unlinking at creation, which is the right trick for an anonymous scratch file, makes the
/// URI php publishes name nothing.
pub(crate) const STREAM_STATE_FLAG_UNLINK_ON_CLOSE: u64 = 1 << 9;

/// Backend kind for a directly owned OS descriptor.
pub(crate) const STREAM_BACKEND_FD: u64 = 1;

/// Backend kind for a userspace stream-wrapper instance.
pub(crate) const STREAM_BACKEND_USER_WRAPPER: u64 = 2;

/// Backend kind for a process pipe owned by `popen`.
pub(crate) const STREAM_BACKEND_POPEN: u64 = 3;

/// Backend kind for a native libc `DIR*` iterator.
pub(crate) const STREAM_BACKEND_DIRECTORY: u64 = 4;

/// Backend kind for a buffered Phar write stream.
pub(crate) const STREAM_BACKEND_PHAR_WRITE: u64 = 5;

/// Backend kind for a userspace directory-wrapper instance.
pub(crate) const STREAM_BACKEND_USER_DIRECTORY: u64 = 6;

/// Backend kind for an owned `glob://` iterator state.
pub(crate) const STREAM_BACKEND_GLOB_DIRECTORY: u64 = 7;

/// Size in bytes of one stream-context state allocation.
pub(crate) const CONTEXT_STATE_SIZE: u64 = 32;

/// Byte offset of the retained options hash in a stream-context state.
pub(crate) const CONTEXT_OPTIONS_OFFSET: i64 = 0;

/// Byte offset of the retained params value in a stream-context state.
pub(crate) const CONTEXT_PARAMS_OFFSET: i64 = 8;

/// Byte offset of the retained notification callable in a stream-context state.
pub(crate) const CONTEXT_NOTIFIER_OFFSET: i64 = 16;

/// Byte offset of stream-context state flags.
pub(crate) const CONTEXT_FLAGS_OFFSET: i64 = 24;

/// Size in bytes of one stable filter-state allocation.
pub(crate) const FILTER_STATE_SIZE: u64 = 96;

/// Byte offset of the next filter in the chain (filter handle, 0 = tail).
pub(crate) const FILTER_NEXT_OFFSET: i64 = 0;

/// Byte offset of the previous filter in the chain (filter handle, 0 = head).
pub(crate) const FILTER_PREV_OFFSET: i64 = 8;

/// Byte offset of the owning stream handle, used to unlink on removal.
pub(crate) const FILTER_STREAM_HANDLE_OFFSET: i64 = 16;

/// Byte offset of the built-in filter id, or zero when a user class drives it.
pub(crate) const FILTER_BUILTIN_ID_OFFSET: i64 = 24;

/// Byte offset of the `php_user_filter` instance backing a user filter.
pub(crate) const FILTER_OBJECT_OFFSET: i64 = 32;

/// Byte offset of the direction bits this node was attached with.
pub(crate) const FILTER_DIRECTION_OFFSET: i64 = 40;

/// Byte offset of the retained `$params` value handed to `onCreate()`.
pub(crate) const FILTER_PARAMS_OFFSET: i64 = 48;

/// Byte offset of the filter lifecycle flags.
pub(crate) const FILTER_FLAGS_OFFSET: i64 = 56;

/// Byte offset of the `line-length` a built-in encoder wraps at, 0 for no wrapping.
///
/// The four fields below are the built-in filter parameters, read out of `$params` ONCE at attach
/// time rather than on every buffer. php's own defaults are what a zeroed allocation already says:
/// measured on `php -n` 8.5.6, both `convert.base64-encode` and `convert.quoted-printable-encode`
/// wrap at nothing unless the caller names a `line-length`.
pub(crate) const FILTER_LINE_LENGTH_OFFSET: i64 = 64;

/// Byte offset of the `line-break-chars` pointer, 0 when the default `\n` applies.
pub(crate) const FILTER_BREAK_PTR_OFFSET: i64 = 72;

/// Byte offset of the `line-break-chars` byte length.
pub(crate) const FILTER_BREAK_LEN_OFFSET: i64 = 80;

/// Byte offset of the `binary` flag, which is what makes quoted-printable escape SPACE and TAB.
pub(crate) const FILTER_BINARY_OFFSET: i64 = 88;

/// Read-direction bit, matching PHP's `STREAM_FILTER_READ`.
pub(crate) const FILTER_DIRECTION_READ: u64 = 1;

/// Write-direction bit, matching PHP's `STREAM_FILTER_WRITE`.
pub(crate) const FILTER_DIRECTION_WRITE: u64 = 2;

/// Set once `onClose()` has run so stream teardown cannot invoke it twice.
pub(crate) const FILTER_FLAG_ONCLOSE_CALLED: u64 = 1;

/// Marks a node that exists for its RESOURCE identity alone.
///
/// `zlib.*`, `bzip2.*` and `convert.iconv.*` are compiled as an inline filter shape over the
/// descriptor rather than as a chain node, so they filtered correctly but minted nothing —
/// `stream_filter_append($h, "zlib.deflate", ...)` answered a value `is_resource()` called false
/// where php answers a live `stream filter`. An inert node restores the identity php documents: it
/// joins the chain so that closing the stream closes it too, carries no built-in id and no object
/// so the applier passes it by, and tells `stream_filter_remove()` to clear the per-descriptor
/// tables that do the actual filtering.
pub(crate) const FILTER_FLAG_INERT: u64 = 2;

const _: () = {
    assert!(RESOURCE_KIND_FREE == 0);
    assert!(RESOURCE_KIND_FILTER != RESOURCE_KIND_STREAM);
    assert!(RESOURCE_KIND_FILTER != RESOURCE_KIND_CONTEXT);
    // The chain heads must live inside StreamState and clear of its other fields.
    assert!(STREAM_READ_FILTER_HEAD_OFFSET > STREAM_CONTEXT_HANDLE_OFFSET);
    assert!(STREAM_WRITE_FILTER_HEAD_OFFSET == STREAM_READ_FILTER_HEAD_OFFSET + 8);
    assert!(STREAM_WRITE_FILTER_HEAD_OFFSET < STREAM_CHUNK_SIZE_OFFSET);
    assert!(STREAM_TLS_SESSION_OFFSET == STREAM_WRITE_FILTER_HEAD_OFFSET + 8);
    assert!(STREAM_WRITTEN_SINCE_FLUSH_OFFSET == STREAM_TLS_SESSION_OFFSET + 8);
    assert!(STREAM_WRITTEN_SINCE_FLUSH_OFFSET < STREAM_CHUNK_SIZE_OFFSET);
    assert!(STREAM_TLS_SESSION_OFFSET < STREAM_CHUNK_SIZE_OFFSET);
    assert!(FILTER_PREV_OFFSET == FILTER_NEXT_OFFSET + 8);
    assert!(FILTER_STREAM_HANDLE_OFFSET == FILTER_PREV_OFFSET + 8);
    // The four built-in filter parameters sit after the lifecycle flags and close the node, so an
    // allocation that forgot to grow with them would be caught here rather than by a stray write.
    assert!(FILTER_LINE_LENGTH_OFFSET == FILTER_FLAGS_OFFSET + 8);
    assert!(FILTER_BREAK_PTR_OFFSET == FILTER_LINE_LENGTH_OFFSET + 8);
    assert!(FILTER_BREAK_LEN_OFFSET == FILTER_BREAK_PTR_OFFSET + 8);
    assert!(FILTER_BINARY_OFFSET == FILTER_BREAK_LEN_OFFSET + 8);
    assert!(FILTER_BINARY_OFFSET + 8 == FILTER_STATE_SIZE as i64);
    assert!(FILTER_DIRECTION_READ | FILTER_DIRECTION_WRITE == 3);
    assert!(SLOT_REQUEST_EPOCH_OFFSET == SLOT_NEXT_FREE_OFFSET);
    assert!(STREAM_WRAPPER_ID_OFFSET == STREAM_BACKEND_KIND_OFFSET + 8);
    assert!(STREAM_URI_PTR_OFFSET == STREAM_FD_OFFSET + 8);
    assert!(STREAM_URI_LEN_OFFSET == STREAM_URI_PTR_OFFSET + 8);
    assert!(STREAM_CONNECT_HOST_PTR_OFFSET == STREAM_URI_LEN_OFFSET + 8);
    assert!(STREAM_CONNECT_HOST_LEN_OFFSET == STREAM_CONNECT_HOST_PTR_OFFSET + 8);
    assert!(STREAM_EOF_OFFSET > STREAM_FD_OFFSET);
    assert!(STREAM_BACKEND_AUX_OFFSET == STREAM_EOF_OFFSET + 8);
    assert!(STREAM_CONTEXT_HANDLE_OFFSET > STREAM_BACKEND_AUX_OFFSET);
    assert!(STREAM_CONTEXT_HANDLE_OFFSET < STREAM_CHUNK_SIZE_OFFSET);
    assert!(STREAM_EOF_OFFSET < STREAM_CHUNK_SIZE_OFFSET);
    assert!(STREAM_CHUNK_SIZE_OFFSET < STREAM_OWNERSHIP_FLAGS_OFFSET);
    // The five filtered-read buffer fields sit in the gap between the chunk size and the
    // ownership flags, contiguous so a single loop can clear them.
    assert!(STREAM_FILTERED_BUF_PTR_OFFSET > STREAM_CHUNK_SIZE_OFFSET);
    assert!(STREAM_FILTERED_BUF_LEN_OFFSET == STREAM_FILTERED_BUF_PTR_OFFSET + 8);
    assert!(STREAM_FILTERED_BUF_CAP_OFFSET == STREAM_FILTERED_BUF_LEN_OFFSET + 8);
    assert!(STREAM_FILTERED_BUF_POS_OFFSET == STREAM_FILTERED_BUF_CAP_OFFSET + 8);
    assert!(STREAM_FILTERED_FLUSHED_OFFSET == STREAM_FILTERED_BUF_POS_OFFSET + 8);
    assert!(STREAM_FILTERED_FLUSHED_OFFSET < STREAM_OWNERSHIP_FLAGS_OFFSET);
    assert!(STREAM_MODE_PTR_OFFSET > STREAM_FILTERED_FLUSHED_OFFSET);
    assert!(STREAM_MODE_LEN_OFFSET == STREAM_MODE_PTR_OFFSET + 8);
    assert!(STREAM_MODE_LEN_OFFSET < STREAM_OWNERSHIP_FLAGS_OFFSET);
    assert!(STREAM_TRANSPORT_OFFSET > STREAM_MODE_LEN_OFFSET);
    assert!(STREAM_TRANSPORT_OFFSET < STREAM_OWNERSHIP_FLAGS_OFFSET);
    assert!(STREAM_APPEND_SKIP_OFFSET == STREAM_TRANSPORT_OFFSET + 8);
    assert!(STREAM_APPEND_SKIP_OFFSET < STREAM_OWNERSHIP_FLAGS_OFFSET);
    assert!(STREAM_WRAPPER_POS_OFFSET == STREAM_APPEND_SKIP_OFFSET + 8);
    assert!(STREAM_WRAPPER_POS_OFFSET < STREAM_OWNERSHIP_FLAGS_OFFSET);
    assert!(STREAM_FILTERED_POS_OFFSET == STREAM_WRAPPER_POS_OFFSET + 8);
    assert!(STREAM_FILTERED_POS_OFFSET < STREAM_OWNERSHIP_FLAGS_OFFSET);
    assert!(CONTEXT_STATE_SIZE == 32);
    assert!(CONTEXT_PARAMS_OFFSET == CONTEXT_OPTIONS_OFFSET + 8);
    assert!(CONTEXT_NOTIFIER_OFFSET == CONTEXT_PARAMS_OFFSET + 8);
    assert!(CONTEXT_FLAGS_OFFSET == CONTEXT_NOTIFIER_OFFSET + 8);
};
