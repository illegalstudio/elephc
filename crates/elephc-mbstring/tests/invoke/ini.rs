//! Purpose:
//! Verifies shared INI invocation and wire-result cleanup with an independent protected host.
//!
//! Called from:
//! - The mbstring invocation integration test harness.
//!
//! Key details:
//! - Cleanup failures must release completed INI identity leases before returning to the host.
//! - Each callback reenters the request engine through the existing fixture host.

use super::*;
use elephc_builtin_contract::mbstring_abi::ini::{INI_GET, INI_SET, INI_RESTORE, INI_GET_ALL, decode_ini_array};

/// Executes one internal INI operation while retaining its wire result for explicit lease assertions.
fn invoke(operation: u32, name: &[u8], value: &[u8], details: bool, host: &mut Host) -> (i32, MbResultV1) {
    let values = [Php::Int(i64::from(operation)), string(name), string(value), Php::Bool(details)];
    let pointers: Vec<_> = values.iter().map(|value| (value as *const Php).cast::<c_void>()).collect();
    let table = host.table();
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_invoke_v1(RuntimeBuiltinId::SharedIni.as_u32(), pointers.as_ptr(),
        pointers.len() as u64, 0, &table, &mut output) };
    assert!(host.live.is_empty(), "unreleased host owners: {:?}", host.events);
    assert!(host.errors.is_empty(), "{:?}", host.errors);
    (status, output)
}

/// Copies binary bytes without extending the lifetime of a returned INI wire buffer.
fn bytes(output: &MbResultV1) -> Vec<u8> {
    if output.len == 0 { Vec::new() } else {
        unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize).to_vec() }
    }
}

/// Shares live mbstring state, restores defaults, and returns fully framed INI enumeration graphs.
#[test]
fn mbstring_invoke_ini_state_and_enumeration() {
    elephc_mbstring_reset_v1();
    for (operation, name, value, expected) in [
        (INI_GET, "mbstring.language", "", "neutral"),
        (INI_SET, "mbstring.language", "Japanese", "neutral"),
        (INI_GET, "mbstring.language", "", "Japanese"),
    ] {
        let (status, mut output) = invoke(operation, name.as_bytes(), value.as_bytes(), false, &mut Host::new("observe"));
        assert_eq!(status, 0);
        assert_eq!(output.kind, RESULT_INI_STRING);
        assert_eq!(bytes(&output), expected.as_bytes());
        unsafe { elephc_mbstring_release_v1(&mut output); }
    }
    assert_eq!(call(RuntimeBuiltinId::MbLanguage, &[]), json!(["string", hex(b"Japanese")]));
    for module in ["core", "mbstring"] {
        for details in [false, true] {
            let (status, mut output) = invoke(INI_GET_ALL, module.as_bytes(), b"", details, &mut Host::new("observe"));
            assert_eq!(status, 0);
            assert_eq!(output.kind, RESULT_INI_ARRAY);
            assert!(decode_ini_array(&bytes(&output), output.value as u64).is_some());
            unsafe { elephc_mbstring_release_v1(&mut output); }
        }
    }
    let (status, mut output) = invoke(INI_RESTORE, b"mbstring.language", b"", false, &mut Host::new("observe"));
    assert_eq!(status, 0);
    assert_eq!(output.kind, RESULT_NULL);
    unsafe { elephc_mbstring_release_v1(&mut output); }
    assert_eq!(call(RuntimeBuiltinId::MbLanguage, &[]), json!(["string", hex(b"neutral")]));
}

/// Retires successful scalar and graph leases when any final argument release reports a failure.
#[test]
fn mbstring_invoke_ini_discards_completed_leases_on_cleanup_failure() {
    for (operation, name, details) in [(INI_GET, "mbstring.language", false),
        (INI_GET_ALL, "mbstring", false), (INI_GET_ALL, "mbstring", true)] {
        for occurrence in 1..=4 {
            for failure in [1, 2, 3, -1] {
                elephc_mbstring_reset_v1();
                let (status, mut previous) = invoke(INI_SET, b"mbstring.language", b"Japanese", false, &mut Host::new("observe"));
                assert_eq!(status, 0);
                unsafe { elephc_mbstring_release_v1(&mut previous); }
                let (status, mut current) = invoke(INI_GET, b"mbstring.language", b"", false, &mut Host::new("observe"));
                assert_eq!(status, 0);
                assert_eq!(current.kind, RESULT_INI_STRING);
                let identity = current.value as u64;
                unsafe { elephc_mbstring_release_v1(&mut current); }
                assert_eq!(elephc_mbstring_ini_string_retain_v1(identity), 1);
                let mut host = Host::new("observe");
                host.fault = Some(Fault { callback: "release", occurrence, status: failure, malformed: false });
                let (status, mut output) = invoke(operation, name.as_bytes(), b"", details, &mut host);
                assert_eq!(status, if failure == 2 { 2 } else { 1 });
                assert_eq!(elephc_mbstring_ini_string_retain_v1(identity), 1,
                    "completed result leaked: {operation}/{details}/{occurrence}/{failure}");
                unsafe { elephc_mbstring_release_v1(&mut output); }
            }
        }
    }
}
