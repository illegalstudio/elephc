//! Purpose:
//! Owns eval-local resource storage backed by host file handles, directory
//! snapshots, stream contexts, and hash contexts. Runtime Mixed cells only carry
//! a numeric resource id.
//!
//! Called from:
//! - `crate::context::ElephcEvalContext` stream-resource accessors.
//! - `crate::interpreter::builtins::filesystem` stream builtin helpers.
//!
//! Key details:
//! - Resource ids are zero-based runtime payloads; PHP display ids are payload + 1.
//! - Resource handles are process-local to eval and are not visible across the C ABI.

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;

use crate::value::RuntimeCellHandle;

/// Eval-owned table of local file streams keyed by runtime resource payload.
#[derive(Default)]
pub(crate) struct EvalStreamResources {
    chunk_sizes: HashMap<i64, i64>,
    default_stream_context: Option<i64>,
    next_id: i64,
    directories: HashMap<i64, EvalDirectoryStream>,
    hash_contexts: HashMap<i64, EvalHashContext>,
    stream_contexts: HashMap<i64, EvalStreamContext>,
    streams: HashMap<i64, EvalFileStream>,
}

impl EvalStreamResources {
    /// Opens a local path using PHP's common `fopen()` mode strings.
    pub(crate) fn open_path(&mut self, path: &str, mode: &str) -> Option<i64> {
        let mode = EvalOpenMode::parse(mode)?;
        let file = mode.open(path).ok()?;
        Some(self.insert(EvalFileStream::new(file, path.to_string(), mode.label)))
    }

    /// Opens an anonymous temporary file and returns its resource id.
    pub(crate) fn open_tmpfile(&mut self) -> Option<i64> {
        let path = eval_tmpfile_path();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .ok()?;
        let _ = std::fs::remove_file(&path);
        Some(self.insert(EvalFileStream::new(
            file,
            path.to_string_lossy().into_owned(),
            "w+".to_string(),
        )))
    }

    /// Opens a local directory and returns its resource id.
    pub(crate) fn open_directory(&mut self, path: &str) -> Option<i64> {
        let directory = EvalDirectoryStream::open(path)?;
        Some(self.insert_directory(directory))
    }

    /// Opens an incremental hash context and returns its resource id.
    pub(crate) fn open_hash_context(&mut self, algo: &[u8]) -> Option<i64> {
        let handle = unsafe {
            // elephc-crypto reads the algorithm name during this call and returns
            // an owned opaque context handle on success.
            elephc_crypto::elephc_crypto_init(algo.as_ptr(), algo.len())
        };
        if handle.is_null() {
            return None;
        }
        Some(self.insert_hash_context(EvalHashContext { handle }))
    }

    /// Opens a stream context resource with optional persisted options.
    pub(crate) fn open_stream_context(&mut self, options: Option<RuntimeCellHandle>) -> i64 {
        self.insert_stream_context(EvalStreamContext { options })
    }

    /// Returns the default stream context resource id, creating it if needed.
    pub(crate) fn default_stream_context(&mut self) -> i64 {
        if let Some(id) = self.default_stream_context {
            return id;
        }
        let id = self.open_stream_context(None);
        self.default_stream_context = Some(id);
        id
    }

    /// Removes a stream resource from the table, closing its file handle.
    pub(crate) fn close(&mut self, id: i64) -> bool {
        self.streams.remove(&id).is_some()
    }

    /// Removes a directory resource from the table.
    pub(crate) fn close_directory(&mut self, id: i64) -> bool {
        self.directories.remove(&id).is_some()
    }

    /// Reads up to `length` bytes from a stream resource.
    pub(crate) fn read(&mut self, id: i64, length: usize) -> Option<Vec<u8>> {
        let stream = self.streams.get_mut(&id)?;
        let mut buffer = vec![0_u8; length];
        let read = stream.file.read(&mut buffer).ok()?;
        buffer.truncate(read);
        stream.eof = read == 0 || read < length;
        Some(buffer)
    }

    /// Reads the next entry name from a directory resource.
    pub(crate) fn read_directory(&mut self, id: i64) -> Option<String> {
        self.directories.get_mut(&id)?.read()
    }

