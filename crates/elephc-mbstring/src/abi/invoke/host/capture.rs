//! Purpose:
//! Owns the protected host writer for one caller-visible mbregex capture output.
//!
//! Called from:
//! - Capture invocation after parameter coercion, and the common ownership arena cleanup.
//!
//! Key details:
//! - Initialization records ownership before inspecting status or metadata.
//! - A successful initialization can coexist with a pending destructor exception.
//! - Opaque writer owners use their dedicated release callback before argument pins retire.

use super::*;

impl Session {
    /// Rejects reference invocations through older hosts before any argument callback executes.
    pub(in super::super) fn has_capture_output(&self) -> bool { self.capture_host.is_some() }

    /// Initializes the exact caller output and reports body readiness independently from status.
    pub(in super::super) unsafe fn initialize_capture(&mut self, index: usize) -> (bool, Option<Status>) {
        let Some(host) = self.capture_host else { return (false, Some(Status::Fatal)); };
        let raw = unsafe { host.capture_initialize.unwrap()(self.context(), self.originals[index], &mut self.capture_output) };
        let mut status = Status::decode(raw).err();
        let valid = match self.capture_output.ready {
            0 => status.is_some(),
            1 => !self.capture_output.writer.is_null(),
            _ => false,
        };
        if !valid { status = Some(status.map_or(Status::Fatal, |error| error.merge(Status::Fatal))); }
        let ready = valid && self.capture_output.ready == 1 && status != Some(Status::Fatal);
        (ready, status)
    }

    /// Inserts the complete ordered capture graph into the retained construction array.
    pub(in super::super) unsafe fn fill_capture(&self, graph: &[u8]) -> Result<(), Status> {
        let host = self.capture_host.ok_or(Status::Fatal)?;
        if self.capture_output.ready != 1 || self.capture_output.writer.is_null() { return Err(Status::Fatal); }
        Status::decode(unsafe { host.capture_fill.unwrap()(self.context(), self.capture_output.writer,
            graph.as_ptr(), graph.len() as u64) })
    }

    /// Consumes any published writer after success, failed initialization, or a contained Rust panic.
    pub(super) unsafe fn release_capture(&mut self) -> Result<(), Status> {
        let writer = std::mem::replace(&mut self.capture_output.writer, std::ptr::null_mut());
        if writer.is_null() { return Ok(()); }
        let host = self.capture_host.ok_or(Status::Fatal)?;
        Status::decode(unsafe { host.capture_release.unwrap()(self.context(), writer) })
    }
}
