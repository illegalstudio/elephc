//! Purpose:
//! Allocates resource ids and inserts file, ephemeral, Phar, TCP, directory,
//! hash, and context storage entries.
//!
//! Called from:
//! - Resource-opening and registration modules in this tree.
//!
//! Key details:
//! - `take_next_id` remains the single monotonically increasing payload source, and
//!   the single place `EVAL_RESOURCE_PAYLOAD_BASE` is applied.

use super::*;

impl EvalStreamResources {
    /// Reserves the next eval-owned resource payload.
    ///
    /// Every eval resource of every kind — streams, directories, hash contexts,
    /// stream contexts, filters, user-wrapper handles — is numbered here, so the
    /// namespace base is applied exactly once. `next_id` stays a plain zero-based
    /// counter internally, which keeps `EvalStreamResources: Default` derivable;
    /// the base is added on the way out. Payloads are never reused: a closed eval
    /// stream's payload is not handed to the next one, which is what makes the
    /// runtime registry mint the NEXT id for it, exactly as PHP does.
    ///
    /// A PAYLOAD IS NOT A PHP RESOURCE ID, AND NOT EVERY PAYLOAD BECOMES ONE. Hash
    /// contexts are numbered from this same counter because they need a table slot, but
    /// PHP 8's `hash_init()` returns a `HashContext` OBJECT that consumes nothing from
    /// the resource counter. `hash_init`/`hash_copy` therefore box their key through
    /// `RuntimeValueOps::hash_context` (resource kind 5, id-less and destructor-less)
    /// rather than `RuntimeValueOps::resource`, so this counter advancing does not move
    /// the id the next `fopen()` reports. Every other opener here is a genuine PHP
    /// resource and must keep binding an id.
    pub(super) fn take_next_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        EVAL_RESOURCE_PAYLOAD_BASE + id
    }

    /// Inserts a file stream and returns the assigned resource payload.
    pub(super) fn insert(&mut self, stream: EvalFileStream) -> i64 {
        let id = self.take_next_id();
        self.streams.insert(id, stream);
        self.resource_types.insert(id, "stream");
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

    /// Inserts a directory stream and returns the assigned resource payload.
    pub(super) fn insert_directory(&mut self, directory: EvalDirectoryStream) -> i64 {
        let id = self.take_next_id();
        self.directories.insert(id, directory);
        self.resource_types.insert(id, "stream");
        id
    }

    /// Inserts a hash context and returns the assigned resource payload.
    pub(super) fn insert_hash_context(&mut self, context: EvalHashContext) -> i64 {
        let id = self.take_next_id();
        self.hash_contexts.insert(id, context);
        id
    }

    /// Inserts a stream context and returns the assigned resource payload.
    pub(super) fn insert_stream_context(&mut self, context: EvalStreamContext) -> i64 {
        let id = self.take_next_id();
        self.stream_contexts.insert(id, context);
        self.resource_types.insert(id, "stream-context");
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every eval payload must clear the runtime registry's standard-stream shortcut.
    ///
    /// `__rt_resource_id_of` answers payloads 0, 1 and 2 as `payload + 1` without
    /// consuming its counter, because those are STDIN/STDOUT/STDERR. Eval's first
    /// three payloads used to land exactly there, which is why `get_resource_id()`
    /// inside `eval()` reported 1, 2, 3 before jumping to the counter's 5.
    #[test]
    fn eval_payloads_clear_the_standard_stream_shortcut() {
        let mut resources = EvalStreamResources::default();
        for _ in 0..8 {
            assert!(resources.take_next_id() > 2);
        }
    }

    /// Eval payloads must be consecutive, never reused, and never negative.
    ///
    /// Consecutive-and-distinct is what keeps one registry slot per live eval
    /// resource; staying positive is what lets the payload round-trip through
    /// `raw_value_word()` and `i64::try_from` in `eval_resource_payload()`.
    #[test]
    fn eval_payloads_are_consecutive_and_positive() {
        let mut resources = EvalStreamResources::default();
        let first = resources.take_next_id();
        assert_eq!(first, EVAL_RESOURCE_PAYLOAD_BASE);
        assert!(first > 0);
        for offset in 1..16 {
            assert_eq!(resources.take_next_id(), EVAL_RESOURCE_PAYLOAD_BASE + offset);
        }
    }

    /// The base must sit above every host payload the registry can also be keyed by.
    ///
    /// Host payloads are file descriptors and user-space addresses. Canonical
    /// user-space addressing stops at 2^47 on x86_64 (2^56 with 5-level paging) and
    /// at 2^52 on AArch64, so the base has to clear 2^56 to be disjoint from both.
    /// It also has to leave room above itself for the counter: the headroom assertion
    /// is what keeps `EVAL_RESOURCE_PAYLOAD_BASE + n` from ever wrapping into the
    /// negatives, which would break `i64::try_from` in `eval_resource_payload()`.
    #[test]
    fn eval_payload_base_is_disjoint_from_host_payloads() {
        assert_eq!(EVAL_RESOURCE_PAYLOAD_BASE, 1 << 62);
        assert!(EVAL_RESOURCE_PAYLOAD_BASE > 1_i64 << 56);
        assert!(i64::MAX - EVAL_RESOURCE_PAYLOAD_BASE > 1_i64 << 61);
    }
}