    /// Feeds bytes into an incremental hash context.
    pub(crate) fn update_hash_context(&mut self, id: i64, data: &[u8]) -> bool {
        let Some(context) = self.hash_contexts.get_mut(&id) else {
            return false;
        };
        unsafe {
            // The table owns the opaque handle and this mutable borrow gives the
            // crypto call exclusive access for the duration of the update.
            elephc_crypto::elephc_crypto_update(context.handle, data.as_ptr(), data.len());
        }
        true
    }

    /// Returns the persisted options for a stream context resource.
    pub(crate) fn stream_context_options(&self, id: i64) -> Option<RuntimeCellHandle> {
        self.stream_contexts.get(&id).and_then(|context| context.options)
    }

    /// Replaces persisted options for a stream context resource.
    pub(crate) fn set_stream_context_options(
        &mut self,
        id: i64,
        options: Option<RuntimeCellHandle>,
    ) -> bool {
        let Some(context) = self.stream_contexts.get_mut(&id) else {
            return false;
        };
        context.options = options;
        true
    }

    /// Reads one stream line up to a limit, newline, or custom delimiter.
    pub(crate) fn read_line(
        &mut self,
        id: i64,
        length: usize,
        ending: Option<&[u8]>,
        include_ending: bool,
        stop_at_newline: bool,
    ) -> Option<Vec<u8>> {
        let stream = self.streams.get_mut(&id)?;
        let mut output = Vec::new();
        let mut byte = [0_u8; 1];
        while output.len() < length {
            let read = stream.file.read(&mut byte).ok()?;
            if read == 0 {
                stream.eof = true;
                break;
            }
            output.push(byte[0]);
            if let Some(ending) = ending {
                if !ending.is_empty() && output.ends_with(ending) {
                    if !include_ending {
                        output.truncate(output.len().saturating_sub(ending.len()));
                    }
                    break;
                }
            } else if stop_at_newline && byte[0] == b'\n' {
                break;
            }
        }
        Some(output)
    }

    /// Writes all provided bytes to a stream resource and returns the written byte count.
    pub(crate) fn write(&mut self, id: i64, data: &[u8]) -> Option<usize> {
        let stream = self.streams.get_mut(&id)?;
        let written = stream.file.write(data).ok()?;
        stream.eof = false;
        Some(written)
    }

    /// Flushes buffered stream data to the host file handle.
    pub(crate) fn flush(&mut self, id: i64) -> bool {
        self.streams
            .get_mut(&id)
            .is_some_and(|stream| stream.file.flush().is_ok())
    }

    /// Returns whether a stream's file descriptor is attached to a terminal.
    pub(crate) fn isatty(&self, id: i64) -> Option<bool> {
        let stream = self.streams.get(&id)?;
        let result = unsafe {
            // libc only reads the descriptor value during the terminal probe.
            libc::isatty(stream.file.as_raw_fd())
        };
        Some(result == 1)
    }

    /// Toggles blocking mode on a stream's file descriptor.
    pub(crate) fn set_blocking(&self, id: i64, enable: bool) -> Option<bool> {
        let stream = self.streams.get(&id)?;
        let fd = stream.file.as_raw_fd();
        let flags = unsafe {
            // fcntl reads the current descriptor flags without taking ownership.
            libc::fcntl(fd, libc::F_GETFL)
        };
        if flags < 0 {
            return Some(false);
        }
        let flags = if enable {
            flags & !libc::O_NONBLOCK
        } else {
            flags | libc::O_NONBLOCK
        };
        let result = unsafe {
            // fcntl updates the descriptor flags in place.
            libc::fcntl(fd, libc::F_SETFL, flags)
        };
        Some(result == 0)
    }

    /// Reports timeout-setting support for local file streams.
    pub(crate) fn set_timeout(&self, id: i64, _seconds: i64, _microseconds: i64) -> Option<bool> {
        self.streams.get(&id).map(|_| false)
    }

    /// Stores a per-stream chunk size and returns the previous size.
    pub(crate) fn set_chunk_size(&mut self, id: i64, size: i64) -> Option<i64> {
        if !self.streams.contains_key(&id) || size <= 0 {
            return None;
        }
        Some(self.chunk_sizes.insert(id, size).unwrap_or(8192))
    }

    /// Accepts read/write buffer settings for local file streams.
    pub(crate) fn set_buffer(&self, id: i64, _size: i64) -> Option<i64> {
        self.streams.get(&id).map(|_| 0)
    }

