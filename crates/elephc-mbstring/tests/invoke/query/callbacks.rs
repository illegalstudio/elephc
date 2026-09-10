//! Purpose:
//! Implements protected query replay callbacks and explicit ownership/failure injection.
//!
//! Called from:
//! - The independent V5 query host used by shared invocation integration tests.
//!
//! Key details:
//! - Every published owner is tracked before callback status can fail.
//! - PHP-like actions reenter the real shared request APIs without borrowing their state.
//! - The storage model consumes wire plans and never repeats query-name normalization.

use super::*;

/// Suppresses fixture setup deprecations without changing the underlying shared INI operation.
pub(super) unsafe extern "C" fn ignore_diagnostic(_: *mut c_void, _: u32, _: *const u8, _: u64) -> i32 { 0 }

/// Initializes the exact supplied output and records previous-owner destructor effects first.
pub(super) unsafe extern "C" fn initialize(context: *mut c_void, original: *const c_void, out: *mut MbCaptureOutputV1) -> i32 {
    let replay = unsafe { &mut *context.cast::<Replay>() };
    let fault = replay.enter("query_initialize");
    assert_eq!(original, replay.original);
    if replay.case["reject_output"] == true {
        replay.host.pending = Some(json!(["typed output"]));
        unsafe { *out = MbCaptureOutputV1::default(); }
        return 2;
    }
    if replay.case["seed"] != "scalar" {
        replay.output = Output::Null;
        replay.events.push(json!(["destroy", Value::Null]));
        if replay.case["seed"] == "settings" {
            if set_ini_live(replay, b"mbstring.http_input", b"SJIS") == 0 {
                call(RuntimeBuiltinId::MbInternalEncoding, &[MbArgV1::string(b"UTF-8")]);
                call(RuntimeBuiltinId::MbSubstituteCharacter, &[MbArgV1::integer(33)]);
            }
        }
        if replay.case["seed"] == "throw" { replay.host.pending = Some(json!(["initialization"])); }
    }
    replay.output = Output::array();
    let writer = Box::into_raw(Box::new(original)).cast::<c_void>();
    replay.writers += 1;
    unsafe { *out = MbCaptureOutputV1 { ready: if fault.is_some_and(|fault| fault.malformed) { 2 } else { 1 }, writer }; }
    status(replay, fault).unwrap_or_else(|| if replay.host.pending.is_some() { 2 } else { 0 })
}

/// Rejects accidental routing of nested query writes through the flat regex capture callback.
pub(super) unsafe extern "C" fn unexpected_fill(_: *mut c_void, _: *mut c_void, _: *const u8, _: u64) -> i32 { 1 }

/// Consumes the writer token even when its release leaves a pending callback exception.
pub(super) unsafe extern "C" fn release(context: *mut c_void, writer: *mut c_void) -> i32 {
    let replay = unsafe { &mut *context.cast::<Replay>() };
    let fault = replay.enter("query_release");
    let original = unsafe { Box::from_raw(writer.cast::<*const c_void>()) };
    assert_eq!(*original, replay.original);
    replay.writers -= 1;
    status(replay, fault).unwrap_or(0)
}

/// Returns owned configuration bytes so success, malformed metadata, and failures exercise cleanup.
pub(super) unsafe extern "C" fn configuration(context: *mut c_void, phase: u32, out: *mut MbQueryConfigV1) -> i32 {
    let replay = unsafe { &mut *context.cast::<Replay>() };
    let name = match phase {
        QUERY_CONFIG_ENTRY => "query_config_entry", QUERY_CONFIG_FIELD => "query_config_field",
        QUERY_CONFIG_DIAGNOSTIC => "query_config_diagnostic", _ => panic!("invalid query config phase"),
    };
    let fault = replay.enter(name);
    let separators = replay.case["separator"].as_str().map(unhex).unwrap_or_else(|| b"&".to_vec());
    let separators = own_bytes(replay, separators);
    unsafe { *out = MbQueryConfigV1 { separators, max_variables: replay.case["max_vars"].as_i64().unwrap(),
        max_nesting: replay.case["max_nesting"].as_i64().unwrap(), display_errors: u64::from(replay.case["display_errors"] == true) }; }
    if fault.is_some_and(|fault| fault.malformed) { unsafe { (*out).display_errors = 2; } }
    status(replay, fault).unwrap_or(0)
}

/// Applies an optional host filter and publishes its byte lease even for a rejected field.
pub(super) unsafe extern "C" fn filter(context: *mut c_void, _: *const u8, _: u64,
    value: *const u8, len: u64, out: *mut MbQueryFilteredV1) -> i32 {
    let replay = unsafe { &mut *context.cast::<Replay>() };
    let fault = replay.enter("query_filter");
    let mut value = unsafe { std::slice::from_raw_parts(value, len as usize) }.to_vec();
    value.make_ascii_uppercase();
    let value = own_bytes(replay, value);
    unsafe { *out = MbQueryFilteredV1 { accepted: u64::from(replay.case["filter_reject"] != true), value }; }
    if fault.is_some_and(|fault| fault.malformed) { unsafe { (*out).accepted = 2; } }
    status(replay, fault).unwrap_or(0)
}

