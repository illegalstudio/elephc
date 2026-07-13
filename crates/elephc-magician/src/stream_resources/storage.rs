//! Purpose:
//! Allocates resource ids and inserts file, ephemeral, Phar, TCP, directory,
//! hash, and context storage entries.
//!
//! Called from:
//! - Resource-opening and registration modules in this tree.
//!
//! Key details:
//! - `next_id` remains the single monotonically increasing id source.

use super::*;

impl EvalStreamResources {
    /// Inserts a file stream and returns the assigned zero-based resource payload.
    pub(super) fn insert(&mut self, stream: EvalFileStream) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.streams.insert(id, stream);
        id
    }

    /// Opens one unlinked temporary file as the backing storage for wrapper streams.
    pub(super) fn open_ephemeral_stream(
        &mut self,
        uri: &str,
        mode: &EvalOpenMode,
        initial: &[u8],
        flush_target: Option<EvalStreamFlushTarget>,
        append: bool,
    ) -> Option<i64> {
        let path = eval_tmpfile_path();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .ok()?;
        let _ = std::fs::remove_file(&path);
        file.write_all(initial).ok()?;
        if append {
            file.seek(SeekFrom::End(0)).ok()?;
        } else {
            file.seek(SeekFrom::Start(0)).ok()?;
        }
        Some(self.insert(EvalFileStream::new_with_flush_target(
            file,
            uri.to_string(),
            mode.label.clone(),
            flush_target,
        )))
    }

    /// Opens a `phar://` entry for reading or buffered write-back on close.
    pub(super) fn open_phar_stream(
        &mut self,
        path: &str,
        mode: &EvalOpenMode,
    ) -> Option<i64> {
        let url = path.as_bytes();
        if mode.write {
            let initial = if mode.truncate {
                Vec::new()
            } else {
                match elephc_phar::extract_url_bytes(url) {
                    Some(bytes) => bytes,
                    None if mode.create => Vec::new(),
                    None => return None,
                }
            };
            return self.open_ephemeral_stream(
                path,
                mode,
                &initial,
                Some(EvalStreamFlushTarget::PharUrl(url.to_vec())),
                mode.append,
            );
        }
        let bytes = elephc_phar::extract_url_bytes(url)?;
        self.open_ephemeral_stream(path, mode, &bytes, None, false)
    }

    /// Inserts a TCP stream as a File-backed eval stream and records endpoint names.
    pub(super) fn insert_tcp_stream(&mut self, stream: TcpStream) -> Option<i64> {
        let local = stream.local_addr().ok()?.to_string();
        let peer = stream.peer_addr().ok().map(|addr| addr.to_string());
        let file = unsafe {
            // The TcpStream is moved into the File-backed eval stream.
            File::from_raw_fd(stream.into_raw_fd())
        };
        let id = self.insert(EvalFileStream::new(file, local.clone(), "r+".to_string()));
        self.socket_names
            .insert(id, EvalSocketNames { local, peer });
        Some(id)
    }

    /// Inserts a directory stream and returns the assigned zero-based resource payload.
    pub(super) fn insert_directory(&mut self, directory: EvalDirectoryStream) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.directories.insert(id, directory);
        id
    }

    /// Inserts a hash context and returns the assigned zero-based resource payload.
    pub(super) fn insert_hash_context(&mut self, context: EvalHashContext) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.hash_contexts.insert(id, context);
        id
    }

    /// Inserts a stream context and returns the assigned zero-based resource payload.
    pub(super) fn insert_stream_context(&mut self, context: EvalStreamContext) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.stream_contexts.insert(id, context);
        id
    }
}