    /// Applies an advisory lock operation to a stream's backing file descriptor.
    pub(crate) fn flock(&self, id: i64, operation: i64) -> Option<(bool, bool)> {
        let stream = self.streams.get(&id)?;
        let operation = eval_flock_operation(operation)?;
        let result = unsafe {
            // libc only observes the borrowed raw fd during this call.
            libc::flock(stream.file.as_raw_fd(), operation)
        };
        if result == 0 {
            Some((true, false))
        } else {
            Some((false, eval_flock_would_block()))
        }
    }

    /// Synchronizes stream data and metadata to storage.
    pub(crate) fn sync_all(&mut self, id: i64) -> bool {
        self.streams
            .get_mut(&id)
            .is_some_and(|stream| stream.file.sync_all().is_ok())
    }

    /// Synchronizes stream data to storage where the host platform supports it.
    pub(crate) fn sync_data(&mut self, id: i64) -> bool {
        self.streams
            .get_mut(&id)
            .is_some_and(|stream| stream.file.sync_data().is_ok())
    }

    /// Returns whether the stream has reached EOF after the last read attempt.
    pub(crate) fn eof(&self, id: i64) -> Option<bool> {
        self.streams.get(&id).map(|stream| stream.eof)
    }

    /// Returns the current stream cursor offset.
    pub(crate) fn tell(&mut self, id: i64) -> Option<u64> {
        self.streams.get_mut(&id)?.file.stream_position().ok()
    }

    /// Moves the stream cursor according to PHP `fseek()` whence values.
    pub(crate) fn seek(&mut self, id: i64, offset: i64, whence: i64) -> bool {
        let Some(stream) = self.streams.get_mut(&id) else {
            return false;
        };
        let position = match whence {
            0 => SeekFrom::Start(u64::try_from(offset).unwrap_or(u64::MAX)),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return false,
        };
        stream.eof = false;
        stream.file.seek(position).is_ok()
    }

    /// Rewinds a stream to the beginning.
    pub(crate) fn rewind(&mut self, id: i64) -> bool {
        self.seek(id, 0, 0)
    }

    /// Rewinds a directory resource to its first entry.
    pub(crate) fn rewind_directory(&mut self, id: i64) -> bool {
        self.directories
            .get_mut(&id)
            .is_some_and(EvalDirectoryStream::rewind)
    }

    /// Finalizes and removes an incremental hash context, returning raw digest bytes.
    pub(crate) fn finalize_hash_context(&mut self, id: i64) -> Option<Vec<u8>> {
        let context = self.hash_contexts.remove(&id)?;
        let mut output = [0_u8; 64];
        let len = unsafe {
            // elephc-crypto consumes and frees the owned context handle here.
            elephc_crypto::elephc_crypto_final(context.handle, output.as_mut_ptr())
        };
        eval_hash_digest_bytes(len, &output)
    }

    /// Clones an incremental hash context into a new resource id.
    pub(crate) fn copy_hash_context(&mut self, id: i64) -> Option<i64> {
        let context = self.hash_contexts.get(&id)?;
        let handle = unsafe {
            // elephc-crypto returns a deep clone with independent ownership.
            elephc_crypto::elephc_crypto_clone(context.handle)
        };
        if handle.is_null() {
            return None;
        }
        Some(self.insert_hash_context(EvalHashContext { handle }))
    }

    /// Truncates a stream to the requested byte length.
    pub(crate) fn truncate(&mut self, id: i64, size: u64) -> bool {
        self.streams
            .get_mut(&id)
            .is_some_and(|stream| stream.file.set_len(size).is_ok())
    }

    /// Returns host metadata for a stream's backing file handle.
    pub(crate) fn metadata(&self, id: i64) -> Option<Metadata> {
        self.streams
            .get(&id)
            .and_then(|stream| stream.file.metadata().ok())
    }

    /// Reads a full or bounded byte sequence from the stream, with optional offset seek.
    pub(crate) fn get_contents(
        &mut self,
        id: i64,
        length: Option<usize>,
        offset: Option<i64>,
    ) -> Option<Vec<u8>> {
        if let Some(offset) = offset {
            if !self.seek(id, offset, 0) {
                return None;
            }
        }
        match length {
            Some(length) => self.read(id, length),
            None => {
                let stream = self.streams.get_mut(&id)?;
                let mut bytes = Vec::new();
                stream.file.read_to_end(&mut bytes).ok()?;
                stream.eof = true;
                Some(bytes)
            }
        }
    }

