//! Purpose:
//! Defines concrete file, socket, wrapper, directory, hash, context, and fopen
//! mode storage used by `EvalStreamResources`.
//!
//! Called from:
//! - Resource opening, registration, operations, storage, and cleanup modules.
//!
//! Key details:
//! - File streams may carry a close-time write-back target for virtual wrappers.

use super::*;

impl Drop for EvalStreamResources {
    /// Frees any incremental hash contexts that were never finalized, and (curl feature
    /// only) any curl multi, easy, and share handles this eval context ever created — see
    /// `EvalCurlEasyHandle`'s own doc for why this `Drop` is the ONLY place they are freed.
    ///
    /// THE CURL ORDER IS MULTI -> EASY -> SHARE, and it is load-bearing on both ends:
    ///
    /// - MULTI BEFORE EASY, because `elephc_curl_multi_free` detaches every still-attached
    ///   easy handle with `curl_multi_remove_handle` before `curl_multi_cleanup`, and it
    ///   can only do that while those easy handles still exist. Freeing the easy handles
    ///   first is not unsafe (libcurl's own `curl_easy_cleanup` detaches a handle from its
    ///   multi, and the bridge skips ids it no longer knows), but it leaves the detaching
    ///   to libcurl's internals instead of doing it explicitly.
    /// - EASY BEFORE SHARE, because the bridge's share free is DEFERRED
    ///   (`crates/elephc-curl/src/share.rs`'s module doc): a `curl_share_cleanup()` while
    ///   any attached easy handle remains is refused by libcurl with `CURLSHE_IN_USE` and
    ///   frees nothing. Running `elephc_curl_easy_free` first lets each handle's own
    ///   `detach_easy` drain the share's attachment list, so the share free that follows
    ///   takes the immediate path and the native share (DNS cache, cookie jar, TLS session
    ///   cache, connection pool) is genuinely released. The other order also terminates
    ///   correctly — the share would simply be marked `pending_free` and cleaned up by the
    ///   LAST easy free instead — so this is a "make the common path the direct one"
    ///   choice, not a correctness cliff. Either way nothing is freed twice: the bridge
    ///   removes an entry from its table before cleaning it up, and this table drains.
    fn drop(&mut self) {
        for context in self.hash_contexts.drain().map(|(_, context)| context) {
            unsafe {
                // The resource table owns these handles; draining prevents reuse
                // after the crypto free call.
                elephc_crypto::elephc_crypto_free(context.handle);
            }
        }
        #[cfg(feature = "curl")]
        {
            for handle in self.curl_multi_handles.drain().map(|(_, handle)| handle) {
                crate::curl_ffi::multi_free(handle.raw);
            }
            for handle in self.curl_easy_handles.drain().map(|(_, handle)| handle) {
                crate::curl_ffi::easy_free(handle.raw);
            }
            for handle in self.curl_share_handles.drain().map(|(_, handle)| handle) {
                crate::curl_ffi::share_free(handle.raw);
            }
        }
    }
}

/// PHP-visible metadata for one eval stream resource.
pub(crate) struct EvalStreamMetaData {
    pub(crate) eof: bool,
    pub(crate) mode: String,
    pub(crate) uri: String,
}

/// Local and peer names tracked for socket-backed eval streams.
pub(super) struct EvalSocketNames {
    pub(super) local: String,
    pub(super) peer: Option<String>,
}

/// Normalizes supported TCP-style stream socket addresses.
///
/// Only the plaintext `tcp://` scheme is stripped. The crypto transports used to be
/// stripped here too, which silently turned `ssl://` and `tls://` into unencrypted
/// TCP connections; `eval_address_selects_tls` now rejects them before they reach
/// this point.
pub(super) fn eval_tcp_address(address: &str) -> &str {
    address.strip_prefix("tcp://").unwrap_or(address)
}

/// Reports whether an address names one of PHP's crypto transports.
///
/// php-src registers `ssl`, `sslv3`, `tls` and `tlsv1.0`..`tlsv1.3` as transports
/// that negotiate TLS inside the connect (`php_openssl_ssl_socket_factory`,
/// ext/openssl/xp_ssl.c). Scheme names are case-insensitive. `sslv2` is excluded
/// because php rejects it rather than negotiating. This mirrors the scheme set the
/// compiled path matches in `__rt_addr_tls_crypto_method`.
pub(super) fn eval_address_selects_tls(address: &str) -> bool {
    let Some((scheme, _)) = address.split_once("://") else {
        return false;
    };
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "ssl" | "sslv3" | "tls" | "tlsv1.0" | "tlsv1.1" | "tlsv1.2" | "tlsv1.3"
    )
}

