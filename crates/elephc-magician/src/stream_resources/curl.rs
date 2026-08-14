//! Purpose:
//! Curl easy-handle storage for `EvalStreamResources` — the eval analog of
//! `resource_registration.rs`'s/`storage.rs`'s hash-context accessors, extended for
//! curl per Task 13 of the php-curl-family plan.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl`'s home files.
//!
//! Key details:
//! - Feature-gated as a whole (see `crate::stream_resources`'s `mod curl` declaration):
//!   this file does not exist in a `libelephc_magician.a` built without `--features curl`.
//! - Ids are drawn from the SAME `take_next_id()` counter every other eval-owned resource
//!   uses, so a curl handle id can never collide with a hash-context, directory, or
//!   stream-context id in the same table generation — see `EvalCurlEasyHandle`'s own doc.

use super::*;

impl EvalStreamResources {
    /// Registers a freshly allocated curl easy handle and returns its eval table key
    /// (NOT the bridge's own raw id — callers box THIS key through
    /// `RuntimeValueOps::curl_handle`).
    pub(crate) fn open_curl_easy_handle(&mut self, raw: i64) -> i64 {
        let id = self.take_next_id();
        self.curl_easy_handles.insert(
            id,
            EvalCurlEasyHandle {
                raw,
                return_transfer: false,
                write_user: false,
                private_value: None,
            },
        );
        id
    }

    /// Registers an already-allocated raw handle (a `curl_copy_handle()` duplicate) under
    /// a fresh eval table key, seeded with copied PHP-layer mirror fields. Mirrors
    /// `open_curl_easy_handle` without re-initializing the raw handle itself. Takes the
    /// fields rather than a whole `EvalCurlEasyHandle`, which stays private to this
    /// module — callers outside `crate::stream_resources` have no way to name that type.
    pub(crate) fn adopt_curl_easy_handle(
        &mut self,
        raw: i64,
        return_transfer: bool,
        write_user: bool,
        private_value: Option<RuntimeCellHandle>,
    ) -> i64 {
        let id = self.take_next_id();
        self.curl_easy_handles.insert(
            id,
            EvalCurlEasyHandle {
                raw,
                return_transfer,
                write_user,
                private_value,
            },
        );
        id
    }

    /// Returns the bridge's raw easy-handle id for an eval table key.
    pub(crate) fn curl_easy_raw(&self, id: i64) -> Option<i64> {
        self.curl_easy_handles.get(&id).map(|handle| handle.raw)
    }

    /// Returns a shallow copy of the PHP-layer mirror fields for an eval table key, so
    /// `curl_copy_handle()` can seed the duplicate without holding two borrows at once.
    pub(crate) fn curl_easy_mirror(&self, id: i64) -> Option<(bool, bool, Option<RuntimeCellHandle>)> {
        self.curl_easy_handles
            .get(&id)
            .map(|handle| (handle.return_transfer, handle.write_user, handle.private_value))
    }

    /// Returns whether `RETURNTRANSFER` is active on this handle (`curl_exec()`'s
    /// return-shape decision). `false` for an unknown id.
    pub(crate) fn curl_easy_return_transfer(&self, id: i64) -> bool {
        self.curl_easy_handles
            .get(&id)
            .is_some_and(|handle| handle.return_transfer)
    }

    /// Sets the `RETURNTRANSFER`/`write_user` mirror flags together, matching
    /// `curl_setopt()`'s "installing a write callback deselects RETURNTRANSFER and vice
    /// versa" rule (`crate::curl_prelude`'s header). Returns `false` for an unknown id.
    pub(crate) fn set_curl_easy_write_mode(
        &mut self,
        id: i64,
        return_transfer: bool,
        write_user: bool,
    ) -> bool {
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            return false;
        };
        handle.return_transfer = return_transfer;
        handle.write_user = write_user;
        true
    }

    /// Returns the stored `CURLOPT_PRIVATE` value, if any was ever set.
    pub(crate) fn curl_easy_private(&self, id: i64) -> Option<RuntimeCellHandle> {
        self.curl_easy_handles
            .get(&id)
            .and_then(|handle| handle.private_value)
    }

    /// Stores a `CURLOPT_PRIVATE` value. Returns `false` for an unknown id.
    pub(crate) fn set_curl_easy_private(&mut self, id: i64, value: RuntimeCellHandle) -> bool {
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            return false;
        };
        handle.private_value = Some(value);
        true
    }

    /// Resets the PHP-layer mirror fields to `curl_reset()`'s fresh-handle defaults,
    /// leaving the raw bridge id untouched (the bridge's own `elephc_curl_easy_reset`
    /// already reset libcurl's own options).
    pub(crate) fn reset_curl_easy_mirror(&mut self, id: i64) -> bool {
        self.set_curl_easy_write_mode(id, false, false);
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            return false;
        };
        handle.private_value = None;
        true
    }
}