/// Executes wire mutations against live output and publishes actual nesting progress before status.
pub(super) unsafe extern "C" fn register(context: *mut c_void, writer: *mut c_void,
    steps: *const MbQueryStepV1, count: u64, value: *const u8, len: u64, out: *mut MbQueryRegisteredV1) -> i32 {
    let replay = unsafe { &mut *context.cast::<Replay>() };
    let fault = replay.enter("query_register");
    assert_eq!(unsafe { *writer.cast::<*const c_void>() }, replay.original);
    let steps = unsafe { std::slice::from_raw_parts(steps, count as usize) };
    let value = unsafe { std::slice::from_raw_parts(value, len as usize) };
    let exceeded = unsafe { replay.output.apply(steps, value) };
    if exceeded && replay.case.get("display_after_write").is_some() {
        replay.case["display_errors"] = replay.case["display_after_write"].clone();
    }
    unsafe { (*out).nesting_exceeded = if fault.is_some_and(|fault| fault.malformed) { 2 } else { u64::from(exceeded) }; }
    status(replay, fault).unwrap_or(0)
}

/// Performs the PHP oracle's live warning actions and preserves thrown diagnostic status.
pub(super) unsafe extern "C" fn diagnostic(context: *mut c_void, level: u32, bytes: *const u8, len: u64) -> i32 {
    let replay = unsafe { &mut *context.cast::<Replay>() };
    let fault = replay.enter("query_diagnostic");
    let message = String::from_utf8(unsafe { std::slice::from_raw_parts(bytes, len as usize) }.to_vec()).unwrap();
    replay.events.push(json!(["warning", level, message, replay.output.snapshot(), setting(RuntimeBuiltinId::MbHttpInput, &[])]));
    let previous = std::mem::replace(&mut replay.handling, true);
    let mut result = 0;
    match replay.case["action"].as_str().unwrap() {
        "scalar" => replay.output = Output::String(b"handler".to_vec()),
        "array" => {
            replay.output = Output::array();
            let step = MbQueryStepV1 { operation: QUERY_STORE, append: 0,
                key: MbHostValueV1 { tag: HOST_STRING, lo: b"handler".as_ptr() as u64, hi: 7 } };
            unsafe { replay.output.apply(&[step], b"kept"); }
        },
        "copy" => replay.copy = replay.output.clone(),
        "reference" => replay.copy_reference = true,
        "settings" => {
            call(RuntimeBuiltinId::MbSubstituteCharacter, &[MbArgV1::integer(33)]);
            call(RuntimeBuiltinId::MbInternalEncoding, &[MbArgV1::string(b"ASCII")]);
            set_ini_live(replay, b"mbstring.http_input", b"pass");
        },
        "nested" => {
            set_ini_live(replay, b"mbstring.http_input", b"SJIS");
            let mut nested_case = replay.case.clone();
            nested_case["seed"] = json!("scalar");
            nested_case["action"] = json!("none");
            let mut nested = Replay::new(nested_case);
            assert_eq!(nested.invoke(&[string(b"nested=value"), Php::Null], 0).0, 0);
            replay.events.push(json!(["nested", setting(RuntimeBuiltinId::MbHttpInput, &[])]));
        },
        "throw" => { replay.host.pending = Some(json!(["diagnostic"])); result = 2; },
        "none" => {},
        action => panic!("unknown query action {action}"),
    }
    replay.handling = previous;
    status(replay, fault).unwrap_or(result)
}

/// Models literal INI setters and PHP's suppression of recursive user error-handler invocation.
fn set_ini_live(replay: &mut Replay, name: &[u8], value: &[u8]) -> i32 {
    let silent = MbIniHostV1 { version: 1, size: std::mem::size_of::<MbIniHostV1>() as u32,
        context: std::ptr::null_mut(), diagnostic: Some(ignore_diagnostic) };
    let mut literal = MbResultV1::default();
    let source = MbArgV1::string(value);
    assert_eq!(unsafe { elephc_mbstring_ini_v1(INI_STRING_INTERNED, &source, 1, &silent, &mut literal) }, 0);
    assert_eq!(literal.kind, RESULT_INI_STRING);
    let arguments = [MbArgV1::string(name), MbArgV1 { kind: ARG_INI_STRING, value: literal.value, bytes: std::ptr::null(), len: 0 }];
    let host = MbIniHostV1 { context: (replay as *mut Replay).cast(),
        diagnostic: Some(if replay.handling { ignore_diagnostic } else { diagnostic }), ..silent };
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_ini_v1(INI_SET, arguments.as_ptr(), 2, &host, &mut output) };
    unsafe { elephc_mbstring_release_v1(&mut output); elephc_mbstring_release_v1(&mut literal); }
    status
}

/// Publishes bytes under the same owned-value protocol consumed by inherited release callbacks.
fn own_bytes(replay: &mut Replay, bytes: Vec<u8>) -> MbHostStringV1 {
    let pointer = bytes.as_ptr();
    let len = bytes.len() as u64;
    let owner = Box::into_raw(Box::new(Php::String(bytes))).cast::<c_void>();
    assert!(replay.host.live.insert(owner as usize));
    MbHostStringV1 { bytes: pointer, len, owner }
}

/// Records a protected test exception before reporting an injected callback status.
fn status(replay: &mut Replay, fault: Option<Fault>) -> Option<i32> {
    fault.map(|fault| {
        if fault.status == 2 { replay.host.pending = Some(json!(["injected"])); }
        fault.status
    })
}