/// Returns the error reported when an eval fragment asks for a crypto transport.
///
/// The interpreter has no TLS engine: `elephc-magician` does not link the
/// `elephc-tls` bridge, so it cannot complete a handshake. Refusing is the only
/// honest answer -- returning a plaintext socket for a `tls://` address would hand
/// back an unencrypted connection under a name that promises confidentiality, which
/// is strictly worse than failing. The compiled path negotiates these transports
/// normally; only dynamic `eval()` fragments reach this fallback.
pub(super) fn eval_tls_transport_unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "TLS stream transports are not available inside eval(); \
         the interpreter fallback has no TLS engine and will not open an \
         unencrypted socket for a crypto scheme",
    )
}

/// Converts PHP `LOCK_*` bit flags into host `flock()` flags.
pub(super) fn eval_flock_operation(operation: i64) -> Option<libc::c_int> {
    let non_blocking = operation & 4 != 0;
    #[cfg(unix)]
    let base = match operation & !4 {
        1 => libc::LOCK_SH,
        2 => libc::LOCK_EX,
        3 => libc::LOCK_UN,
        _ => return None,
    };
    #[cfg(windows)]
    let base = match operation & !4 {
        1 => 1,
        2 => 2,
        3 => 3,
        _ => return None,
    };
    #[cfg(unix)]
    let non_blocking_flag = libc::LOCK_NB;
    #[cfg(windows)]
    let non_blocking_flag = 4;
    Some(base | if non_blocking { non_blocking_flag } else { 0 })
}

/// Returns whether the last host `flock()` failure was a non-blocking lock miss.
#[cfg(unix)]
pub(super) fn eval_flock_would_block() -> bool {
    let errno = std::io::Error::last_os_error().raw_os_error();
    errno.is_some_and(|code| code == libc::EWOULDBLOCK || code == libc::EAGAIN)
}

/// Converts an elephc-crypto digest length into owned raw bytes.
pub(super) fn eval_hash_digest_bytes(len: isize, output: &[u8; 64]) -> Option<Vec<u8>> {
    let len = usize::try_from(len).ok()?;
    if len > output.len() {
        return None;
    }
    Some(output[..len].to_vec())
}

/// Normalizes a PHP stream wrapper protocol name for eval registry storage.
pub(super) fn eval_normalize_stream_wrapper_protocol(protocol: &str) -> Option<String> {
    let protocol = protocol.trim().trim_end_matches("://");
    if protocol.is_empty() {
        return None;
    }
    Some(protocol.to_ascii_lowercase())
}

/// Returns whether the protocol is one of elephc's built-in stream wrappers.
pub(super) fn eval_builtin_stream_wrapper_exists(builtins: &[&str], protocol: &str) -> bool {
    builtins
        .iter()
        .any(|builtin| builtin.eq_ignore_ascii_case(protocol))
}

/// Host I/O object stored behind one eval stream resource.
pub(super) enum EvalStreamHandle {
    File(File),
    Tcp(TcpStream),
    ChildStdout(ChildStdout),
    ChildStdin(ChildStdin),
}

impl Read for EvalStreamHandle {
    /// Reads from handles that expose a readable byte stream.
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(handle) => handle.read(buffer),
            Self::Tcp(handle) => handle.read(buffer),
            Self::ChildStdout(handle) => handle.read(buffer),
            Self::ChildStdin(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "process stdin is not readable",
            )),
        }
    }
}

impl Write for EvalStreamHandle {
    /// Writes to handles that expose a writable byte stream.
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::File(handle) => handle.write(buffer),
            Self::Tcp(handle) => handle.write(buffer),
            Self::ChildStdin(handle) => handle.write(buffer),
            Self::ChildStdout(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "process stdout is not writable",
            )),
        }
    }

    /// Flushes writable handles while treating unbuffered sockets as already flushed.
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::File(handle) => handle.flush(),
            Self::Tcp(handle) => handle.flush(),
            Self::ChildStdin(handle) => handle.flush(),
            Self::ChildStdout(_) => Ok(()),
        }
    }
}

