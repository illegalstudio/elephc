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
    /// Frees any incremental hash contexts that were never finalized.
    fn drop(&mut self) {
        for context in self.hash_contexts.drain().map(|(_, context)| context) {
            unsafe {
                // The resource table owns these handles; draining prevents reuse
                // after the crypto free call.
                elephc_crypto::elephc_crypto_free(context.handle);
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
pub(super) fn eval_tcp_address(address: &str) -> &str {
    address
        .strip_prefix("tcp://")
        .or_else(|| address.strip_prefix("ssl://"))
        .or_else(|| address.strip_prefix("tls://"))
        .unwrap_or(address)
}

/// Converts Rust's socket shutdown enum into libc constants.
pub(super) fn eval_shutdown_how(shutdown: Shutdown) -> libc::c_int {
    match shutdown {
        Shutdown::Read => libc::SHUT_RD,
        Shutdown::Write => libc::SHUT_WR,
        Shutdown::Both => libc::SHUT_RDWR,
    }
}

/// Converts PHP `LOCK_*` bit flags into host `flock()` flags.
pub(super) fn eval_flock_operation(operation: i64) -> Option<libc::c_int> {
    let non_blocking = operation & 4 != 0;
    let base = match operation & !4 {
        1 => libc::LOCK_SH,
        2 => libc::LOCK_EX,
        3 => libc::LOCK_UN,
        _ => return None,
    };
    Some(base | if non_blocking { libc::LOCK_NB } else { 0 })
}

/// Returns whether the last host `flock()` failure was a non-blocking lock miss.
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

/// File stream stored behind one eval resource id.
pub(super) struct EvalFileStream {
    pub(super) file: File,
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
            file,
            uri,
            mode,
            eof: false,
            flush_target,
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

/// Builds a unique temporary path for eval `tmpfile()`.
pub(super) fn eval_tmpfile_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "elephc-magician-tmpfile-{}-{}",
        std::process::id(),
        eval_tmpfile_nonce()
    ));
    path
}

/// Returns a monotonic-ish nonce for temporary file names.
pub(super) fn eval_tmpfile_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}
