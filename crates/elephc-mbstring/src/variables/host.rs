//! Purpose:
//! Adapts protected native and eval callbacks to live mbstring variable traversal.
//!
//! Called from:
//! - The mb_convert_variables invocation coordinator after argument coercion.
//!
//! Key details:
//! - PHP value and slot ownership remain entirely with the host.
//! - Borrowed string bytes are copied before another callback can run.
//! - Every malformed status, cursor, kind, or byte range fails closed.

use std::ffi::c_void;

use elephc_builtin_contract::mbstring_abi::variables::{
    MbInvokeHostV6, MbVariableChildNextV1, MbVariableChildV1, MbVariableHandleV1,
    MbVariableInspectV1, MbVariablePrepareWriteV1, MbVariableViewV1,
    MbVariableWriteStringV1, VARIABLE_ARRAY, VARIABLE_CHILD_END, VARIABLE_CHILD_ENTRY,
    VARIABLE_OBJECT, VARIABLE_OTHER, VARIABLE_STRING,
};

use super::{Container, LiveHost, LiveValue};

/// A protected callback's fatal or pending-throwable status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostError { Fatal, Pending }

impl HostError {
    /// Decodes the common callback status without accepting unknown values.
    fn check(raw: i32) -> Result<(), Self> {
        match raw { 0 => Ok(()), 2 => Err(Self::Pending), _ => Err(Self::Fatal) }
    }
}

/// Borrowed host callback table for one synchronous conversion invocation.
pub struct HostAdapter {
    context: *mut c_void,
    inspect: MbVariableInspectV1,
    child_next: MbVariableChildNextV1,
    prepare_write: MbVariablePrepareWriteV1,
    write_string: MbVariableWriteStringV1,
}

impl HostAdapter {
    /// Requires a complete version-six extension before touching caller storage.
    pub fn new(host: &MbInvokeHostV6) -> Option<Self> {
        if host.base.base.base.base.base.version != 6 || host.base.base.base.base.base.size as usize
            != std::mem::size_of::<MbInvokeHostV6>() { return None; }
        Some(Self {
            context: host.base.base.base.base.base.context,
            inspect: host.variable_inspect?,
            child_next: host.variable_child_next?,
            prepare_write: host.variable_prepare_write?,
            write_string: host.variable_write_string?,
        })
    }
}

impl LiveHost for HostAdapter {
    type Handle = MbVariableHandleV1;
    type Identity = u64;
    type Error = HostError;

    /// Reads a dereferenced value and copies any binary string before the next host call.
    fn inspect(&mut self, handle: &Self::Handle) -> Result<LiveValue<u64>, HostError> {
        let mut view = MbVariableViewV1::default();
        HostError::check(unsafe { (self.inspect)(self.context, handle, &mut view) })?;
        match view.kind {
            VARIABLE_OTHER if view.identity == 0 && view.len == 0 => Ok(LiveValue::Other),
            VARIABLE_STRING if view.identity == 0 && view.len <= isize::MAX as u64
                && (view.len == 0 || !view.bytes.is_null()) => {
                let bytes = if view.len == 0 { Vec::new() }
                    else { unsafe { std::slice::from_raw_parts(view.bytes, view.len as usize).to_vec() } };
                Ok(LiveValue::String(bytes))
            },
            VARIABLE_ARRAY if view.identity != 0 && view.len == 0 =>
                Ok(LiveValue::Container(Container::Array(view.identity))),
            VARIABLE_OBJECT if view.identity != 0 && view.len == 0 =>
                Ok(LiveValue::Container(Container::Object(view.identity))),
            _ => Err(HostError::Fatal),
        }
    }

    /// Advances in host storage order without creating a value owner or converting a key.
    fn child(&mut self, container: Container<u64>, cursor: &mut usize)
        -> Result<Option<Self::Handle>, HostError> {
        let mut position = *cursor as u64;
        let before = position;
        let mut child = MbVariableChildV1::default();
        let (kind, identity) = match container {
            Container::Array(identity) => (VARIABLE_ARRAY, identity),
            Container::Object(identity) => (VARIABLE_OBJECT, identity),
        };
        HostError::check(unsafe { (self.child_next)(
            self.context, kind, identity, &mut position, &mut child,
        ) })?;
        match child.kind {
            VARIABLE_CHILD_END if position == before => Ok(None),
            VARIABLE_CHILD_ENTRY if position > before && usize::try_from(position).is_ok() => {
                *cursor = position as usize;
                Ok(Some(child.handle))
            },
            _ => Err(HostError::Fatal),
        }
    }