impl Seek for EvalStreamHandle {
    /// Seeks regular files and rejects non-seekable sockets and process pipes.
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        match self {
            Self::File(handle) => handle.seek(position),
            Self::Tcp(_) | Self::ChildStdout(_) | Self::ChildStdin(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "stream is not seekable",
            )),
        }
    }
}

impl EvalStreamHandle {
    /// Returns regular-file metadata when the resource has a file backing.
    pub(super) fn metadata(&self) -> io::Result<Metadata> {
        match self {
            Self::File(handle) => handle.metadata(),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "no file metadata")),
        }
    }

    /// Resizes a regular file and rejects non-file resources.
    pub(super) fn set_len(&self, size: u64) -> io::Result<()> {
        match self {
            Self::File(handle) => handle.set_len(size),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "stream is not resizable")),
        }
    }

    /// Synchronizes a regular file's data and metadata.
    pub(super) fn sync_all(&self) -> io::Result<()> {
        match self {
            Self::File(handle) => handle.sync_all(),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "stream is not syncable")),
        }
    }

    /// Synchronizes a regular file's data.
    pub(super) fn sync_data(&self) -> io::Result<()> {
        match self {
            Self::File(handle) => handle.sync_data(),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "stream is not syncable")),
        }
    }

    /// Applies socket shutdown only to TCP-backed resources.
    pub(super) fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match self {
            Self::Tcp(handle) => handle.shutdown(how),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "stream is not a socket")),
        }
    }

    /// Toggles non-blocking mode only for TCP-backed resources.
    #[cfg(windows)]
    pub(super) fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        match self {
            Self::Tcp(handle) => handle.set_nonblocking(nonblocking),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "stream is not a socket")),
        }
    }

    /// Applies PHP flock semantics to regular Windows file handles.
    #[cfg(windows)]
    pub(super) fn flock(&self, operation: i64) -> io::Result<()> {
        use std::ffi::c_void;

        #[repr(C)]
        struct Overlapped {
            internal: usize,
            internal_high: usize,
            offset: u32,
            offset_high: u32,
            event: *mut c_void,
        }

        #[link(name = "kernel32")]
        unsafe extern "system" {
            /// Acquires a shared or exclusive byte-range lock on a Windows file handle.
            fn LockFileEx(
                file: *mut c_void,
                flags: u32,
                reserved: u32,
                bytes_low: u32,
                bytes_high: u32,
                overlapped: *mut Overlapped,
            ) -> i32;
            /// Releases a byte-range lock from a Windows file handle.
            fn UnlockFileEx(
                file: *mut c_void,
                reserved: u32,
                bytes_low: u32,
                bytes_high: u32,
                overlapped: *mut Overlapped,
            ) -> i32;
        }

        const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x0000_0001;
        const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;
        let Self::File(file) = self else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "flock requires a regular file",
            ));
        };
        let mut overlapped = Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            event: std::ptr::null_mut(),
        };
        let base = operation & !4;
        let status = unsafe {
            if base == 3 {
                UnlockFileEx(
                    file.as_raw_handle(),
                    0,
                    u32::MAX,
                    u32::MAX,
                    &mut overlapped,
                )
            } else {
                let mut flags = if base == 2 { LOCKFILE_EXCLUSIVE_LOCK } else { 0 };
                if operation & 4 != 0 {
                    flags |= LOCKFILE_FAIL_IMMEDIATELY;
                }
                LockFileEx(
                    file.as_raw_handle(),
                    flags,
                    0,
                    u32::MAX,
                    u32::MAX,
                    &mut overlapped,
                )
            }
        };
        if status == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Returns the borrowed Unix descriptor used by terminal and lock operations.
    #[cfg(unix)]
    pub(super) fn as_raw_fd(&self) -> RawFd {
        match self {
            Self::File(handle) => handle.as_raw_fd(),
            Self::Tcp(handle) => handle.as_raw_fd(),
            Self::ChildStdout(handle) => handle.as_raw_fd(),
            Self::ChildStdin(handle) => handle.as_raw_fd(),
        }
    }

    /// Returns the borrowed Windows HANDLE used by console-mode operations.
    #[cfg(windows)]
    pub(super) fn as_raw_handle(&self) -> isize {
        match self {
            Self::File(handle) => handle.as_raw_handle() as isize,
            Self::Tcp(_) => -1,
            Self::ChildStdout(handle) => handle.as_raw_handle() as isize,
            Self::ChildStdin(handle) => handle.as_raw_handle() as isize,
        }
    }
}

