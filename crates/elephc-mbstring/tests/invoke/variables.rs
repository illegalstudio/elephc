//! Purpose:
//! Exercises the public mb_convert_variables invocation through live root storage.
//!
//! Called from:
//! - The focused mbstring invocation integration binary.
//!
//! Key details:
//! - Source encoding and root bytes cross the actual protected V6 boundary.
//! - The root remains the caller's original slot while encoding inputs are copied.
//! - Unused V4/V5 capabilities remain complete but are never invoked.

use super::*;
use elephc_builtin_contract::mbstring_abi::{
    invoke::{MbCaptureOutputV1, MbInvokeHostV4, MbInvokeHostV5, MbQueryConfigV1,
        MbQueryRegisteredV1, MbQueryStepV1},
    variables::{MbInvokeHostV6, MbVariableChildV1, MbVariableHandleV1, MbVariableViewV1,
        VARIABLE_CHILD_END, VARIABLE_STRING},
};

/// The ordinary fixture host is at offset zero so its established callbacks share this context.
#[repr(C)]
struct VariableFixture { host: Host }

impl VariableFixture {
    /// Builds every required V6 prefix without enabling unrelated operation callbacks.
    fn table(&mut self) -> MbInvokeHostV6 {
        let mut base = self.host.table_v3();
        base.base.base.version = 6;
        base.base.base.size = std::mem::size_of::<MbInvokeHostV6>() as u32;
        MbInvokeHostV6 {
            base: MbInvokeHostV5 {
                base: MbInvokeHostV4 {
                    base,
                    capture_initialize: Some(unused_capture_initialize),
                    capture_fill: Some(unused_capture_fill),
                    capture_release: Some(unused_capture_release),
                },
                query_configuration: Some(unused_query_configuration),
                query_filter: None,
                query_register: Some(unused_query_register),
            },
            variable_inspect: Some(inspect),
            variable_child_next: Some(child),
            variable_prepare_write: Some(prepare),
            variable_write_string: Some(write),
        }
    }
}

/// Reads a root PHP string from its original caller slot.
unsafe extern "C" fn inspect(
    _: *mut c_void, handle: *const MbVariableHandleV1, view: *mut MbVariableViewV1,
) -> i32 {
    let handle = unsafe { &*handle };
    if handle.words[1] != 1 { return 1; }
    let value = unsafe { &*(handle.words[0] as usize as *const Php) };
    let Php::String(bytes) = value else { return 1; };
    unsafe { *view = MbVariableViewV1 {
        kind: VARIABLE_STRING, identity: 0, bytes: bytes.as_ptr(), len: bytes.len() as u64,
    }; }
    0
}

/// Scalar roots have no array or object children.
unsafe extern "C" fn child(
    _: *mut c_void, _: u64, _: u64, _: *mut u64, output: *mut MbVariableChildV1,
) -> i32 {
    unsafe { (*output).kind = VARIABLE_CHILD_END; }
    0
}

/// String roots never request container separation.
unsafe extern "C" fn prepare(
    _: *mut c_void, _: *const MbVariableHandleV1, _: u64, identity: u64, output: *mut u64,
) -> i32 {
    unsafe { *output = identity; }
    0
}

/// Publishes a converted string into the exact caller slot.
unsafe extern "C" fn write(
    _: *mut c_void, handle: *const MbVariableHandleV1, bytes: *const u8, len: u64,
) -> i32 {
    let handle = unsafe { &*handle };
    if handle.words[1] != 1 { return 1; }
    let slot = unsafe { &mut *(handle.words[0] as usize as *mut Php) };
    *slot = Php::String(unsafe { std::slice::from_raw_parts(bytes, len as usize) }.to_vec());
    0
}

unsafe extern "C" fn unused_capture_initialize(
    _: *mut c_void, _: *const c_void, _: *mut MbCaptureOutputV1,
) -> i32 { 1 }
unsafe extern "C" fn unused_capture_fill(
    _: *mut c_void, _: *mut c_void, _: *const u8, _: u64,
) -> i32 { 1 }
unsafe extern "C" fn unused_capture_release(_: *mut c_void, _: *mut c_void) -> i32 { 1 }
unsafe extern "C" fn unused_query_configuration(
    _: *mut c_void, _: u32, _: *mut MbQueryConfigV1,
) -> i32 { 1 }
unsafe extern "C" fn unused_query_register(
    _: *mut c_void, _: *mut c_void, _: *const MbQueryStepV1, _: u64, _: *const u8, _: u64,
    _: *mut MbQueryRegisteredV1,
) -> i32 { 1 }

/// The shared coordinator must mutate the caller slot and return one canonical source name.
#[test]
fn converts_live_variadic_roots_through_v6() {
    elephc_mbstring_reset_v1();
    let mut fixture = VariableFixture { host: Host::new("") };
    let mut values = [
        Php::String(b"UTF-8".to_vec()),
        Php::String(b"ISO-8859-1".to_vec()),
        Php::String(b"caf\xe9".to_vec()),
        Php::String(b"na\xefve".to_vec()),
    ];
    let pointers = values.iter_mut().map(|value| (value as *mut Php).cast::<c_void>() as *const c_void)
        .collect::<Vec<_>>();
    let table = fixture.table();
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_invoke_v1(
        RuntimeBuiltinId::MbConvertVariables.as_u32(), pointers.as_ptr(), pointers.len() as u64,
        0, &table.base.base.base.base.base, &mut output,
    ) };
    assert_eq!(status, 0);
    assert_eq!(result(output), json!(["string", hex(b"ISO-8859-1")]));
    assert!(matches!(&values[2], Php::String(bytes) if bytes == b"caf\xc3\xa9"));
    assert!(matches!(&values[3], Php::String(bytes) if bytes == b"na\xc3\xafve"));
    assert!(fixture.host.live.is_empty(), "input owners leaked");
    assert!(fixture.host.errors.is_empty(), "{:?}", fixture.host.errors);
}
