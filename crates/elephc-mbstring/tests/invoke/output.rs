//! Purpose:
//! Verifies public output invocation through independent value callbacks and real response metadata.
//!
//! Called from:
//! - The focused mbstring invocation integration harness.
//!
//! Key details:
//! - Arity precedes host access; every argument is copied before ordered scalar coercion.
//! - Separate response context carries the actual handler flag while value callbacks retain owners.

use super::*;
use std::{cell::RefCell, rc::Rc};
use elephc_builtin_contract::mbstring_abi::{output_handler::*, *};

/// Reports an unexpected header publication because these calls model execution inside a handler.
unsafe extern "C" fn header(_: *mut c_void, _: *const u8, _: u64) -> i32 { 1 }

/// Invokes public argument preparation with a distinct live handler flag and consumes its wire result.
fn run_output(values: &[Php], strict: bool, host: &mut Host) -> (i32, Value) {
    let pointers: Vec<_> = values.iter().map(|value| (value as *const Php).cast::<c_void>()).collect();
    let mut in_handler = 1_u64;
    let response = MbOutputHostV1 { version: 1, size: std::mem::size_of::<MbOutputHostV1>() as u32,
        context: (&mut in_handler as *mut u64).cast(), info: Some(elephc_mbstring_response_info_v1), header: Some(header) };
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_output_invoke_v1(pointers.as_ptr(), pointers.len() as u64,
        strict as u32, &host.table(), &response, &mut output) };
    (status, result(output))
}

/// Checks exact arity without consulting missing hosts or argument storage.
#[test]
fn mbstring_output_invoke_arity_before_hosts() {
    for count in [0, 1, 3, u64::MAX] {
        let mut output = MbResultV1::default();
        assert_eq!(unsafe { elephc_mbstring_output_invoke_v1(std::ptr::null(), count, 0,
            std::ptr::null(), std::ptr::null(), &mut output) }, 0);
        let output = result(output);
        assert_eq!(output[1], "ArgumentCountError");
        assert_eq!(output[2], hex(format!("mb_output_handler() expects exactly 2 arguments, {count} given").as_bytes()));
    }
}

/// Preserves binary strings, weak scalar conversions, and strict public parameter diagnostics.
#[test]
fn mbstring_output_invoke_scalar_contract() {
    elephc_mbstring_reset_v1();
    for (values, strict, expected) in [
        (vec![string(b"a\0b"), Php::Int(9)], true, json!(["string", "610062"])),
        (vec![Php::Int(123), string(b"9")], false, json!(["string", "313233"])),
        (vec![Php::Bool(true), Php::Bool(true)], false, json!(["string", "31"])),
        (vec![Php::Int(123), Php::Int(9)], true, json!(["error", "TypeError", hex(b"mb_output_handler(): Argument #1 ($string) must be of type string, int given")])),
        (vec![string(b"a"), string(b"9")], true, json!(["error", "TypeError", hex(b"mb_output_handler(): Argument #2 ($status) must be of type int, string given")])),
    ] {
        let mut host = Host::new("observe");
        assert_eq!(run_output(&values, strict, &mut host), (0, expected));
        assert!(host.live.is_empty() && host.errors.is_empty());
    }
}

/// Retains the original phase before Stringable mutates its caller reference and conversion begins.
#[test]
fn mbstring_output_invoke_copies_before_coercion() {
    elephc_mbstring_reset_v1();
    assert_eq!(call(RuntimeBuiltinId::MbHttpOutput, &[MbArgV1::string(b"UTF-16LE")]), json!(["bool", true]));
    let phase = Rc::new(RefCell::new(Php::Int(9)));
    let values = [object("bytes", "é".as_bytes(), Action::Assign(phase.clone(), Box::new(Php::Int(0)))), Php::Reference(phase.clone())];
    let mut host = Host::new("observe");
    assert_eq!(run_output(&values, false, &mut host), (0, json!(["string", "e900"])));
    assert!(matches!(*phase.borrow(), Php::Int(0)));
    assert_eq!(&host.events[..2], ["clone", "clone"]);
    assert_eq!(host.trace, [json!(["stringify", "bytes"])]);
    assert!(host.live.is_empty() && host.errors.is_empty());
}

/// Stops at invalid phase coercion after the earlier Stringable action, with no response conversion.
#[test]
fn mbstring_output_invoke_ordered_type_failure() {
    elephc_mbstring_reset_v1();
    let mut host = Host::new("observe");
    let values = [object("bytes", b"abc", Action::None), string(b"invalid")];
    let (status, output) = run_output(&values, false, &mut host);
    assert_eq!(status, 0);
    assert_eq!(output[1], "TypeError");
    assert_eq!(host.trace, [json!(["stringify", "bytes"])]);
    assert!(host.live.is_empty() && host.errors.is_empty());
}

/// Releases all value and metadata owners when a protected input callback or final cleanup fails.
#[test]
fn mbstring_output_invoke_cleanup_failures() {
    for callback in ["clone", "describe", "stringable", "release"] {
        for status in [1, 2] {
            elephc_mbstring_reset_v1();
            let mut host = Host::new("observe");
            host.fault = Some(Fault { callback, occurrence: 1, status, malformed: false });
            let values = [object("bytes", b"abc", Action::None), Php::Int(9)];
            assert_eq!(run_output(&values, false, &mut host).0, status, "{callback}");
            assert!(host.live.is_empty() && host.errors.is_empty(), "{callback}");
        }
    }
}