/// File stream stored behind one eval resource id.
pub(super) struct EvalFileStream {
    pub(super) file: EvalStreamHandle,
    pub(super) uri: String,
    pub(super) mode: String,
    pub(super) eof: bool,
    pub(super) flush_target: Option<EvalStreamFlushTarget>,
}

impl EvalFileStream {
    /// Creates a tracked stream around a host file handle.
    pub(super) fn new(file: File, uri: String, mode: String) -> Self {
        Self::new_with_flush_target(file, uri, mode, None)
    }

    /// Creates a tracked stream that may write back to a wrapper target on close.
    pub(super) fn new_with_flush_target(
        file: File,
        uri: String,
        mode: String,
        flush_target: Option<EvalStreamFlushTarget>,
    ) -> Self {
        Self {
            file: EvalStreamHandle::File(file),
            uri,
            mode,
            eof: false,
            flush_target,
        }
    }

    /// Creates a tracked stream around a TCP socket without conflating SOCKET and HANDLE on Windows.
    pub(super) fn new_tcp(stream: TcpStream, uri: String, mode: String) -> Self {
        Self {
            file: EvalStreamHandle::Tcp(stream),
            uri,
            mode,
            eof: false,
            flush_target: None,
        }
    }

    /// Creates a readable tracked stream around a child process stdout pipe.
    pub(super) fn new_child_stdout(stream: ChildStdout, uri: String, mode: String) -> Self {
        Self {
            file: EvalStreamHandle::ChildStdout(stream),
            uri,
            mode,
            eof: false,
            flush_target: None,
        }
    }

    /// Creates a writable tracked stream around a child process stdin pipe.
    pub(super) fn new_child_stdin(stream: ChildStdin, uri: String, mode: String) -> Self {
        Self {
            file: EvalStreamHandle::ChildStdin(stream),
            uri,
            mode,
            eof: false,
            flush_target: None,
        }
    }

    /// Flushes any buffered wrapper target before the stream resource disappears.
    pub(super) fn finalize_on_close(mut self) -> bool {
        let Some(flush_target) = self.flush_target.take() else {
            return true;
        };
        let mut bytes = Vec::new();
        if self.file.flush().is_err() || self.file.seek(SeekFrom::Start(0)).is_err() {
            return false;
        }
        if self.file.read_to_end(&mut bytes).is_err() {
            return false;
        }
        flush_target.write_back(&bytes)
    }
}

/// Userspace wrapper stream stored behind one eval resource id.
pub(super) struct EvalUserWrapperStream {
    pub(super) object: RuntimeCellHandle,
    pub(super) class_name: String,
    pub(super) uri: String,
    pub(super) mode: String,
    pub(super) eof: bool,
}

impl EvalUserWrapperStream {
    /// Copies the dispatch-relevant wrapper fields out of the resource table.
    pub(super) fn info(&self) -> EvalUserWrapperStreamInfo {
        EvalUserWrapperStreamInfo {
            object: self.object,
            class_name: self.class_name.clone(),
            eof: self.eof,
        }
    }
}

/// Copied userspace-wrapper stream fields used while dispatching PHP methods.
pub(crate) struct EvalUserWrapperStreamInfo {
    pub(crate) object: RuntimeCellHandle,
    pub(crate) class_name: String,
    pub(crate) eof: bool,
}

/// Userspace-wrapper directory stored behind one eval resource id.
pub(super) struct EvalUserWrapperDirectory {
    pub(super) object: RuntimeCellHandle,
    pub(super) class_name: String,
}

impl EvalUserWrapperDirectory {
    /// Copies the dispatch fields needed while invoking wrapper directory methods.
    pub(super) fn info(&self) -> EvalUserWrapperDirectoryInfo {
        EvalUserWrapperDirectoryInfo {
            object: self.object,
            class_name: self.class_name.clone(),
        }
    }
}

/// Copied userspace-wrapper directory fields used while dispatching PHP methods.
pub(crate) struct EvalUserWrapperDirectoryInfo {
    pub(crate) object: RuntimeCellHandle,
    pub(crate) class_name: String,
}

/// Wrapper targets that need a write-back step when their stream closes.
pub(super) enum EvalStreamFlushTarget {
    PharUrl(Vec<u8>),
}

