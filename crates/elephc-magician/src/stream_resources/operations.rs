//! Purpose:
//! Performs close, read/write, positioning, metadata, synchronization, hash,
//! socket, directory, context, filter, and copy operations on resource ids.
//!
//! Called from:
//! - Stream builtins after a resource id has been resolved.
//!
//! Key details:
//! - Each operation checks the concrete resource table and preserves existing
//!   false/none behavior for incompatible ids.

use super::*;

impl EvalStreamResources {

    /// Removes a stream resource from the table, closing its file handle.
    pub(crate) fn close(&mut self, id: i64) -> bool {
        let mut closed = false;
        let mut ok = true;
        if let Some(stream) = self.streams.remove(&id) {
            closed = true;
            ok = stream.finalize_on_close();
        }
        closed = closed
            || self.user_wrapper_streams.remove(&id).is_some()
            || self.filter_resources.remove(&id)
            || self.socket_listeners.remove(&id).is_some();
        self.socket_names.remove(&id);
        if let Some(mut child) = self.process_children.remove(&id) {
            let _ = child.wait();
        }
        closed && ok
    }

    /// Returns whether a file-like stream resource exists.
    pub(crate) fn has_stream(&self, id: i64) -> bool {
        self.streams.contains_key(&id) || self.user_wrapper_streams.contains_key(&id)
    }

    /// Returns whether an eval-owned resource payload is still OPEN.
    ///
    /// Eval-created resources carry no close sentinel: their payload IS the key of these
    /// tables (`EVAL_RESOURCE_PAYLOAD_BASE + n`), so negating it the way the compiled side
    /// does would break every builtin that later resolves the handle. Their close state
    /// therefore lives in the tables instead — `close()`, `pclose()` and
    /// `close_directory()` REMOVE the entry, and `take_next_id()` never reuses a payload —
    /// so "handed out and now in no table" is exactly "closed". That is what lets
    /// `var_dump()` and `get_resource_type()` rename a closed eval resource to `Unknown`,
    /// as PHP 8.5.6 does.
    ///
    /// THE PREDICATE IS "CLOSED UNLESS FOUND", so a table forgotten here would report a
    /// LIVE resource as closed. The destructure below is therefore exhaustive and names
    /// every field explicitly instead of sweeping the rest with `..`: adding a resource
    /// table to `EvalStreamResources` must break this function's compile rather than
    /// silently mislabel the resources it holds.
    ///
    /// `stream_contexts` and `hash_contexts` are consulted even though no close path ever
    /// removes them, which keeps them permanently live — the behaviour eval `hash_init()`
    /// has today, where the interpreter still hands back a resource although the compiled
    /// side models a `HashContext` as an object. `curl_easy_handles` (curl feature only)
    /// is the same shape for the same reason: `curl_close()` is a documented PHP 8 no-op,
    /// so a curl handle id never leaves this table before `EvalStreamResources::drop`.
    pub(crate) fn is_live(&self, id: i64) -> bool {
        let EvalStreamResources {
            chunk_sizes: _,
            default_stream_context: _,
            disabled_builtin_stream_wrappers: _,
            next_id: _,
            socket_names: _,
            user_stream_wrapper_classes: _,
            user_stream_wrappers: _,
            directories,
            filter_resources,
            hash_contexts,
            process_children,
            socket_listeners,
            stream_contexts,
            streams,
            user_wrapper_directories,
            user_wrapper_streams,
            #[cfg(feature = "curl")]
            curl_easy_handles,
            #[cfg(feature = "curl")]
            curl_multi_handles,
            #[cfg(feature = "curl")]
            curl_share_handles,
        } = self;
        streams.contains_key(&id)
            || user_wrapper_streams.contains_key(&id)
            || directories.contains_key(&id)
            || user_wrapper_directories.contains_key(&id)
            || filter_resources.contains(&id)
            || socket_listeners.contains_key(&id)
            || hash_contexts.contains_key(&id)
            || stream_contexts.contains_key(&id)
            || process_children.contains_key(&id)
            || {
                #[cfg(feature = "curl")]
                {
                    curl_easy_handles.contains_key(&id)
                        || curl_multi_handles.contains_key(&id)
                        || curl_share_handles.contains_key(&id)
                }
                #[cfg(not(feature = "curl"))]
                {
                    false
                }
            }
    }

