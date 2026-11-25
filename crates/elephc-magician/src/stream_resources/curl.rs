//! Purpose:
//! Curl easy-handle storage for `EvalStreamResources` — the eval analog of
//! `resource_registration.rs`'s/`storage.rs`'s hash-context accessors, extended for
//! curl.
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

use crate::errors::EvalStatus;
use crate::interpreter::RuntimeValueOps;

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
                private_value: RuntimeOwnerSlot::default(),
                callbacks: Default::default(),
                callback_owners: std::array::from_fn(|_| RuntimeOwnerSlot::default()),
            },
        );
        id
    }

    /// Registers an already-allocated raw handle (a `curl_copy_handle()` duplicate) under
    /// a fresh eval table key, seeded with copied PHP-layer mirror fields. Mirrors
    /// `open_curl_easy_handle` without re-initializing the raw handle itself. Takes the
    /// fields rather than a whole `EvalCurlEasyHandle`, which stays private to this
    /// module — callers outside `crate::stream_resources` have no way to name that type.
    ///
    /// RETAINS `private_value` when present: the source handle's own entry keeps its
    /// existing already-retained reference, so the new
    /// entry needs an INDEPENDENT owned reference of its own. Without this, both entries
    /// would share one retained cell with only one owner recorded anywhere, and freeing
    /// either handle (or overwriting `CURLOPT_PRIVATE` on either) would release a
    /// reference the OTHER handle still points at.
    pub(crate) fn adopt_curl_easy_handle(
        &mut self,
        raw: i64,
        return_transfer: bool,
        write_user: bool,
        private_value: Option<RuntimeCellHandle>,
        values: &mut impl RuntimeValueOps,
    ) -> Result<i64, EvalStatus> {
        let private_value = match private_value {
            Some(value) => RuntimeOwnerSlot::new(values.retain(value)?),
            None => RuntimeOwnerSlot::default(),
        };
        let id = self.take_next_id();
        self.curl_easy_handles.insert(
            id,
            EvalCurlEasyHandle {
                raw,
                return_transfer,
                write_user,
                private_value,
                // NOT copied from the source handle: `elephc_curl_easy_duphandle` clears
                // every bridge registration on the duplicate (libcurl's own `dupset` would
                // otherwise carry the ORIGINAL's `CURLOPT_*DATA`, i.e. the original's id),
                // and `curl_copy_handle()` re-installs them onto the copy afterwards —
                // exactly the loop `crate::curl_prelude::curl_copy_handle` runs.
                callbacks: Default::default(),
                callback_owners: std::array::from_fn(|_| RuntimeOwnerSlot::default()),
            },
        );
        Ok(id)
    }

    /// Returns the bridge's raw easy-handle id for an eval table key.
    pub(crate) fn curl_easy_raw(&self, id: i64) -> Option<i64> {
        self.curl_easy_handles.get(&id).map(|handle| handle.raw)
    }

    /// Reverse lookup: the EVAL TABLE KEY owning a given bridge raw easy-handle id.
    ///
    /// `curl_multi_info_read()` needs it and nothing else does: the bridge reports a
    /// completed transfer by its own easy id (`INFO_FIELD_EASY_ID`), and eval addresses
    /// handles by table key. A linear scan is right here — the map holds one entry per
    /// `curl_init()` an eval fragment ever made, and this runs once per completion message.
    ///
    /// The mapping is unambiguous: every entry gets its raw id from a distinct
    /// `elephc_curl_easy_init()`/`elephc_curl_easy_duphandle()` call, and the bridge never
    /// reuses an id, so no two entries can share a raw.
    pub(crate) fn curl_easy_id_for_raw(&self, raw: i64) -> Option<i64> {
        self.curl_easy_handles
            .iter()
            .find(|(_, handle)| handle.raw == raw)
            .map(|(&id, _)| id)
    }

    /// Returns a shallow copy of the PHP-layer mirror fields for an eval table key, so
    /// `curl_copy_handle()` can seed the duplicate without holding two borrows at once.
    pub(crate) fn curl_easy_mirror(&self, id: i64) -> Option<(bool, bool, Option<RuntimeCellHandle>)> {
        self.curl_easy_handles
            .get(&id)
            .map(|handle| (
                handle.return_transfer,
                handle.write_user,
                (!handle.private_value.is_empty()).then(|| handle.private_value.borrowed()),
            ))
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
            .filter(|handle| !handle.private_value.is_empty())
            .map(|handle| handle.private_value.borrowed())
    }

    /// Publishes an already-retained `CURLOPT_PRIVATE` owner and detaches its predecessor.
    ///
    /// The interpreter releases the returned batch after this table borrow ends, so a
    /// destructor cannot re-enter while the preceding owner is still visible here. An
    /// unknown id returns the incoming owner in the detached batch for the same reason.
    pub(crate) fn replace_curl_easy_private(
        &mut self,
        id: i64,
        retained: RuntimeCellHandle,
    ) -> (bool, Vec<RuntimeCellHandle>) {
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            return (false, vec![retained]);
        };
        let previous = handle.private_value.replace(Some(retained));
        (true, previous.into_iter().collect())
    }

    /// Resets the PHP-layer mirror and detaches every PHP owner in one non-reentrant step.
    ///
    /// The raw bridge id stays untouched because `elephc_curl_easy_reset` already reset
    /// libcurl. The caller releases the detached private value and callback roots after
    /// this table borrow ends.
    pub(crate) fn take_curl_easy_reset_owners(
        &mut self,
        id: i64,
    ) -> Option<Vec<RuntimeCellHandle>> {
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            return None;
        };
        handle.return_transfer = false;
        handle.write_user = false;
        Some(Self::take_curl_easy_handle_php_owners(handle))
    }

    /// Detaches every PHP cell owned by one easy-handle record before cleanup can re-enter.
    fn take_curl_easy_handle_php_owners(handle: &mut EvalCurlEasyHandle) -> Vec<RuntimeCellHandle> {
        let mut owners = Vec::with_capacity(1 + handle.callback_owners.len());
        if let Some(private) = handle.private_value.take_optional() {
            owners.push(private);
        }
        for (state, slot) in handle.callbacks.iter_mut().zip(&handle.callback_owners) {
            *state = EvalCurlCallbackKind::Empty;
            if let Some(owner) = slot.take_optional() {
                owners.push(owner);
            }
        }
        owners
    }

    /// Detaches every easy-handle PHP owner while retaining the native handles for `Drop`.
    pub(super) fn take_curl_easy_php_owners(&mut self) -> Vec<RuntimeCellHandle> {
        let mut owners = Vec::new();
        for handle in self.curl_easy_handles.values_mut() {
            owners.extend(Self::take_curl_easy_handle_php_owners(handle));
        }
        owners
    }

    /// Test adapter that detaches and attempts every easy-handle PHP owner release.
    #[cfg(test)]
    pub(crate) fn release_curl_easy_php_values(&mut self, values: &mut impl RuntimeValueOps) {
        let owners = self.take_curl_easy_php_owners();
        let _ = elephc_memoir::runtime_owners::release_all(
            owners,
            elephc_memoir::runtime_owners::ReleaseOrder::Forward,
            elephc_memoir::runtime_owners::CleanupErrorPolicy::Ignore,
            |owner| values.release(owner),
        );
    }

    /// Publishes an already-retained callback state and detaches the preceding owner.
    ///
    /// An unknown handle or slot returns any incoming callable in the detached batch, so
    /// the interpreter can release it after the table borrow ends.
    pub(crate) fn replace_curl_easy_callback(
        &mut self,
        id: i64,
        slot: usize,
        value: EvalCurlCallbackSlot,
    ) -> (bool, Vec<RuntimeCellHandle>) {
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            let detached = match value {
                EvalCurlCallbackSlot::Callable(cell) => vec![cell],
                _ => Vec::new(),
            };
            return (false, detached);
        };
        let Some(current) = handle.callbacks.get_mut(slot) else {
            let detached = match value {
                EvalCurlCallbackSlot::Callable(cell) => vec![cell],
                _ => Vec::new(),
            };
            return (false, detached);
        };
        let owner = handle.callback_owners.get(slot).expect("callback state and owner arrays match");
        let (replacement, kind) = match value {
            EvalCurlCallbackSlot::Empty => (None, EvalCurlCallbackKind::Empty),
            EvalCurlCallbackSlot::Silent => (None, EvalCurlCallbackKind::Silent),
            EvalCurlCallbackSlot::Callable(cell) => (Some(cell), EvalCurlCallbackKind::Callable),
        };
        *current = kind;
        let detached = owner.replace(replacement).into_iter().collect();
        (true, detached)
    }

    /// Detaches every callback owner on one handle before any release can re-enter.
    #[cfg(test)]
    pub(crate) fn take_curl_easy_callbacks(
        &mut self,
        id: i64,
    ) -> Vec<RuntimeCellHandle> {
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            return Vec::new();
        };
        let mut owners = Vec::new();
        for (state, slot) in handle.callbacks.iter_mut().zip(&handle.callback_owners) {
            *state = EvalCurlCallbackKind::Empty;
            if let Some(cell) = slot.take_optional() {
                owners.push(cell);
            }
        }
        owners
    }

    /// Returns every non-`Empty` callback slot on one handle as `(slot index, state)`, so
    /// `curl_exec()`/`curl_multi_exec()` can build their invocation frame and
    /// `curl_copy_handle()` can re-install the set onto the duplicate — neither of which can
    /// hold a borrow on this table while it calls back into the interpreter.
    pub(crate) fn curl_easy_callbacks(&self, id: i64) -> Vec<(usize, EvalCurlCallbackSlot)> {
        let Some(handle) = self.curl_easy_handles.get(&id) else {
            return Vec::new();
        };
        handle
            .callbacks
            .iter()
            .enumerate()
            .filter(|(_, state)| **state != EvalCurlCallbackKind::Empty)
            .map(|(index, state)| {
                let state = match *state {
                    EvalCurlCallbackKind::Empty => EvalCurlCallbackSlot::Empty,
                    EvalCurlCallbackKind::Silent => EvalCurlCallbackSlot::Silent,
                    EvalCurlCallbackKind::Callable => EvalCurlCallbackSlot::Callable(
                        handle.callback_owners[index].borrowed(),
                    ),
                };
                (index, state)
            })
            .collect()
    }

    /// Exercises private-value publication with the historical test adapter.
    #[cfg(test)]
    pub(crate) fn set_curl_easy_private(
        &mut self,
        id: i64,
        value: RuntimeCellHandle,
        values: &mut impl RuntimeValueOps,
    ) -> Result<bool, EvalStatus> {
        let retained = values.retain(value)?;
        let (published, detached) = self.replace_curl_easy_private(id, retained);
        elephc_memoir::runtime_owners::release_all(
            detached,
            elephc_memoir::runtime_owners::ReleaseOrder::Forward,
            elephc_memoir::runtime_owners::CleanupErrorPolicy::First,
            |owner| values.release(owner),
        )?;
        Ok(published)
    }

    /// Exercises reset detachment with the historical test adapter.
    #[cfg(test)]
    pub(crate) fn reset_curl_easy_mirror(
        &mut self,
        id: i64,
        values: &mut impl RuntimeValueOps,
    ) -> Result<bool, EvalStatus> {
        let Some(detached) = self.take_curl_easy_reset_owners(id) else {
            return Ok(false);
        };
        elephc_memoir::runtime_owners::release_all(
            detached,
            elephc_memoir::runtime_owners::ReleaseOrder::Forward,
            elephc_memoir::runtime_owners::CleanupErrorPolicy::First,
            |owner| values.release(owner),
        )?;
        Ok(true)
    }

    /// Exercises callback publication with the historical test adapter.
    #[cfg(test)]
    pub(crate) fn set_curl_easy_callback(
        &mut self,
        id: i64,
        slot: usize,
        value: EvalCurlCallbackSlot,
        values: &mut impl RuntimeValueOps,
    ) -> Result<bool, EvalStatus> {
        let value = match value {
            EvalCurlCallbackSlot::Callable(cell) => {
                EvalCurlCallbackSlot::Callable(values.retain(cell)?)
            }
            other => other,
        };
        let (published, detached) = self.replace_curl_easy_callback(id, slot, value);
        elephc_memoir::runtime_owners::release_all(
            detached,
            elephc_memoir::runtime_owners::ReleaseOrder::Forward,
            elephc_memoir::runtime_owners::CleanupErrorPolicy::First,
            |owner| values.release(owner),
        )?;
        Ok(published)
    }

    /// Exercises callback detachment with the historical test adapter.
    #[cfg(test)]
    pub(crate) fn clear_curl_easy_callbacks(
        &mut self,
        id: i64,
        values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        let detached = self.take_curl_easy_callbacks(id);
        elephc_memoir::runtime_owners::release_all(
            detached,
            elephc_memoir::runtime_owners::ReleaseOrder::Forward,
            elephc_memoir::runtime_owners::CleanupErrorPolicy::First,
            |owner| values.release(owner),
        )
    }

    /// Registers a freshly allocated curl MULTI handle and returns its eval table key.
    pub(crate) fn open_curl_multi_handle(&mut self, raw: i64) -> i64 {
        let id = self.take_next_id();
        self.curl_multi_handles.insert(
            id,
            EvalCurlMultiHandle {
                raw,
                attached: Vec::new(),
            },
        );
        id
    }

    /// Returns the bridge's raw multi-handle id for an eval table key.
    pub(crate) fn curl_multi_raw(&self, id: i64) -> Option<i64> {
        self.curl_multi_handles.get(&id).map(|handle| handle.raw)
    }

    /// Records an easy handle's eval table key on a multi handle's add-order list, matching
    /// `crate::curl_prelude::CurlMultiHandle::__elephc_attach`. Callers must invoke this
    /// ONLY after the bridge answered `CURLM_OK`, so a refused attach leaves the list
    /// exactly as it was — otherwise `curl_multi_get_handles()` would list a handle libcurl
    /// never took. Idempotent for an already-listed key.
    pub(crate) fn attach_curl_multi_easy(&mut self, multi_id: i64, easy_id: i64) {
        if let Some(handle) = self.curl_multi_handles.get_mut(&multi_id) {
            if !handle.attached.contains(&easy_id) {
                handle.attached.push(easy_id);
            }
        }
    }

    /// Removes an easy handle's eval table key from a multi handle's add-order list,
    /// matching `crate::curl_prelude::CurlMultiHandle::__elephc_detach`.
    pub(crate) fn detach_curl_multi_easy(&mut self, multi_id: i64, easy_id: i64) {
        if let Some(handle) = self.curl_multi_handles.get_mut(&multi_id) {
            handle.attached.retain(|&id| id != easy_id);
        }
    }

    /// Returns a multi handle's attached easy-handle table keys in add order — the list
    /// PHP 8.5's `curl_multi_get_handles()` reports and `curl_multi_info_read()` resolves
    /// its completion message against.
    pub(crate) fn curl_multi_attached(&self, multi_id: i64) -> Option<Vec<i64>> {
        self.curl_multi_handles
            .get(&multi_id)
            .map(|handle| handle.attached.clone())
    }

    /// Registers a freshly allocated curl SHARE handle and returns its eval table key.
    /// `persistent` marks a share minted by PHP 8.5's `curl_share_init_persistent()`.
    pub(crate) fn open_curl_share_handle(&mut self, raw: i64, persistent: bool) -> i64 {
        let id = self.take_next_id();
        self.curl_share_handles
            .insert(id, EvalCurlShareHandle { raw, persistent });
        id
    }

    /// Returns the bridge's raw share-handle id for an eval table key.
    pub(crate) fn curl_share_raw(&self, id: i64) -> Option<i64> {
        self.curl_share_handles.get(&id).map(|handle| handle.raw)
    }

    /// Returns whether an eval table key names a PERSISTENT share (PHP 8.5's
    /// `CurlSharePersistentHandle`), which `curl_share_setopt()`/`curl_share_errno()`/
    /// `curl_share_close()` must refuse exactly as their AOT `CurlShareHandle` parameter
    /// type does — php-src does not make the persistent class a subclass.
    pub(crate) fn curl_share_is_persistent(&self, id: i64) -> bool {
        self.curl_share_handles
            .get(&id)
            .is_some_and(|handle| handle.persistent)
    }
}