impl EvalStreamFlushTarget {
    /// Writes buffered stream bytes back to the target URL.
    pub(super) fn write_back(&self, bytes: &[u8]) -> bool {
        match self {
            Self::PharUrl(url) => elephc_phar::put_url_bytes(url, bytes).is_some(),
        }
    }
}

/// Directory stream stored behind one eval resource id.
pub(super) struct EvalDirectoryStream {
    pub(super) entries: Vec<String>,
    pub(super) index: usize,
}

impl EvalDirectoryStream {
    /// Opens a local directory and snapshots its entry names.
    pub(super) fn open(path: &str) -> Option<Self> {
        let entries = std::fs::read_dir(path).ok()?;
        let mut names = vec![".".to_string(), "..".to_string()];
        for entry in entries {
            let entry = entry.ok()?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        Some(Self {
            entries: names,
            index: 0,
        })
    }

    /// Returns the next directory entry name.
    pub(super) fn read(&mut self) -> Option<String> {
        let name = self.entries.get(self.index)?.clone();
        self.index += 1;
        Some(name)
    }

    /// Moves the directory cursor back to its first entry.
    pub(super) fn rewind(&mut self) -> bool {
        self.index = 0;
        true
    }
}

/// Opaque elephc-crypto incremental hash context resource.
pub(super) struct EvalHashContext {
    pub(super) handle: *mut c_void,
}

/// Opaque `elephc-curl` easy-handle resource, plus the same small set of PHP-layer
/// mirror fields `crate::curl_prelude::CurlHandle` keeps on the object in the AOT
/// build (`$__elephc_return_transfer`/`$__elephc_private`/`$__elephc_write_user`):
/// `curl_getinfo(..., CURLINFO_PRIVATE)` and `curl_exec()`'s return-shape decision both
/// need them and neither is anything the bridge itself tracks.
///
/// FREED ONLY BY `EvalStreamResources::drop`, never by any PHP-visible action: eval has
/// no real `CurlHandle` object to hang a destructor off (this is a `resource kind 5`
/// cell, `RuntimeValueOps::hash_context`'s doc explains why that means "no destructor
/// runs" at Mixed-cell teardown), and `curl_close()` is a documented no-op in PHP 8
/// itself. A curl handle created inside `eval()` therefore lives for the lifetime of the
/// surrounding `ElephcEvalContext` — the same accepted, documented tradeoff
/// `EvalHashContext`'s never-finalized case already makes.
#[cfg(feature = "curl")]
pub(super) struct EvalCurlEasyHandle {
    /// The bridge's own easy-handle id (`elephc_curl_easy_init()`'s return value) —
    /// NOT the eval table key. Every `elephc_curl_easy_*` call needs this, not the
    /// `EvalStreamResources` key that boxes it into a runtime cell.
    pub(super) raw: i64,
    /// Mirrors `CurlHandle::$__elephc_return_transfer`.
    pub(super) return_transfer: bool,
    /// Mirrors `CurlHandle::$__elephc_write_user`.
    pub(super) write_user: bool,
    /// Mirrors `CurlHandle::$__elephc_private` (`CURLOPT_PRIVATE`'s stored value).
    /// `None` until first set, read back as PHP `false` — matching the AOT property's
    /// `false` default.
    pub(super) private_value: Option<RuntimeCellHandle>,
    /// Mirrors `CurlHandle::$__elephc_callbacks`, indexed by `crate::curl_ffi`'s `SLOT_*`.
    /// THE ROOT for every installed callback: the bridge's own slot holds only two opaque
    /// integers (the slot index and this handle's eval table key), never the callable, so
    /// this table is what keeps the PHP value alive between `curl_setopt()` and the
    /// transfer that fires it.
    pub(super) callbacks: [EvalCurlCallbackSlot; crate::curl_ffi::SLOT_COUNT],
}

/// One `CURLOPT_*FUNCTION` slot's eval-side state.
#[cfg(feature = "curl")]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum EvalCurlCallbackSlot {
    /// Nothing installed: the bridge slot is cleared and libcurl's own default applies.
    #[default]
    Empty,
    /// INSTALLED BUT DELIBERATELY SILENT — the `CURLOPT_DEBUGFUNCTION => null` case, and
    /// only that one. Clearing the debug registration does not restore "nothing", it
    /// restores libcurl's OWN default, which with `CURLOPT_VERBOSE` on dumps the entire
    /// trace to the process's fd 2; php-src prints nothing there because its C trampoline
    /// stays installed with no PHP callable behind it. Keeping the bridge slot registered
    /// with no callable reproduces php exactly. See
    /// `crate::curl_prelude::curl_setopt`'s `$slot === 4` branch, which installs a no-op
    /// closure for the identical reason.
    Silent,
    /// A real PHP callable, RETAINED by this table (released on reset, overwrite, and
    /// context teardown).
    Callable(RuntimeCellHandle),
}

