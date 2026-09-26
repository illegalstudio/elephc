//! Purpose:
//! Checks ordered callback preparation and explicit result ownership through the public mbregex ABI.
//!
//! Called from:
//! - The focused shared-invocation integration test harness.
//!
//! Key details:
//! - A fake value host records every acquired/released box independently of the engine.
//! - Ordinary PHP callback failures return a pending status without native unwinding.

use super::*;
use elephc_builtin_contract::mbstring_abi::{array::{ArrayGraph, Key, Value as GraphValue},
    callback::{MbCallbackCallV1, MbCallbackHostV1}};

/// Keeps callback behavior separate from the value host's owner accounting.
struct Callback {
    host: *mut Host,
    value: Php,
    captures: Vec<ArrayGraph>,
    fail_resolve: bool,
    fail_call: bool,
}

/// Resolves the original callable at argument two and returns a separately tracked callback owner.
unsafe extern "C" fn resolve(context: *mut c_void, value: *const c_void, out: *mut *mut c_void) -> i32 {
    let callback = unsafe { &mut *context.cast::<Callback>() };
    let host = unsafe { &mut *callback.host };
    host.trace.push(json!(["resolve"]));
    if callback.fail_resolve { host.pending = Some(json!("invalid callback")); return 2; }
    let table = host.table();
    unsafe { table.clone_value.unwrap()(table.context, value, out) }
}

/// Copies the capture graph before publishing an independent PHP callback result or ordinary throw.
unsafe extern "C" fn invoke(context: *mut c_void, _: *const c_void, call: *mut MbCallbackCallV1) -> i32 {
    let callback = unsafe { &mut *context.cast::<Callback>() };
    let host = unsafe { &mut *callback.host };
    host.trace.push(json!(["replace"]));
    let call = unsafe { &mut *call };
    let bytes = unsafe { std::slice::from_raw_parts(call.captures, call.captures_len as usize) };
    let Some(graph) = ArrayGraph::decode(bytes) else { return 1; };
    callback.captures.push(graph);
    if callback.fail_call { host.pending = Some(json!("callback stopped")); return 2; }
    let table = host.table();
    unsafe { table.clone_value.unwrap()(table.context, (&callback.value as *const Php).cast(), &mut call.result) }
}

/// Invokes the actual coordinator and requires complete cleanup after success or a pending callback error.
fn run_callback(values: &[Php], returned: Php, fail_resolve: bool, fail_call: bool) -> (i32, Value, Vec<Value>, Vec<ArrayGraph>) {
    elephc_mbstring_reset_v1();
    let mut host = Host::new("");
    let table = host.table();
    let mut callback = Callback { host: &mut host, value: returned, captures: Vec::new(), fail_resolve, fail_call };
    let callbacks = MbCallbackHostV1 { version: 1, size: std::mem::size_of::<MbCallbackHostV1>() as u32,
        context: (&mut callback as *mut Callback).cast(), resolve: Some(resolve), invoke: Some(invoke) };
    let values: Vec<_> = values.iter().map(|value| (value as *const Php).cast()).collect();
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_callback_invoke_v1(values.as_ptr(), values.len() as u64, 0, &table, &callbacks, &mut output) };
    assert!(host.live.is_empty(), "unreleased callback owners: {:?}", host.live);
    assert!(host.errors.is_empty(), "{:?}", host.errors);
    let result = if status == 0 { result(output) } else {
        unsafe { elephc_mbstring_release_v1(&mut output); }
        host.pending.clone().unwrap_or(Value::Null)
    };
    (status, result, host.trace, callback.captures)
}

/// Resolves callbacks between pattern and subject Stringable conversions and preserves capture ordering.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_replacement_callback_order_and_values() {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    let values = [Php::Object { label: "pattern", bytes: b"(?<name>a)(?<tail>b)?".to_vec(), action: Action::None }, string(b"callback"),
        Php::Object { label: "subject", bytes: b"a ab".to_vec(), action: Action::None }];
    for (returned, bytes) in [(Php::Int(12), b"12 12".as_slice()), (Php::Bool(false), b" "), (string(b"\\1"), b"\\1 \\1")] {
        let (status, value, trace, captures) = run_callback(&values, returned, false, false);
        assert_eq!(status, 0);
        assert_eq!(value, json!(["string", hex(bytes)]));
        assert_eq!(trace, vec![json!(["stringify", "pattern"]), json!(["resolve"]), json!(["stringify", "subject"]),
            json!(["replace"]), json!(["replace"])]);
        let entries = &captures[0].arrays()[captures[0].root()];
        assert!(entries.contains(&(Key::Int(2), GraphValue::String(Vec::new()))));
        assert!(entries.contains(&(Key::String(b"name".to_vec()), GraphValue::String(b"a".to_vec()))));
        assert_eq!(captures.len(), 2);
    }
}

/// Stops ordered coercion after failed resolution and stops later matches after a PHP callback throw.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_replacement_callback_stops_after_throw() {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    let values = [string(b"a"), string(b"callback"),
        Php::Object { label: "subject", bytes: b"a a".to_vec(), action: Action::None }];
    let (status, value, trace, captures) = run_callback(&values, Php::Null, true, false);
    assert_eq!((status, value), (2, json!("invalid callback")));
    assert_eq!(trace, vec![json!(["resolve"])]);
    assert!(captures.is_empty());
    let (status, value, trace, captures) = run_callback(&values, Php::Null, false, true);
    assert_eq!((status, value), (2, json!("callback stopped")));
    assert_eq!(trace, vec![json!(["resolve"]), json!(["stringify", "subject"]), json!(["replace"])]);
    assert_eq!(captures.len(), 1);
}