    /// Copies bytes between two streams and returns the copied byte count.
    pub(crate) fn copy_to_stream(
        &mut self,
        from: i64,
        to: i64,
        length: Option<usize>,
        offset: Option<i64>,
    ) -> Option<usize> {
        let bytes = self.get_contents(from, length, offset)?;
        self.write(to, &bytes)
    }

    /// Returns metadata fields used by PHP `stream_get_meta_data()`.
    pub(crate) fn meta_data(&self, id: i64) -> Option<EvalStreamMetaData> {
        let stream = self.streams.get(&id)?;
        Some(EvalStreamMetaData {
            eof: stream.eof,
            mode: stream.mode.clone(),
            uri: stream.uri.clone(),
        })
    }

    /// Inserts a file stream and returns the assigned zero-based resource payload.
    fn insert(&mut self, stream: EvalFileStream) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.streams.insert(id, stream);
        id
    }

    /// Inserts a directory stream and returns the assigned zero-based resource payload.
    fn insert_directory(&mut self, directory: EvalDirectoryStream) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.directories.insert(id, directory);
        id
    }

    /// Inserts a hash context and returns the assigned zero-based resource payload.
    fn insert_hash_context(&mut self, context: EvalHashContext) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.hash_contexts.insert(id, context);
        id
    }

    /// Inserts a stream context and returns the assigned zero-based resource payload.
    fn insert_stream_context(&mut self, context: EvalStreamContext) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.stream_contexts.insert(id, context);
        id
    }
}

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

/// Converts PHP `LOCK_*` bit flags into host `flock()` flags.
fn eval_flock_operation(operation: i64) -> Option<libc::c_int> {
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
fn eval_flock_would_block() -> bool {
    let errno = std::io::Error::last_os_error().raw_os_error();
    errno.is_some_and(|code| code == libc::EWOULDBLOCK || code == libc::EAGAIN)
}

/// Converts an elephc-crypto digest length into owned raw bytes.
fn eval_hash_digest_bytes(len: isize, output: &[u8; 64]) -> Option<Vec<u8>> {
    let len = usize::try_from(len).ok()?;
    if len > output.len() {
        return None;
    }
    Some(output[..len].to_vec())
}

/// File stream stored behind one eval resource id.
struct EvalFileStream {
    file: File,
    uri: String,
    mode: String,
    eof: bool,
}

impl EvalFileStream {
    /// Creates a tracked stream around a host file handle.
    fn new(file: File, uri: String, mode: String) -> Self {
        Self {
            file,
            uri,
            mode,
            eof: false,
        }
    }
}

/// Directory stream stored behind one eval resource id.
struct EvalDirectoryStream {
    entries: Vec<String>,
    index: usize,
}

impl EvalDirectoryStream {
    /// Opens a local directory and snapshots its entry names.
    fn open(path: &str) -> Option<Self> {
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
    fn read(&mut self) -> Option<String> {
        let name = self.entries.get(self.index)?.clone();
        self.index += 1;
        Some(name)
    }

    /// Moves the directory cursor back to its first entry.
    fn rewind(&mut self) -> bool {
        self.index = 0;
        true
    }
}

/// Opaque elephc-crypto incremental hash context resource.
struct EvalHashContext {
    handle: *mut c_void,
}

/// Stream context metadata tracked by eval.
struct EvalStreamContext {
    options: Option<RuntimeCellHandle>,
}

/// Parsed PHP fopen mode used to configure `OpenOptions`.
struct EvalOpenMode {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    label: String,
}

impl EvalOpenMode {
    /// Parses PHP's common fopen mode grammar, ignoring binary/text markers.
    fn parse(mode: &str) -> Option<Self> {
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
    fn open(&self, path: &str) -> std::io::Result<File> {
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
fn eval_tmpfile_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "elephc-eval-tmpfile-{}-{}",
        std::process::id(),
        eval_tmpfile_nonce()
    ));
    path
}

/// Returns a monotonic-ish nonce for temporary file names.
fn eval_tmpfile_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}