    /// Lets the host separate one array and returns the live post-COW allocation identity.
    fn prepare_write(&mut self, handle: &Self::Handle, container: Container<u64>)
        -> Result<Container<u64>, HostError> {
        let (kind, identity) = match container {
            Container::Array(identity) => (VARIABLE_ARRAY, identity),
            Container::Object(identity) => (VARIABLE_OBJECT, identity),
        };
        let mut current = 0;
        HostError::check(unsafe { (self.prepare_write)(
            self.context, handle, kind, identity, &mut current,
        ) })?;
        if current == 0 || matches!(container, Container::Object(_)) && current != identity {
            return Err(HostError::Fatal);
        }
        Ok(match container {
            Container::Array(_) => Container::Array(current),
            Container::Object(_) => Container::Object(current),
        })
    }

    /// Publishes converted bytes in the exact caller or nested slot selected by the host.
    fn write_string(&mut self, handle: &Self::Handle, bytes: Vec<u8>) -> Result<(), HostError> {
        HostError::check(unsafe {
            (self.write_string)(self.context, handle, bytes.as_ptr(), bytes.len() as u64)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Host storage deliberately reallocates after inspection to prove the adapter owns bytes.
    struct Fixture { bytes: Vec<u8>, written: Vec<u8> }

    unsafe extern "C" fn inspect(
        context: *mut c_void, _: *const MbVariableHandleV1, view: *mut MbVariableViewV1,
    ) -> i32 {
        let fixture = unsafe { &mut *(context as *mut Fixture) };
        unsafe { *view = MbVariableViewV1 {
            kind: VARIABLE_STRING, identity: 0, bytes: fixture.bytes.as_ptr(),
            len: fixture.bytes.len() as u64,
        }; }
        0
    }

    unsafe extern "C" fn child(
        _: *mut c_void, _: u64, _: u64, _: *mut u64, child: *mut MbVariableChildV1,
    ) -> i32 {
        unsafe { (*child).kind = VARIABLE_CHILD_END; }
        0
    }

    unsafe extern "C" fn prepare(
        _: *mut c_void, _: *const MbVariableHandleV1, _: u64, identity: u64, output: *mut u64,
    ) -> i32 {
        unsafe { *output = identity; }
        0
    }

    unsafe extern "C" fn write(
        context: *mut c_void, _: *const MbVariableHandleV1, bytes: *const u8, len: u64,
    ) -> i32 {
        let fixture = unsafe { &mut *(context as *mut Fixture) };
        fixture.written = unsafe { std::slice::from_raw_parts(bytes, len as usize) }.to_vec();
        0
    }

    /// The V6 adapter copies callback bytes and rejects incomplete capability tables.
    #[test]
    fn live_host_table_owns_inspected_bytes() {
        let mut fixture = Fixture { bytes: b"before".to_vec(), written: Vec::new() };
        let mut table: MbInvokeHostV6 = unsafe { std::mem::zeroed() };
        table.base.base.base.base.base.version = 6;
        table.base.base.base.base.base.size = std::mem::size_of::<MbInvokeHostV6>() as u32;
        table.base.base.base.base.base.context = (&mut fixture as *mut Fixture).cast();
        table.variable_inspect = Some(inspect);
        table.variable_child_next = Some(child);
        table.variable_prepare_write = Some(prepare);
        assert!(HostAdapter::new(&table).is_none());
        table.variable_write_string = Some(write);
        let mut adapter = HostAdapter::new(&table).expect("complete V6 host");
        let handle = MbVariableHandleV1::default();
        let LiveValue::String(original) = adapter.inspect(&handle).expect("inspect") else {
            panic!("expected string");
        };
        fixture.bytes = b"replacement with a larger allocation".to_vec();
        assert_eq!(original, b"before");
        adapter.write_string(&handle, original).expect("write");
        assert_eq!(fixture.written, b"before");
    }
}
