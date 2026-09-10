//! Purpose:
//! Owns native argument copies and temporary callback results during shared mbstring invocation.
//!
//! Called from:
//! - The invocation coordinator before and after pure planning and request-state dispatch.
//!
//! Key details:
//! - Ownership slots exist before callbacks publish pointers, so Rust panics cannot lose owners.
//! - Cleanup is explicit, continues after failures, and never invokes PHP from a Rust destructor.
//! - Host callbacks must contain PHP exceptions and consume released owners even when they throw.

use super::*;
use elephc_builtin_contract::mbstring_abi::callback::{MbCallbackCallV1, MbCallbackHostV1};

mod graph;
mod capture;
mod query;
mod callback;

/// Internal failure status, with pending PHP exceptions taking precedence during final cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status { Fatal, Pending }

impl Status {
    /// Rejects unknown or unsupported callback results without inventing a PHP value.
    fn decode(raw: i32) -> Result<(), Self> {
        match raw { 0 => Ok(()), 2 => Err(Self::Pending), _ => Err(Self::Fatal) }
    }

    /// Preserves a pending exception when another cleanup callback encounters a fatal status.
    pub(super) fn merge(self, later: Self) -> Self {
        if self == Self::Pending || later == Self::Pending { Self::Pending } else { Self::Fatal }
    }
}

/// Explicit ownership arena, deliberately lacking a destructor that could execute PHP.
#[derive(Default)]
pub(super) struct Session {
    host: Option<MbInvokeHostV1>,
    arguments: Vec<*mut c_void>,
    originals: Vec<*const c_void>,
    temporary: *mut c_void,
    entry: *mut c_void,
    array_value: Option<MbArrayValueV2>,
    graph_value: Option<MbArrayValueV3>,
    pin_value: Option<MbPinValueV3>,
    pins: Vec<*mut c_void>,
    graph_entries: Vec<MbArrayEntryV3>,
    capture_host: Option<MbInvokeHostV4>,
    capture_output: MbCaptureOutputV1,
    query_host: Option<MbInvokeHostV5>,
    callback_host: Option<MbCallbackHostV1>,
    callback: *mut c_void,
}

impl Session {
    /// Validates the complete versioned table and reserves every argument owner before callbacks.
    pub(super) unsafe fn initialize(&mut self, host: *const MbInvokeHostV1, count: usize) -> Result<(), Status> {
        let host = unsafe { host.as_ref() }.ok_or(Status::Fatal)?;
        self.array_value = match (host.version, host.size as usize) {
            (1, size) if size == std::mem::size_of::<MbInvokeHostV1>() => None,
            (2, size) if size == std::mem::size_of::<MbInvokeHostV2>() => {
                let extended = unsafe { &*(host as *const MbInvokeHostV1).cast::<MbInvokeHostV2>() };
                Some(extended.array_value.ok_or(Status::Fatal)?)
            },
            (3, size) if size == std::mem::size_of::<MbInvokeHostV3>() => {
                let extended = unsafe { &*(host as *const MbInvokeHostV1).cast::<MbInvokeHostV3>() };
                self.graph_value = Some(extended.graph_value.ok_or(Status::Fatal)?);
                self.pin_value = Some(extended.pin_value.ok_or(Status::Fatal)?);
                Some(extended.base.array_value.ok_or(Status::Fatal)?)
            },
            (version @ (4 | 5), size) if size == if version == 4 { std::mem::size_of::<MbInvokeHostV4>() } else { std::mem::size_of::<MbInvokeHostV5>() } => {
                if version == 5 {
                    let query = unsafe { &*(host as *const MbInvokeHostV1).cast::<MbInvokeHostV5>() };
                    if query.query_configuration.is_none() || query.query_register.is_none() { return Err(Status::Fatal); }
                    self.query_host = Some(*query);
                }
                let extended = unsafe { &*(host as *const MbInvokeHostV1).cast::<MbInvokeHostV4>() };
                if extended.capture_initialize.is_none() || extended.capture_fill.is_none()
                    || extended.capture_release.is_none() { return Err(Status::Fatal); }
                self.capture_host = Some(*extended);
                self.graph_value = Some(extended.base.graph_value.ok_or(Status::Fatal)?);
                self.pin_value = Some(extended.base.pin_value.ok_or(Status::Fatal)?);
                Some(extended.base.base.array_value.ok_or(Status::Fatal)?)
            },
            _ => return Err(Status::Fatal),
        };
        if host.clone_value.is_none() || host.describe_value.is_none() || host.stringable.is_none()
            || host.format_float.is_none() || host.diagnostic.is_none() || host.release_owner.is_none()
            || host.array_next.is_none() { return Err(Status::Fatal); }
        self.host = Some(*host);
        self.arguments = vec![std::ptr::null_mut(); count];
        self.originals = vec![std::ptr::null(); count];
        self.pins = vec![std::ptr::null_mut(); count];
        Ok(())
    }