/// Opaque `elephc-curl` MULTI-handle resource, plus the eval counterpart of
/// `crate::curl_prelude::CurlMultiHandle`'s `$__elephc_ids`/`$__elephc_handles` identity
/// map. The AOT class needs two parallel lists because it stores real PHP OBJECTS whose
/// identity `curl_multi_info_read()` must hand back; eval stores only the attached easy
/// handles' EVAL TABLE KEYS, because an eval curl handle is an inert resource-kind-5 cell
/// (`crate::interpreter::builtins::curl`'s module doc) that can be re-boxed from its key at
/// any time — two cells carrying the same key are interchangeable and neither owns
/// anything, so there is no double-free hazard for AOT's map to prevent here.
#[cfg(feature = "curl")]
pub(super) struct EvalCurlMultiHandle {
    /// The bridge's own multi-handle id (`elephc_curl_multi_init()`'s return value).
    pub(super) raw: i64,
    /// EVAL TABLE KEYS of the easy handles currently attached, in add order — what
    /// `curl_multi_get_handles()` lists and what `curl_multi_info_read()` resolves a
    /// completion message's easy handle through.
    pub(super) attached: Vec<i64>,
}

/// Opaque `elephc-curl` SHARE-handle resource. Carries no attachment bookkeeping of its
/// own: the BRIDGE owns that (`crates/elephc-curl/src/share.rs`'s `ShareEntry::attached`
/// and its deferred-free protocol), and duplicating it here would be a second, desyncable
/// copy of the same truth.
#[cfg(feature = "curl")]
pub(super) struct EvalCurlShareHandle {
    /// The bridge's own share-handle id.
    pub(super) raw: i64,
    /// Set for a share minted by PHP 8.5's `curl_share_init_persistent()`. Recorded only so
    /// `curl_setopt($ch, CURLOPT_SHARE, $sh)`'s TypeError message and this table's own
    /// teardown can tell the two apart; the bridge independently refuses to free a
    /// persistent share, so this flag is never load-bearing for lifetime.
    pub(super) persistent: bool,
}

/// Stream context metadata tracked by eval.
pub(super) struct EvalStreamContext {
    pub(super) options: Option<RuntimeCellHandle>,
}

/// Parsed PHP fopen mode used to configure `OpenOptions`.
pub(super) struct EvalOpenMode {
    pub(super) read: bool,
    pub(super) write: bool,
    pub(super) append: bool,
    pub(super) truncate: bool,
    pub(super) create: bool,
    pub(super) create_new: bool,
    pub(super) label: String,
}

impl EvalOpenMode {
    /// Parses PHP's common fopen mode grammar, ignoring binary/text markers.
    pub(super) fn parse(mode: &str) -> Option<Self> {
        let mut chars = mode.chars();
        let first = chars.next()?;
        let plus = mode.contains('+');
        if !mode
            .chars()
            .all(|ch| matches!(ch, 'r' | 'w' | 'a' | 'x' | 'c' | '+' | 'b' | 't' | 'e'))
        {
            return None;
        }
        let mut mode = match first {
            'r' => Self {
                read: true,
                write: plus,
                append: false,
                truncate: false,
                create: false,
                create_new: false,
                label: if plus { "r+" } else { "r" }.to_string(),
            },
            'w' => Self {
                read: plus,
                write: true,
                append: false,
                truncate: true,
                create: true,
                create_new: false,
                label: if plus { "w+" } else { "w" }.to_string(),
            },
            'a' => Self {
                read: plus,
                write: true,
                append: true,
                truncate: false,
                create: true,
                create_new: false,
                label: if plus { "a+" } else { "a" }.to_string(),
            },
            'x' => Self {
                read: plus,
                write: true,
                append: false,
                truncate: false,
                create: false,
                create_new: true,
                label: if plus { "x+" } else { "x" }.to_string(),
            },
            'c' => Self {
                read: plus,
                write: true,
                append: false,
                truncate: false,
                create: true,
                create_new: false,
                label: if plus { "c+" } else { "c" }.to_string(),
            },
            _ => return None,
        };
        mode.write = mode.write || plus;
        Some(mode)
    }

