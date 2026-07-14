//! Purpose:
//! Registers directories, incremental hashes, stream contexts, filters, and
//! user-defined stream wrappers in the eval resource table.
//!
//! Called from:
//! - Directory, hash, context, and userspace-wrapper builtins.
//!
//! Key details:
//! - Wrapper protocol names are normalized before registry mutation.

use super::*;

impl EvalStreamResources {

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

    /// Registers a user stream wrapper protocol and class in eval-local state.
    pub(crate) fn register_stream_wrapper(
        &mut self,
        protocol: &str,
        class_name: &str,
        builtins: &[&str],
    ) -> bool {
        let Some(protocol) = eval_normalize_stream_wrapper_protocol(protocol) else {
            return false;
        };
        if self
            .user_stream_wrappers
            .iter()
            .any(|current| current.eq_ignore_ascii_case(&protocol))
        {
            return false;
        }
        if eval_builtin_stream_wrapper_exists(builtins, &protocol)
            && !self.disabled_builtin_stream_wrappers.contains(&protocol)
        {
            return false;
        }
        self.user_stream_wrapper_classes
            .insert(protocol.clone(), class_name.to_string());
        self.user_stream_wrappers.push(protocol);
        true
    }

    /// Unregisters a user or built-in stream wrapper protocol.
    pub(crate) fn unregister_stream_wrapper(&mut self, protocol: &str, builtins: &[&str]) -> bool {
        let Some(protocol) = eval_normalize_stream_wrapper_protocol(protocol) else {
            return false;
        };
        if let Some(index) = self
            .user_stream_wrappers
            .iter()
            .position(|current| current.eq_ignore_ascii_case(&protocol))
        {
            let protocol = self.user_stream_wrappers.remove(index);
            self.user_stream_wrapper_classes.remove(&protocol);
            return true;
        }
        if eval_builtin_stream_wrapper_exists(builtins, &protocol) {
            return self.disabled_builtin_stream_wrappers.insert(protocol);
        }
        false
    }

    /// Restores a built-in stream wrapper protocol or accepts no-op user restores.
    pub(crate) fn restore_stream_wrapper(&mut self, protocol: &str, builtins: &[&str]) -> bool {
        let Some(protocol) = eval_normalize_stream_wrapper_protocol(protocol) else {
            return false;
        };
        if eval_builtin_stream_wrapper_exists(builtins, &protocol) {
            self.disabled_builtin_stream_wrappers.remove(&protocol);
        }
        true
    }

    /// Returns the currently visible stream wrapper protocol list.
    pub(crate) fn stream_wrappers(&self, builtins: &[&str]) -> Vec<String> {
        let mut wrappers = Vec::with_capacity(builtins.len() + self.user_stream_wrappers.len());
        for builtin in builtins {
            if !self.disabled_builtin_stream_wrappers.contains(*builtin) {
                wrappers.push((*builtin).to_string());
            }
        }
        wrappers.extend(self.user_stream_wrappers.iter().cloned());
        wrappers
    }

    /// Returns the registered userspace wrapper class for a URL scheme.
    pub(crate) fn user_stream_wrapper_class_for_path(&self, path: &str) -> Option<String> {
        let scheme = stream_wrappers::stream_scheme(path)?;
        self.user_stream_wrapper_classes.get(&scheme).cloned()
    }

    /// Opens a userspace wrapper stream around an eval-created wrapper object.
    pub(crate) fn open_user_wrapper_stream(
        &mut self,
        object: RuntimeCellHandle,
        class_name: &str,
        uri: &str,
        mode: &str,
    ) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.user_wrapper_streams.insert(
            id,
            EvalUserWrapperStream {
                object,
                class_name: class_name.to_string(),
                uri: uri.to_string(),
                mode: mode.to_string(),
                eof: false,
            },
        );
        id
    }

    /// Opens a userspace wrapper directory around an eval-created wrapper object.
    pub(crate) fn open_user_wrapper_directory(
        &mut self,
        object: RuntimeCellHandle,
        class_name: &str,
    ) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.user_wrapper_directories.insert(
            id,
            EvalUserWrapperDirectory {
                object,
                class_name: class_name.to_string(),
            },
        );
        id
    }

    /// Returns a copied userspace-wrapper stream descriptor for dispatch.
    pub(crate) fn user_wrapper_stream_info(
        &self,
        id: i64,
    ) -> Option<EvalUserWrapperStreamInfo> {
        self.user_wrapper_streams
            .get(&id)
            .map(EvalUserWrapperStream::info)
    }

    /// Returns a copied userspace-wrapper directory descriptor for dispatch.
    pub(crate) fn user_wrapper_directory_info(
        &self,
        id: i64,
    ) -> Option<EvalUserWrapperDirectoryInfo> {
        self.user_wrapper_directories
            .get(&id)
            .map(EvalUserWrapperDirectory::info)
    }

    /// Removes a userspace-wrapper directory and returns its descriptor.
    pub(crate) fn close_user_wrapper_directory(
        &mut self,
        id: i64,
    ) -> Option<EvalUserWrapperDirectoryInfo> {
        self.user_wrapper_directories
            .remove(&id)
            .map(|directory| directory.info())
    }

    /// Updates the cached EOF state for a userspace-wrapper stream.
    pub(crate) fn set_user_wrapper_eof(&mut self, id: i64, eof: bool) -> bool {
        let Some(stream) = self.user_wrapper_streams.get_mut(&id) else {
            return false;
        };
        stream.eof = eof;
        true
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

}