    /// Gives the invocation an independent by-value copy before any parameter is converted.
    pub(super) unsafe fn clone_argument(&mut self, index: usize, argument: *const c_void) -> Result<(), Status> {
        unsafe { self.pin_argument(index, argument)?; }
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        let status = unsafe { host.clone_value.unwrap()(host.context, argument, &mut self.arguments[index]) };
        Status::decode(status)?;
        if self.arguments[index].is_null() { return Err(Status::Fatal); }
        Ok(())
    }

    /// Retains a caller identity without copying its current value or delaying the old value's destructor.
    pub(super) unsafe fn pin_argument(&mut self, index: usize, argument: *const c_void) -> Result<(), Status> {
        self.originals[index] = argument;
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        if let Some(pin) = self.pin_value {
            Status::decode(unsafe { pin(host.context, argument, &mut self.pins[index]) })?;
        }
        Ok(())
    }

    /// Obtains concrete metadata and records its owner before inspecting callback success.
    pub(super) unsafe fn describe(&mut self, index: usize) -> Result<MbCoercionInputV1, Status> {
        unsafe { self.describe_owner(self.arguments[index]) }
    }

    /// Borrows concrete metadata for either an argument copy or the current encoding-list entry.
    unsafe fn describe_owner(&mut self, owner: *const c_void) -> Result<MbCoercionInputV1, Status> {
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        let mut input = MbCoercionInputV1 { kind: 0, value: 0, bytes: std::ptr::null(), len: 0, flags: 0 };
        let status = unsafe { host.describe_value.unwrap()(host.context, owner, &mut input, &mut self.temporary) };
        Status::decode(status)?;
        Ok(input)
    }

    /// Delivers an owned diagnostic while no request-state borrow or metadata owner remains active.
    pub(super) unsafe fn diagnostic(&self, level: u32, message: &[u8]) -> Result<(), Status> {
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        Status::decode(unsafe { host.diagnostic.unwrap()(host.context, level, message.as_ptr(), message.len() as u64) })
    }

    /// Runs the protected Stringable boundary and copies its bytes before releasing native ownership.
    pub(super) unsafe fn stringable(&mut self, index: usize) -> Result<Vec<u8>, Status> {
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        let mut output = MbHostStringV1 { bytes: std::ptr::null(), len: 0, owner: std::ptr::null_mut() };
        let status = unsafe { host.stringable.unwrap()(host.context, self.arguments[index], &mut output) };
        self.temporary = output.owner;
        Status::decode(status)?;
        unsafe { self.consume_string(output) }
    }

    /// Formats a float only at its parameter's conversion point, using the host's current precision.
    pub(super) unsafe fn format_float(&mut self, bits: u64) -> Result<Vec<u8>, Status> {
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        let mut output = MbHostStringV1 { bytes: std::ptr::null(), len: 0, owner: std::ptr::null_mut() };
        let status = unsafe { host.format_float.unwrap()(host.context, bits, &mut output) };
        self.temporary = output.owner;
        Status::decode(status)?;
        unsafe { self.consume_string(output) }
    }