    /// Opens a path with the parsed stream mode.
    pub(super) fn open(&self, path: &str) -> std::io::Result<File> {
        OpenOptions::new()
            .read(self.read)
            .write(self.write)
            .append(self.append)
            .truncate(self.truncate)
            .create(self.create)
            .create_new(self.create_new)
            .open(path)
    }
}

/// Builds a unique temporary path for eval `tmpfile()` and every ephemeral stream.
///
/// `open_ephemeral_stream` backs `php://memory`, `data:` and buffered `phar://` writes
/// with a file created here and unlinked immediately, so this is on the path of far more
/// eval streams than `tmpfile()` alone — which is why its uniqueness has to hold under
/// concurrency. See `eval_tmpfile_nonce`.
pub(super) fn eval_tmpfile_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "elephc-magician-tmpfile-{}-{}",
        std::process::id(),
        eval_tmpfile_nonce()
    ));
    path
}

/// Returns a nonce for temporary file names that no other call in this process repeats.
///
/// THE WALL CLOCK ALONE IS NOT A NONCE. `SystemTime::now()` is *reported* in
/// nanoseconds, but `CLOCK_REALTIME` only ADVANCES in whole microseconds on macOS (and
/// at a granularity of its own, coarser than a nanosecond, elsewhere), so `as_nanos()`
/// hands the same number to every caller inside one tick. Two threads opening an
/// ephemeral eval stream in the same tick therefore built the SAME path, and the loser's
/// `create_new` failed with `AlreadyExists` in the window between the winner's create and
/// its `remove_file` — which `open_ephemeral_stream` turns into `None`, i.e. an eval
/// `fopen("php://memory")` answering `false` for a stream with nothing wrong with it.
///
/// The process-wide sequence is what makes the nonce unique: `fetch_add` gives every
/// caller a distinct number no matter how many threads ask at once. The clock stays
/// because the sequence restarts at zero in each process, so it is the clock — with the
/// process id — that separates one run of the compiler from the next.
pub(super) fn eval_tmpfile_nonce() -> String {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{clock}-{sequence}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    /// Threads: enough to have several callers inside one clock tick.
    const RACING_THREADS: usize = 8;

    /// Barrier-aligned attempts per thread.
    const RACING_ROUNDS: usize = 256;

    /// Runs `body` on `RACING_THREADS` threads, realigned on a barrier every round.
    ///
    /// The barrier is what turns a probabilistic race into a forced one: every round
    /// releases all the threads at the same instant, so they call the code under test
    /// inside the same microsecond rather than whenever the scheduler feels like it.
    ///
    /// `body` MUST NOT PANIC. A thread that unwinds never reaches the next
    /// `Barrier::wait`, which hangs its siblings forever instead of failing the test —
    /// so the callers below record what they saw and assert after the join.
    fn race<T: Send + 'static>(
        body: impl Fn(&mut EvalStreamResources) -> T + Send + Clone + 'static,
    ) -> Vec<Vec<T>> {
        let barrier = Arc::new(Barrier::new(RACING_THREADS));
        let mut handles = Vec::with_capacity(RACING_THREADS);
        for _ in 0..RACING_THREADS {
            let barrier = Arc::clone(&barrier);
            let body = body.clone();
            handles.push(std::thread::spawn(move || {
                let mut observed = Vec::with_capacity(RACING_ROUNDS);
                for _ in 0..RACING_ROUNDS {
                    barrier.wait();
                    // A fresh table per round so the round's file is closed and
                    // dropped before the next one opens, keeping the fd count at
                    // one per thread instead of one per round.
                    let mut resources = EvalStreamResources::default();
                    observed.push(body(&mut resources));
                }
                observed
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("racing thread"))
            .collect()
    }

    /// Two threads must never build the SAME temporary path.
    ///
    /// This is the mechanism itself, with no filesystem in the way. The wall clock is
    /// not a nonce: `SystemTime::now()` is *reported* in nanoseconds, but
    /// `CLOCK_REALTIME` only ADVANCES in whole microseconds on macOS, so a name built
    /// from the clock alone is identical for every caller inside one tick. Against a
    /// clock-only nonce the barrier makes duplicates near-certain over these rounds;
    /// with the sequence counter no interleaving can produce one.
    #[test]
    fn concurrent_temporary_paths_are_all_distinct() {
        let observed = race(|_| eval_tmpfile_path());
        let total = observed.iter().map(Vec::len).sum::<usize>();
        let distinct: HashSet<&PathBuf> = observed.iter().flatten().collect();
        assert_eq!(distinct.len(), total, "two threads built the same temporary path");
    }

    /// Concurrent eval `fopen("php://memory")` calls must all return a stream.
    ///
    /// The PHP-visible symptom of the same defect, through the real opener. The loser
    /// of a path collision failed `create_new` with `AlreadyExists` in the window
    /// between the winner's create and its `remove_file`, and `open_ephemeral_stream`
    /// turns any such error into `None` — an eval `fopen()` answering `false` for a
    /// stream that has nothing wrong with it. Counted rather than asserted inside the
    /// threads, because a panic there would deadlock the barrier.
    #[test]
    fn concurrent_ephemeral_streams_never_fail_to_open() {
        let failures = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&failures);
        let observed = race(move |resources: &mut EvalStreamResources| {
            if resources.open_path("php://memory", "w+").is_none() {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        });
        let total = observed.iter().map(Vec::len).sum::<usize>();
        assert_eq!(
            failures.load(Ordering::Relaxed),
            0,
            "php://memory failed to open in {total} concurrent attempts"
        );
    }

    /// A temporary path must stay inside the temp directory and carry this process id.
    ///
    /// The NEGATIVE CONTROL for the two tests above: a bare global counter would make
    /// every path distinct within this process and pass both, while colliding with
    /// every OTHER elephc process on the machine. The sequence restarts at zero in each
    /// process, so the process id is what keeps concurrent processes apart, and the
    /// directory is what keeps the file out of the compiling project.
    #[test]
    fn temporary_paths_stay_scoped_to_this_process_and_the_temp_directory() {
        let path = eval_tmpfile_path();
        assert!(
            path.starts_with(std::env::temp_dir()),
            "{} is not in the temp directory",
            path.display()
        );
        let name = path
            .file_name()
            .expect("temporary file name")
            .to_string_lossy()
            .into_owned();
        let prefix = format!("elephc-magician-tmpfile-{}-", std::process::id());
        assert!(name.starts_with(&prefix), "{name} does not start with {prefix}");
        assert!(name.len() > prefix.len(), "{name} carries no nonce");
    }
}

#[cfg(test)]
mod transport_scheme_tests {
    use super::{eval_address_selects_tls, eval_tcp_address};

    /// Verifies every crypto transport php-src registers is recognised, in any case,
    /// so none of them can fall through to an unencrypted socket.
    #[test]
    fn detects_every_crypto_scheme() {
        for address in [
            "ssl://h:1",
            "sslv3://h:1",
            "tls://h:1",
            "tlsv1.0://h:1",
            "tlsv1.1://h:1",
            "tlsv1.2://h:1",
            "tlsv1.3://h:1",
            "TLS://h:1",
            "SsL://h:1",
        ] {
            assert!(eval_address_selects_tls(address), "{address} must select TLS");
        }
    }

    /// Verifies plaintext and unknown schemes stay on the plain path. `sslv2` belongs
    /// here because php rejects it outright rather than negotiating it.
    #[test]
    fn leaves_plain_schemes_alone() {
        for address in [
            "tcp://h:1",
            "udp://h:1",
            "unix:///tmp/s",
            "sslv2://h:1",
            "tlsv1.4://h:1",
            "tlsx://h:1",
            "h:1",
        ] {
            assert!(!eval_address_selects_tls(address), "{address} must stay plain");
        }
    }

    /// Verifies only the plaintext scheme is stripped, so a crypto address can never
    /// be normalised into something `TcpStream::connect` would happily open.
    #[test]
    fn strips_only_the_plaintext_scheme() {
        assert_eq!(eval_tcp_address("tcp://h:1"), "h:1");
        assert_eq!(eval_tcp_address("h:1"), "h:1");
        assert_eq!(eval_tcp_address("tls://h:1"), "tls://h:1");
    }
}
