//! Purpose:
//! Retains the resolved replacement callback and each boxed result until explicit host cleanup.
//!
//! Called from:
//! - Callback invocation preparation, replacement rendering, and the common arena cleanup.
//!
//! Key details:
//! - Callback result casting shares the existing value host's scalar/Stringable path.
//! - Every output owner is registered before inspecting the callback's returned status.

use super::*;

impl Session {
    /// Validates the replacement host before acquiring its callable or argument owners.
    pub(in super::super) unsafe fn initialize_callback_host(&mut self, host: *const MbCallbackHostV1) -> Result<(), Status> {
        let host = unsafe { host.as_ref() }.ok_or(Status::Fatal)?;
        if host.version != 1 || host.size as usize != std::mem::size_of::<MbCallbackHostV1>()
            || host.resolve.is_none() || host.invoke.is_none() { return Err(Status::Fatal); }
        self.callback_host = Some(*host);
        Ok(())
    }

    /// Resolves the retained original argument at its PHP parameter-validation point.
    pub(in super::super) unsafe fn resolve_callback(&mut self, index: usize) -> Result<(), Status> {
        let host = self.callback_host.ok_or(Status::Fatal)?;
        if !self.callback.is_null() { return Err(Status::Fatal); }
        let status = unsafe { host.resolve.unwrap()(host.context, self.arguments[index], &mut self.callback) };
        Status::decode(status)?;
        if self.callback.is_null() { return Err(Status::Fatal); }
        Ok(())
    }

    /// Calls with an independently encoded capture array, casts the owned result, and retires it.
    pub(in super::super) unsafe fn call_replacement(&mut self, graph: &[u8]) -> Result<Vec<u8>, Status> {
        let host = self.callback_host.ok_or(Status::Fatal)?;
        if self.callback.is_null() || !self.entry.is_null() { return Err(Status::Fatal); }
        let mut call = MbCallbackCallV1 { captures: graph.as_ptr(), captures_len: graph.len() as u64,
            result: std::ptr::null_mut() };
        let status = unsafe { host.invoke.unwrap()(host.context, self.callback, &mut call) };
        self.entry = call.result;
        Status::decode(status)?;
        if self.entry.is_null() { return Err(Status::Fatal); }
        let bytes = unsafe { self.entry_string()? };
        unsafe { self.release_entry()?; }
        Ok(bytes)
    }

    /// Consumes the resolved callback through the value host even after failed resolution or invocation.
    pub(super) unsafe fn release_callback(&mut self) -> Result<(), Status> {
        let callback = std::mem::replace(&mut self.callback, std::ptr::null_mut());
        if callback.is_null() { return Ok(()); }
        let host = self.host.ok_or(Status::Fatal)?;
        Status::decode(unsafe { host.release_owner.unwrap()(host.context, callback) })
    }
}