    /// Validates a borrowed native string range, copies it, and consumes the separate native owner.
    unsafe fn consume_string(&mut self, output: MbHostStringV1) -> Result<Vec<u8>, Status> {
        if output.len > isize::MAX as u64 || (output.len != 0 && output.bytes.is_null()) { return Err(Status::Fatal); }
        let bytes = if output.len == 0 { Vec::new() }
            else { unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize).to_vec() } };
        unsafe { self.release_temporary()?; }
        Ok(bytes)
    }

    /// Consumes optional metadata/string ownership exactly once, even if its destructor throws.
    pub(super) unsafe fn release_temporary(&mut self) -> Result<(), Status> {
        let owner = std::mem::replace(&mut self.temporary, std::ptr::null_mut());
        if owner.is_null() { return Ok(()); }
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        Status::decode(unsafe { host.release_owner.unwrap()(host.context, owner) })
    }

    /// Acquires the next element before conversion, preserving its owner even on callback failure.
    pub(super) unsafe fn next_entry(&mut self, index: usize, cursor: &mut u64) -> Result<bool, Status> {
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        let callback = self.array_value.ok_or(Status::Fatal)?;
        if !self.entry.is_null() { return Err(Status::Fatal); }
        let mut result = MbArrayEntryV2::default();
        let source = MbArraySourceV2 { retained: self.arguments[index], original: self.originals[index] };
        let status = unsafe { callback(host.context, &source, cursor, &mut result) };
        self.entry = result.owner;
        Status::decode(status)?;
        match result.kind {
            ITER_END if self.entry.is_null() => Ok(false),
            ITER_ENTRY if !self.entry.is_null() => Ok(true),
            _ => Err(Status::Fatal),
        }
    }

    /// Counts top-level stored entries without resolving references or traversing nested arrays.
    pub(super) unsafe fn entry_count(&self, root: MbHostValueV1) -> Result<usize, Status> {
        let mut cursor = 0;
        let mut seen = std::collections::HashSet::from([cursor]);
        let mut count = 0usize;
        loop {
            let mut key = MbHostValueV1::null();
            let mut value = MbHostValueV1::null();
            match unsafe { self.reader()(self.context(), &root, &mut cursor, &mut key, &mut value) } {
                ITER_END => return Ok(count),
                ITER_ENTRY if seen.insert(cursor) => count = count.checked_add(1).ok_or(Status::Fatal)?,
                _ => return Err(Status::Fatal),
            }
        }
    }

    /// Applies map integer conversion and delivers warnings before reading another reference.
    pub(super) unsafe fn entry_integer(&mut self) -> Result<Option<i64>, Status> {
        let input = unsafe { self.describe_owner(self.entry)? };
        let decoded = unsafe { super::super::coercion::decode_input(&input) }.ok_or(Status::Fatal)?;
        let (value, diagnostics) = crate::coercion::entity_map::integer(decoded);
        unsafe { self.release_temporary()?; }
        for diagnostic in diagnostics {
            unsafe { self.diagnostic(diagnostic.level, &diagnostic.message)?; }
        }
        Ok(value)
    }

    /// Converts only the current element, keeping subsequent reference values unevaluated.
    pub(super) unsafe fn entry_string(&mut self) -> Result<Vec<u8>, Status> {
        use crate::coercion::Input;
        let input = unsafe { self.describe_owner(self.entry)? };
        let decoded = unsafe { super::super::coercion::decode_input(&input) }.ok_or(Status::Fatal)?;
        let bytes = match decoded {
            Input::Null | Input::Bool(false) => Some(Vec::new()),
            Input::Bool(true) => Some(b"1".to_vec()),
            Input::Int(value) => Some(value.to_string().into_bytes()),
            Input::String(bytes) => Some(bytes.to_vec()),
            _ => None,
        };
        unsafe { self.release_temporary()?; }
        if let Some(bytes) = bytes { return Ok(bytes); }
        if input.kind == HOST_FLOAT { return unsafe { self.format_float(input.value) }; }
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        let mut output = MbHostStringV1 { bytes: std::ptr::null(), len: 0, owner: std::ptr::null_mut() };
        let status = unsafe { host.stringable.unwrap()(host.context, self.entry, &mut output) };
        self.temporary = output.owner;
        Status::decode(status)?;
        unsafe { self.consume_string(output) }
    }

    /// Consumes the current copied element before reading another array slot.
    pub(super) unsafe fn release_entry(&mut self) -> Result<(), Status> {
        let owner = std::mem::replace(&mut self.entry, std::ptr::null_mut());
        if owner.is_null() { return Ok(()); }
        let host = self.host.as_ref().ok_or(Status::Fatal)?;
        Status::decode(unsafe { host.release_owner.unwrap()(host.context, owner) })
    }

    /// Reports whether this invocation opted into the V2-or-later owned-array and catalog protocol.
    pub(super) fn has_array_values(&self) -> bool { self.array_value.is_some() }

    /// Returns the validated nonmutating array reader for delayed graph snapshots.
    pub(super) fn reader(&self) -> MbArrayNextV1 { self.host.as_ref().unwrap().array_next.unwrap() }

    /// Returns the original host context without interpreting its memory.
    pub(super) fn context(&self) -> *mut c_void { self.host.as_ref().unwrap().context }

    /// Borrows the protected diagnostic callback and context for a nested shared INI operation.
    pub(super) fn ini_host(&self) -> elephc_builtin_contract::mbstring_abi::ini::MbIniHostV1 {
        use elephc_builtin_contract::mbstring_abi::ini::MbIniHostV1;
        let host = self.host.as_ref().expect("initialized invocation");
        MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
            context: host.context, diagnostic: host.diagnostic }
    }

    /// Releases temporary and argument owners after success, early failure, or a contained Rust panic.
    pub(super) unsafe fn cleanup(&mut self) -> Result<(), Status> {
        let mut result = unsafe { self.release_temporary() };
        if let Err(status) = unsafe { self.release_capture() } {
            result = Err(result.err().map_or(status, |previous| previous.merge(status)));
        }
        if let Err(status) = unsafe { self.release_entry() } {
            result = Err(result.err().map_or(status, |previous| previous.merge(status)));
        }
        if let Err(status) = unsafe { self.release_callback() } {
            result = Err(result.err().map_or(status, |previous| previous.merge(status)));
        }
        if let Some(host) = &self.host {
            for entry in self.graph_entries.iter_mut().rev() {
                for owner in [&mut entry.owner, &mut entry.original] {
                    let owner = std::mem::replace(owner, std::ptr::null_mut());
                    if owner.is_null() { continue; }
                    if let Err(status) = Status::decode(unsafe { host.release_owner.unwrap()(host.context, owner) }) {
                        result = Err(result.err().map_or(status, |previous| previous.merge(status)));
                    }
                }
            }
            for owner in &mut self.arguments {
                let owner = std::mem::replace(owner, std::ptr::null_mut());
                if owner.is_null() { continue; }
                if let Err(status) = Status::decode(unsafe { host.release_owner.unwrap()(host.context, owner) }) {
                    result = Err(result.err().map_or(status, |previous| previous.merge(status)));
                }
            }
            for owner in &mut self.pins {
                let owner = std::mem::replace(owner, std::ptr::null_mut());
                if owner.is_null() { continue; }
                if let Err(status) = Status::decode(unsafe { host.release_owner.unwrap()(host.context, owner) }) {
                    result = Err(result.err().map_or(status, |previous| previous.merge(status)));
                }
            }
        }
        result
    }
}
