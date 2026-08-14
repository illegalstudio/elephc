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
            },
        );
        Ok(id)
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
    /// WHY THIS EXISTS (item 19, php-curl-family punch list): `set_curl_easy_private`
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
    /// wiring; this method is the storage-layer half of that fix.
    pub(crate) fn release_curl_easy_private_values(
        &mut self,
        values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        for handle in self.curl_easy_handles.values_mut() {
            if let Some(previous) = handle.private_value.take() {
                values.release(previous)?;
            }
        }
        Ok(())
    }
}
