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
                private_value: None,
                callbacks: Default::default(),
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
    /// existing (already-retained — see `set_curl_easy_private`) reference, so the new
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
            Some(value) => Some(values.retain(value)?),
            None => None,
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

    /// Stores a `CURLOPT_PRIVATE` value, RETAINING it so this table holds an independent
    /// owned reference — not the caller's borrowed scope cell, which `unset()`/reassignment
    /// can release out from under a stored-but-unretained handle (the exact use-after-free
    /// this method used to allow: `curl_setopt($ch, CURLOPT_PRIVATE, $k); unset($k);
    /// curl_getinfo($ch, CURLINFO_PRIVATE);` retained a freed cell). Releases any
    /// PREVIOUSLY stored value first, matching PHP's own property-reassignment semantics
    /// (`crate::curl_prelude`'s `$handle->__elephc_private = $value;` drops the old
    /// reference the same way) and preventing an unbounded retain leak across repeated
    /// `curl_setopt($ch, CURLOPT_PRIVATE, ...)` calls on one handle. Returns `false` (after
    /// releasing the just-retained value, so nothing leaks) for an unknown id.
    pub(crate) fn set_curl_easy_private(
        &mut self,
        id: i64,
        value: RuntimeCellHandle,
        values: &mut impl RuntimeValueOps,
    ) -> Result<bool, EvalStatus> {
        let retained = values.retain(value)?;
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            values.release(retained)?;
            return Ok(false);
        };
        if let Some(previous) = handle.private_value.replace(retained) {
            values.release(previous)?;
        }
        Ok(true)
    }

    /// Resets the PHP-layer mirror fields to `curl_reset()`'s fresh-handle defaults,
    /// leaving the raw bridge id untouched (the bridge's own `elephc_curl_easy_reset`
    /// already reset libcurl's own options). Releases the stored `CURLOPT_PRIVATE` value
    /// (if any) before clearing it — the same reference `set_curl_easy_private` retained on
    /// the way in, mirroring `crate::curl_prelude::curl_reset`'s
    /// `$handle->__elephc_private = false;`.
    pub(crate) fn reset_curl_easy_mirror(
        &mut self,
        id: i64,
        values: &mut impl RuntimeValueOps,
    ) -> Result<bool, EvalStatus> {
        self.set_curl_easy_write_mode(id, false, false);
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            return Ok(false);
        };
        if let Some(previous) = handle.private_value.take() {
            values.release(previous)?;
        }
        Ok(true)
    }

    /// Releases every retained `CURLOPT_PRIVATE` value this table still holds, leaving the
    /// raw bridge handles themselves untouched — `EvalStreamResources`'s `Drop` impl, which
    /// runs immediately after this at the SAME teardown, still frees those the way it
    /// always has.
    ///
    /// WHY THIS EXISTS: `set_curl_easy_private`
    /// retains an independent owned reference for every stored `CURLOPT_PRIVATE` value
    /// (this file's own doc), and until this method existed nothing ever released that
    /// reference for a handle that was simply never explicitly reset/overwritten —
    /// `Drop for EvalStreamResources` frees the RAW bridge handle
    /// (`crate::curl_ffi::easy_free`) but cannot also call `RuntimeValueOps::release`,
    /// because `Drop::drop` receives only `&mut self`, never a `RuntimeValueOps`
    /// implementor.
    ///
    /// THE FIX IS NOT IN `Drop`, because `Drop` genuinely cannot reach a `RuntimeValueOps`
    /// — it is ONE STEP EARLIER, in `crate::ffi::context::__elephc_eval_context_free`, the
    /// FFI teardown entry point generated code calls to free an `ElephcEvalContext`
    /// (`EvalStreamResources`'s owner). That function has NO `RuntimeValueOps` PARAMETER
    /// either, but — unlike `Drop` — it does not need one handed to it: production's
    /// `RuntimeValueOps` implementor, `crate::runtime_hooks::ElephcRuntimeOps`, is a
    /// STATELESS adapter over generated runtime C-ABI symbols
    /// (`ElephcRuntimeOps::new()` needs no context pointer, no live eval call, nothing
    /// beyond the process's already-running runtime heap), so it can be constructed
    /// ad hoc, right there, a moment before `Box::from_raw(ctx)` drops the context (and,
    /// transitively, this table). See `__elephc_eval_context_free`'s own call site for the
    /// wiring — INCLUDING WHERE it calls this relative to that function's OTHER teardown
    /// steps, which is load-bearing (a retained `CURLOPT_PRIVATE` value can be, or
    /// transitively reference, an eval-declared object with a `__destruct()`, and that
    /// dispatch needs this context's dynamic-object registration still intact) — this
    /// method is the storage-layer half of that fix.
    ///
    /// CONTINUES ON A PER-ENTRY RELEASE FAILURE rather than propagating the first one:
    /// this runs once, at teardown, with no later chance to retry, so letting one
    /// `values.release()` failure short-circuit the loop (the previous `?`-propagating
    /// shape) would silently leak every OTHER still-retained entry in this same table
    /// instead of just the one that failed. Every entry's own `private_value` is still
    /// unconditionally `.take()`n either way, so a failed release cannot be retried twice
    /// (matching `RuntimeValueOps::release`'s own contract: a failure here is not a
    /// "nothing happened" outcome to undo).
    pub(crate) fn release_curl_easy_private_values(&mut self, values: &mut impl RuntimeValueOps) {
        for handle in self.curl_easy_handles.values_mut() {
            if let Some(previous) = handle.private_value.take() {
                let _ = values.release(previous);
            }
            // THE CALLBACK ROOTS GO OUT THE SAME DOOR, for the identical reason and at the
            // identical moment: `set_curl_easy_callback` retains every installed callable,
            // and `Drop for EvalStreamResources` cannot release one (it never receives a
            // `RuntimeValueOps`). A callback closure can capture — or be — an eval-declared
            // object with a `__destruct()`, which is exactly why this runs from
            // `__elephc_eval_context_free` while the context's dynamic-object registration
            // is still intact, rather than from `Drop`.
            for slot in &mut handle.callbacks {
                if let EvalCurlCallbackSlot::Callable(cell) = std::mem::take(slot) {
                    let _ = values.release(cell);
                }
            }
        }
    }

    /// Replaces one callback slot, RETAINING a `Callable` value and releasing whatever the
    /// slot previously held — the same independent-owned-reference discipline
    /// `set_curl_easy_private` documents, and for the same reason: the caller's cell is a
    /// borrowed scope entry that `unset()`/reassignment can free while libcurl still has the
    /// slot registered.
    ///
    /// Returns `false` (after releasing the just-retained value, so nothing leaks) for an
    /// unknown id or an out-of-range slot.
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
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            if let EvalCurlCallbackSlot::Callable(cell) = value {
                values.release(cell)?;
            }
            return Ok(false);
        };
        let Some(current) = handle.callbacks.get_mut(slot) else {
            if let EvalCurlCallbackSlot::Callable(cell) = value {
                values.release(cell)?;
            }
            return Ok(false);
        };
        let previous = std::mem::replace(current, value);
        if let EvalCurlCallbackSlot::Callable(cell) = previous {
            values.release(cell)?;
        }
        Ok(true)
    }

    /// Drops every callback slot on one handle, releasing each retained callable —
    /// `curl_reset()`'s half of `crates/elephc-curl/src/callbacks.rs`'s `clear_all`
    /// (php-src frees its handler callables in `curl_reset()` too).
    pub(crate) fn clear_curl_easy_callbacks(
        &mut self,
        id: i64,
        values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        let Some(handle) = self.curl_easy_handles.get_mut(&id) else {
            return Ok(());
        };
        for slot in &mut handle.callbacks {
            if let EvalCurlCallbackSlot::Callable(cell) = std::mem::take(slot) {
                values.release(cell)?;
            }
        }
        Ok(())
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
            .filter(|(_, slot)| **slot != EvalCurlCallbackSlot::Empty)
            .map(|(index, slot)| (index, *slot))
            .collect()
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