    /// Returns a local or remote socket name for a socket resource.
    pub(crate) fn socket_name(&self, id: i64, remote: bool) -> Option<String> {
        let names = self.socket_names.get(&id)?;
        if remote {
            names.peer.clone()
        } else {
            Some(names.local.clone())
        }
    }

    /// Applies a TCP/Unix stream shutdown operation.
    pub(crate) fn socket_shutdown(&self, id: i64, mode: i64) -> Option<bool> {
        let stream = self.streams.get(&id)?;
        let shutdown = match mode {
            0 => Shutdown::Read,
            1 => Shutdown::Write,
            2 => Shutdown::Both,
            _ => return Some(false),
        };
        let result = unsafe {
            // libc shutdown only observes the borrowed descriptor and mode.
            libc::shutdown(stream.file.as_raw_fd(), eval_shutdown_how(shutdown))
        };
        Some(result == 0)
    }

    /// Allocates an eval-local stream filter resource handle.
    pub(crate) fn open_filter_resource(&mut self) -> i64 {
        let id = self.take_next_id();
        self.filter_resources.insert(id);
        id
    }

    /// Removes an eval-local stream filter resource handle.
    pub(crate) fn close_filter_resource(&mut self, id: i64) -> bool {
        self.filter_resources.remove(&id)
    }

    /// Closes a process pipe stream and returns the child exit status.
    pub(crate) fn pclose(&mut self, id: i64) -> Option<i64> {
        let mut child = self.process_children.remove(&id)?;
        self.streams.remove(&id)?;
        let status = child.wait().ok()?;
        Some(status.code().unwrap_or(0) as i64)
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
        self.streams
            .get(&id)
            .map(|stream| stream.eof)
            .or_else(|| self.user_wrapper_streams.get(&id).map(|stream| stream.eof))
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
        if let Some(stream) = self.streams.get(&id) {
            return Some(EvalStreamMetaData {
                eof: stream.eof,
                mode: stream.mode.clone(),
                uri: stream.uri.clone(),
            });
        }
        let stream = self.user_wrapper_streams.get(&id)?;
        Some(EvalStreamMetaData {
            eof: stream.eof,
            mode: stream.mode.clone(),
            uri: stream.uri.clone(),
        })
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload that was never handed out is not live, and one that was is — until the
    /// matching close removes it.
    ///
    /// This is the predicate `var_dump()` and `get_resource_type()` invert to decide
    /// between `stream` and `Unknown`, so both directions matter: an always-true answer
    /// would keep the pre-fix behaviour, and an always-false one would rename every LIVE
    /// eval resource to `Unknown`.
    #[test]
    fn a_filter_resource_is_live_only_between_its_open_and_close() {
        let mut resources = EvalStreamResources::default();
        let unused = resources.take_next_id();
        assert!(!resources.is_live(unused));

        let id = resources.open_filter_resource();
        assert!(resources.is_live(id));
        assert!(resources.close_filter_resource(id));
        assert!(!resources.is_live(id));
    }

    /// A payload from outside the eval namespace is never live here.
    ///
    /// Host resources are keyed by file descriptors and allocator handles and live in the
    /// compiled program, not in these tables, so `is_live` must not be consulted for them
    /// — `eval_resource_is_closed` gates on `EVAL_RESOURCE_PAYLOAD_BASE` for exactly that
    /// reason. This pins the shape of the answer if the gate is ever removed by accident.
    #[test]
    fn a_host_payload_is_never_live_in_the_eval_tables() {
        let resources = EvalStreamResources::default();
        assert!(!resources.is_live(3));
        assert!(!resources.is_live(0));
    }

    /// A stream context stays live forever, because no close path removes one.
    ///
    /// `is_live` is a "closed unless found" test, so a table left out of it would report
    /// its live resources as `Unknown`. `stream_contexts` and `hash_contexts` are the two
    /// tables with no remover at all, which makes them the most likely to be forgotten.
    #[test]
    fn a_stream_context_stays_live_because_nothing_closes_one() {
        let mut resources = EvalStreamResources::default();
        let id = resources.insert_stream_context(EvalStreamContext { options: None });
        assert!(resources.is_live(id));
    }
}
