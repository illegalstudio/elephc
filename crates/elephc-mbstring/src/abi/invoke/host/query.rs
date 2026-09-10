//! Purpose:
//! Adapts protected query configuration, filtering, and registration callbacks to owned Rust data.
//!
//! Called from:
//! - The shared mb_parse_str invocation coordinator.
//!
//! Key details:
//! - Published byte owners enter the common cleanup arena before callback status is inspected.
//! - Pending exceptions do not discard valid completed filtering or registration metadata.
//! - Name-plan descriptors borrow only stable Rust-owned bytes for the duration of one callback.

use super::*;
use crate::input::RegistrationStep;
use elephc_builtin_contract::mbstring_abi::array::Key;

/// Owned core settings with no host string lease remaining across another callback.
pub(in super::super) struct QueryConfiguration {
    pub separators: Vec<u8>, pub max_variables: i64, pub max_nesting: i64, pub display_errors: bool,
}

impl Session {
    /// Requires V5 before acquiring any argument owner for a writable query output.
    pub(in super::super) fn has_query_output(&self) -> bool { self.query_host.is_some() }

    /// Copies phase-specific settings and retires their optional separator owner on every status.
    pub(in super::super) unsafe fn query_configuration(&mut self, phase: u32) -> Result<(QueryConfiguration, Option<Status>), Status> {
        let host = self.query_host.ok_or(Status::Fatal)?;
        let mut output = MbQueryConfigV1 { separators: empty_string(), max_variables: 0, max_nesting: 0, display_errors: 0 };
        let raw = unsafe { host.query_configuration.unwrap()(self.context(), phase, &mut output) };
        self.temporary = output.separators.owner;
        let status = callback_status(raw)?;
        if output.display_errors > 1 { return Err(status.unwrap_or(Status::Fatal)); }
        let (separators, status) = unsafe { self.query_string(output.separators, status)? };
        Ok((QueryConfiguration { separators, max_variables: output.max_variables,
            max_nesting: output.max_nesting, display_errors: output.display_errors == 1 }, status))
    }

    /// Runs an optional SAPI filter, preserving its valid replacement even with a pending exception.
    pub(in super::super) unsafe fn query_filter(&mut self, name: &[u8], value: Vec<u8>) -> Result<(Option<Vec<u8>>, Option<Status>), Status> {
        let host = self.query_host.ok_or(Status::Fatal)?;
        let Some(filter) = host.query_filter else { return Ok((Some(value), None)); };
        let mut output = MbQueryFilteredV1 { accepted: 0, value: empty_string() };
        let raw = unsafe { filter(self.context(), name.as_ptr(), name.len() as u64, value.as_ptr(), value.len() as u64, &mut output) };
        self.temporary = output.value.owner;
        let status = callback_status(raw)?;
        if output.accepted > 1 { return Err(status.unwrap_or(Status::Fatal)); }
        let (bytes, status) = unsafe { self.query_string(output.value, status)? };
        Ok(((output.accepted == 1).then_some(bytes), status))
    }

    /// Applies a shared plan to live storage and reports only an actually reached nesting removal.
    pub(in super::super) unsafe fn query_register(&self, plan: &[RegistrationStep], value: &[u8]) -> Result<(bool, Option<Status>), Status> {
        let host = self.query_host.ok_or(Status::Fatal)?;
        if self.capture_output.ready != 1 || self.capture_output.writer.is_null() { return Err(Status::Fatal); }
        let steps = plan.iter().map(|step| {
            let (operation, key) = match step {
                RegistrationStep::Enter(key) => (QUERY_ENTER, key.as_ref()),
                RegistrationStep::Store(key) => (QUERY_STORE, key.as_ref()),
                RegistrationStep::RemoveRoot(key) => (QUERY_REMOVE_ROOT, Some(key)),
            };
            MbQueryStepV1 { operation, append: u64::from(key.is_none()), key: key.map_or_else(MbHostValueV1::null, descriptor) }
        }).collect::<Vec<_>>();
        let mut output = MbQueryRegisteredV1::default();
        let raw = unsafe { host.query_register.unwrap()(self.context(), self.capture_output.writer,
            steps.as_ptr(), steps.len() as u64, value.as_ptr(), value.len() as u64, &mut output) };
        let status = callback_status(raw)?;
        if output.nesting_exceeded > 1 { return Err(status.unwrap_or(Status::Fatal)); }
        Ok((output.nesting_exceeded == 1, status))
    }

    /// Copies borrowed callback bytes before retiring their lease, retaining completed data on throws.
    unsafe fn query_string(&mut self, output: MbHostStringV1, status: Option<Status>) -> Result<(Vec<u8>, Option<Status>), Status> {
        if output.len > isize::MAX as u64 || (output.len != 0 && output.bytes.is_null()) { return Err(status.unwrap_or(Status::Fatal)); }
        let bytes = if output.len == 0 { Vec::new() }
            else { unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize).to_vec() } };
        let cleanup = unsafe { self.release_temporary() };
        match cleanup {
            Ok(()) => Ok((bytes, status)),
            Err(Status::Pending) => Ok((bytes, Some(Status::Pending))),
            Err(Status::Fatal) => Err(status.unwrap_or(Status::Fatal)),
        }
    }
}

/// Accepts completed success/pending responses and rejects all unsupported or malformed statuses.
fn callback_status(raw: i32) -> Result<Option<Status>, Status> {
    match raw { 0 => Ok(None), 2 => Ok(Some(Status::Pending)), _ => Err(Status::Fatal) }
}

/// Initializes output byte metadata before a protected callback can publish an owner.
fn empty_string() -> MbHostStringV1 { MbHostStringV1 { bytes: std::ptr::null(), len: 0, owner: std::ptr::null_mut() } }

/// Borrows one already-normalized key without repeating PHP numeric-string conversion in the host.
fn descriptor(key: &Key) -> MbHostValueV1 {
    match key {
        Key::Int(value) => MbHostValueV1 { tag: HOST_INT, lo: *value as u64, hi: 0 },
        Key::String(bytes) => MbHostValueV1 { tag: HOST_STRING, lo: bytes.as_ptr() as u64, hi: bytes.len() as u64 },
    }
}
