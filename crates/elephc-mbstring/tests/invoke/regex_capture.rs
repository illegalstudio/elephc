//! Purpose:
//! Replays independent PHP capture traces through the protected reference-output C ABI.
//!
//! Called from:
//! - The invoke integration binary with the managed Oniguruma provider.
//!
//! Key details:
//! - This independent host models PHP lvalues and records actual ABI ownership actions.
//! - Construction-array aliases remain distinct from ordinary by-value argument copies.
//! - Native/eval adapters still need their own end-to-end coverage before public registration.

use super::*;
use std::{cell::RefCell, rc::Rc};
use elephc_builtin_contract::mbstring_abi::invoke::{MbInvokeHostV4, MbCaptureOutputV1};
use elephc_builtin_contract::mbstring_abi::coercion::PREPARED_ARGUMENT_COUNT_ERROR;
use elephc_builtin_contract::mbstring_abi::host::{MbHostValueV1, HOST_BOOL, HOST_INT, HOST_STRING};

/// Models an observable output value and the identity of arrays being filled by PHP internals.
#[derive(Clone)]
enum Output { Scalar(Value), Array(Rc<RefCell<Vec<Value>>>) }

/// Retains only the caller's reference token, leaving its current value in the host's live slot.
struct Writer { original: *const c_void }

impl Output {
    /// Allocates an empty construction array without retaining the previous output value.
    fn array() -> Self { Self::Array(Rc::new(RefCell::new(Vec::new()))) }

    /// Copies observable contents while keeping construction identity in the host itself.
    fn snapshot(&self) -> Value {
        match self { Self::Scalar(value) => value.clone(), Self::Array(entries) => json!({"array": *entries.borrow()}) }
    }

    /// Writes one exact key without COW separation of aliases exposed during initialization.
    fn insert(&self, entry: Value) {
        let Self::Array(entries) = self else { panic!("writer lost its construction array"); };
        let mut entries = entries.borrow_mut();
        if let Some(previous) = entries.iter_mut().find(|old| old[0] == entry[0]) { *previous = entry; }
        else { entries.push(entry); }
    }
}

/// Keeps the established value host first so inherited callbacks share its exact context address.
#[repr(C)]
struct CaptureReplay {
    host: Host, case: Value, output: Output, copy: Output, initialization: Vec<Value>,
    actions: Vec<Value>, warnings: Vec<String>, matches_at_warning: Vec<Value>, callbacks: Vec<Vec<Value>>,
    original: *const c_void, writers: usize, fault: Option<(&'static str, i32, u64)>,
    stores: usize, overwritten: Vec<Value>,
}

impl CaptureReplay {
    /// Starts one independent PHP observation with no live construction writer.
    fn new(case: &Value) -> Self {
        let seed = if case["storage"] == "int" { json!(17) }
            else if case.get("storage").is_some() { json!({"object": "CaptureOutputOwner"}) }
            else { case.get("matches").cloned().unwrap_or_else(|| encoded(b"old")) };
        Self { host: Host::new(""), case: case.clone(), output: Output::Scalar(seed), copy: Output::Scalar(Value::Null),
            initialization: Vec::new(), actions: Vec::new(), warnings: Vec::new(), matches_at_warning: Vec::new(),
            callbacks: Vec::new(), original: std::ptr::null(), writers: 0, fault: None,
            stores: 0, overwritten: Vec::new() }
    }

    /// Calls typed shared dispatch with separate string values and a pinned reference token.
    fn invoke(&mut self, arguments: &[Php], strict: u32) -> Result<Value, Value> {
        self.invoke_via(arguments, strict, false)
    }

    /// Selects typed dispatch or its legacy compatibility entry over the same independent host.
    fn invoke_via(&mut self, arguments: &[Php], strict: u32, legacy: bool) -> Result<Value, Value> {
        let pointers: Vec<_> = arguments.iter().map(|value| (value as *const Php).cast::<c_void>()).collect();
        self.original = pointers.get(2).copied().unwrap_or(std::ptr::null());
        let mut base = self.host.table_v3();
        base.base.base.version = 4;
        base.base.base.size = std::mem::size_of::<MbInvokeHostV4>() as u32;
        base.base.base.diagnostic = Some(diagnostic);
        let table = MbInvokeHostV4 { base, capture_initialize: Some(initialize),
            capture_fill: Some(fill), capture_release: Some(release) };
        let mut result = MbResultV1::default();
        let ignore_case = u32::from(matches!(self.case["op"].as_str(), Some("eregi" | "mb_eregi")));
        let operation = if ignore_case == 0 { RuntimeBuiltinId::MbEreg } else { RuntimeBuiltinId::MbEregi };
        let status = unsafe {
            if legacy {
                elephc_mbstring_capture_v1(ignore_case, pointers.as_ptr(), pointers.len() as u64,
                    strict, &table.base.base.base, &mut result)
            } else {
                elephc_mbstring_invoke_v1(operation.as_u32(), pointers.as_ptr(), pointers.len() as u64,
                    strict, &table.base.base.base, &mut result)
            }
        };
        assert!(self.host.live.is_empty() && self.host.errors.is_empty(), "{:?}", self.host.errors);
        assert_eq!(self.writers, 0, "capture writer leaked");
        match status {
            0 => decode_wire(result),
            2 => { unsafe { elephc_mbstring_release_v1(&mut result); } Err(self.host.pending.clone().expect("pending throwable")) },
            _ => { unsafe { elephc_mbstring_release_v1(&mut result); } Err(json!(["fatal"])) },
        }
    }
}

/// Performs the PHP-oracle initialization actions while retaining only caller reference identity.
unsafe extern "C" fn initialize(context: *mut c_void, original: *const c_void, out: *mut MbCaptureOutputV1) -> i32 {
    let replay = unsafe { &mut *context.cast::<CaptureReplay>() };
    assert_eq!(original, replay.original);
    assert_eq!(replay.host.events.iter().filter(|&&event| event == "clone").count(), 2,
        "output contents must not be copied by value");
    replay.host.events.push("capture_initialize");
    let storage = replay.case["storage"].as_str().unwrap_or("ordinary");
    if matches!(storage, "int" | "object") {
        let (class, ty) = if storage == "int" { ("CaptureIntProperty", "int") }
            else { ("CaptureObjectProperty", "CaptureOutputOwner") };
        let message = format!("Cannot assign array to reference held by property {class}::$value of type {ty}");
        replay.host.pending = Some(json!(["TypeError", hex(message.as_bytes()), null]));
        return 2;
    }
    replay.output = if storage == "local" { Output::Scalar(Value::Null) } else { Output::array() };
    if storage != "ordinary" {
        replay.initialization.push(replay.output.snapshot());
        for step in replay.case["actions"].as_array().unwrap() { replay.actions.push(super::run(step)); }
        if replay.case["mutate"] == true {
            if matches!(replay.output, Output::Scalar(_)) { replay.output = Output::array(); }
            replay.output.insert(json!([encoded(b"kept"), encoded(b"initialization")]));
            replay.copy = replay.output.clone();
        }
        if replay.case["throw"] == true { replay.host.pending = Some(json!(["RuntimeException", hex(b"output cleanup"), null])); }
        if storage == "local" { replay.output = Output::array(); }
    }
    if replay.case["capture_retarget"] == true {
        replay.output.insert(json!([0, {"object": "ReassignCaptureOutput"}]));
        replay.copy = replay.output.clone();
    }
    let writer = Box::into_raw(Box::new(Writer { original })).cast();
    replay.writers += 1;
    unsafe { *out = MbCaptureOutputV1 { ready: 1, writer }; }
    if let Some(("initialize", status, ready)) = replay.fault {
        unsafe { (*out).ready = ready; }
        if status == 2 { replay.host.pending = Some(json!(["RuntimeException", hex(b"injected initialization"), null])); }
        return status;
    }
    if replay.host.pending.is_some() { 2 } else { 0 }
}

/// Applies the real shared graph writer without holding host borrows across its callbacks.
unsafe extern "C" fn fill(context: *mut c_void, writer: *mut c_void, data: *const u8, len: u64) -> i32 {
    unsafe { (*context.cast::<CaptureReplay>()).host.events.push("capture_fill"); }
    let status = unsafe { elephc_mbstring_capture_apply_v1(context, writer, data, len, Some(store_capture)) };
    let replay = unsafe { &mut *context.cast::<CaptureReplay>() };
    if status == 1 { return status; }
    if let Some(("fill", status, _)) = replay.fault {
        if status == 2 { replay.host.pending = Some(json!(["RuntimeException", hex(b"injected fill"), null])); }
        return status;
    }
    if replay.host.pending.is_some() { 2 } else { 0 }
}

/// Resolves the live reference for one entry and finishes that write after modeled destruction.
unsafe extern "C" fn store_capture(context: *mut c_void, writer: *mut c_void,
    key: *const MbHostValueV1, value: *const MbHostValueV1) -> i32 {
    let replay = unsafe { &mut *context.cast::<CaptureReplay>() };
    assert_eq!(unsafe { (*writer.cast::<Writer>()).original }, replay.original);
    let key = unsafe { capture_value(&*key) };
    let value = unsafe { capture_value(&*value) };
    let destination = replay.output.clone();
    if replay.stores == 0 && replay.case["capture_retarget"] == true {
        replay.overwritten.push(json!("overwrite"));
        replay.output = Output::array();
        replay.output.insert(json!([encoded(b"replacement"), encoded(b"new")]));
        if replay.case["capture_throw"] == true {
            replay.host.pending = Some(json!(["RuntimeException", hex(b"capture overwrite"), null]));
        }
    }
    destination.insert(json!([key, value]));
    replay.stores += 1;
    if replay.host.pending.is_some() { 2 } else { 0 }
}

/// Copies a borrowed native capture descriptor into the independent PHP trace encoding.
unsafe fn capture_value(value: &MbHostValueV1) -> Value {
    match value.tag {
        HOST_INT => json!(value.lo as i64),
        HOST_BOOL => json!(value.lo != 0),
        HOST_STRING => encoded(unsafe { std::slice::from_raw_parts(value.lo as *const u8, value.hi as usize) }),
        tag => panic!("unexpected capture descriptor {tag}"),
    }
}

/// Consumes the writer even on cleanup failure, before the invocation releases argument pins.
unsafe extern "C" fn release(context: *mut c_void, writer: *mut c_void) -> i32 {
    let replay = unsafe { &mut *context.cast::<CaptureReplay>() };
    replay.host.events.push("capture_release");
    unsafe { drop(Box::from_raw(writer.cast::<Writer>())); }
    replay.writers -= 1;
    if let Some(("release", status, _)) = replay.fault {
        if status == 2 { replay.host.pending = Some(json!(["RuntimeException", hex(b"injected release"), null])); }
        return status;
    }
    0
}

/// Observes initialized output at warning delivery and reenters the real exported regex operations.
unsafe extern "C" fn diagnostic(context: *mut c_void, level: u32, data: *const u8, len: u64) -> i32 {
    let replay = unsafe { &mut *context.cast::<CaptureReplay>() };
    assert_eq!(level, 2);
    replay.warnings.push(hex(unsafe { std::slice::from_raw_parts(data, len as usize) }));
    replay.matches_at_warning.push(replay.output.snapshot());
    if let Some(value) = replay.case.get("warning_matches") { replay.output = Output::Scalar(value.clone()); }
    if replay.callbacks.is_empty() {
        if let Some(actions) = replay.case["on_warning"].as_array() {
            replay.callbacks.push(actions.iter().map(super::run).collect());
            if replay.case["throw"] == true {
                replay.host.pending = Some(json!(["RuntimeException", hex(b"regex handler failed"), null]));
                return 2;
            }
        }
    }
    0
}

/// Supplies an opaque PHP reference token that inherited pinning retains without copying its contents.
fn output_token() -> Php { Php::Reference(Rc::new(RefCell::new(Php::Null))) }

/// Builds the two value parameters and preserves the optional output argument's actual presence.
fn arguments(case: &Value) -> Vec<Php> {
    let mut args = vec![Php::String(bytes(&case["pattern"]).unwrap()), Php::String(bytes(&case["subject"]).unwrap())];
    if case["with_matches"] != false { args.push(output_token()); }
    args
}

/// Replays an ordinary capture step with exact warnings, optional outputs, and live request settings.
pub(super) fn run(step: &Value) -> Value {
    let mut replay = CaptureReplay::new(step);
    let result = replay.invoke(&arguments(step), 0);
    let mut trace = json!({"warnings": replay.warnings, "matches": replay.output.snapshot(),
        "matches_at_warning": replay.matches_at_warning, "position": getter(RuntimeBuiltinId::MbEregSearchGetpos),
        "encoding": setting(RuntimeBuiltinId::MbRegexEncoding), "options": setting(RuntimeBuiltinId::MbRegexSetOptions)});
    if step.get("on_warning").is_some() { trace["callbacks"] = json!(replay.callbacks); }
    match result { Ok(value) => trace["value"] = value, Err(error) => trace["error"] = error }
    trace
}

/// Rejects malformed typed calls before reading a host, including counts too large for pointer slices.
#[test]
fn mbstring_invoke_regex_capture_validates_before_host_access() {
    for operation in [RuntimeBuiltinId::MbEreg, RuntimeBuiltinId::MbEregi] {
        for count in [0, 1, 4, u64::MAX] {
            let mut out = MbResultV1::default();
            assert_eq!(unsafe { elephc_mbstring_invoke_v1(operation.as_u32(), std::ptr::dangling(),
                count, 0, std::ptr::dangling(), &mut out) }, 0);
            assert_eq!(out.kind, PREPARED_ARGUMENT_COUNT_ERROR);
            unsafe { elephc_mbstring_release_v1(&mut out); }
        }
        let mut out = MbResultV1::default();
        assert_eq!(unsafe { elephc_mbstring_invoke_v1(operation.as_u32(), std::ptr::dangling(),
            2, 2, std::ptr::dangling(), &mut out) }, 1);
        unsafe { elephc_mbstring_release_v1(&mut out); }
        assert_eq!(unsafe { elephc_mbstring_invoke_v1(operation.as_u32(), std::ptr::null(),
            2, 0, std::ptr::dangling(), &mut out) }, 1);
        unsafe { elephc_mbstring_release_v1(&mut out); }
    }
}

/// Checks all ordinary capture traces through typed runtime dispatch and its common coercion boundary.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_capture_php_corpus() {
    replay_corpus(include_bytes!("../fixtures/regex_capture.jsonl.gz"), 718);
}

/// Checks pending exceptions, typed constraints, destructor reentry, and by-value construction aliases.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_capture_output_php_corpus() {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    let reader = flate2::read::GzDecoder::new(include_bytes!("../fixtures/regex_output.jsonl.gz").as_slice());
    let mut count = 0;
    for line in BufReader::new(reader).lines() {
        elephc_mbstring_reset_v1();
        call(RuntimeBuiltinId::MbRegexSetOptions, &[MbArgV1::string(b"pr")]);
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        for step in case["before"].as_array().unwrap() { super::run(step); }
        let mut replay = CaptureReplay::new(&case);
        let result = replay.invoke(&arguments(&case), 0);
        let mut trace = json!({"matches": replay.output.snapshot(), "copy": replay.copy.snapshot(),
            "initialization": replay.initialization, "actions": replay.actions, "warnings": replay.warnings,
            "matches_at_warning": replay.matches_at_warning, "encoding": setting(RuntimeBuiltinId::MbRegexEncoding),
            "options": setting(RuntimeBuiltinId::MbRegexSetOptions), "position": getter(RuntimeBuiltinId::MbEregSearchGetpos),
            "registers": getter(RuntimeBuiltinId::MbEregSearchGetregs)});
        match result { Ok(value) => trace["value"] = value, Err(error) => trace["error"] = error }
        assert_eq!(trace, case["trace"], "case {count}: {case}");
        count += 1;
    }
    assert_eq!(count, 1200);
}

/// Replays PHP destructor-driven reference replacement between numeric and named capture writes.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_capture_retargets_between_entries() {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    let cases: Vec<Value> = serde_json::from_str(include_str!("../fixtures/regex_retarget.json")).unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        elephc_mbstring_reset_v1();
        let mut replay = CaptureReplay::new(&case);
        let result = replay.invoke(&arguments(&case), 0);
        let mut trace = json!({"matches": replay.output.snapshot(), "copy": replay.copy.snapshot(),
            "events": replay.overwritten});
        match result { Ok(value) => trace["value"] = value, Err(error) => trace["error"] = error }
        assert_eq!(trace, case["trace"], "{case}");
        assert_eq!(replay.stores, 5, "pending destruction must not skip later captures");
    }
}

/// Exercises every writer failure edge, malformed readiness, legacy-host rejection, and pre-host arity checks.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_capture_protocol_failures() {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    let case = json!({"op": "ereg", "pattern": hex(b"(a)"), "subject": hex(b"a")});
    for action in ["initialize", "fill", "release"] {
        for status in [1, 2, 3, 254, -1] {
            elephc_mbstring_reset_v1();
            let mut replay = CaptureReplay::new(&case);
            replay.fault = Some((action, status, 1));
            assert!(replay.invoke(&arguments(&case), 0).is_err());
            let filled = replay.host.events.contains(&"capture_fill");
            assert_eq!(filled, action != "initialize" || status == 2);
        }
    }
    for (status, ready) in [(0, 0), (0, 2), (2, 0), (2, 2)] {
        let mut replay = CaptureReplay::new(&case);
        replay.fault = Some(("initialize", status, ready));
        assert!(replay.invoke(&arguments(&case), 0).is_err());
        assert!(!replay.host.events.contains(&"capture_fill"));
    }
    let mut host = Host::new("");
    let table = host.table_v3();
    let mut out = MbResultV1::default();
    let args = arguments(&case);
    let pointers: Vec<_> = args.iter().map(|value| (value as *const Php).cast::<c_void>()).collect();
    assert_eq!(unsafe { elephc_mbstring_capture_v1(0, pointers.as_ptr(), 3, 0, &table.base.base, &mut out) }, 1);
    unsafe { elephc_mbstring_release_v1(&mut out); }
    assert!(host.events.is_empty());
    for count in [0, 1, 4, u64::MAX] {
        assert_eq!(unsafe { elephc_mbstring_capture_v1(0, std::ptr::dangling(), count, 0, std::ptr::dangling(), &mut out) }, 0);
        assert_eq!(out.kind, PREPARED_ARGUMENT_COUNT_ERROR);
        unsafe { elephc_mbstring_release_v1(&mut out); }
    }
}

/// Verifies common coercion completes before output mutation and retains compatibility with older value hosts.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_capture_coercion_and_legacy_hosts() {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    let case = json!({"op": "ereg", "pattern": hex(b"a"), "subject": hex(b"a")});
    for (first, strict) in [(Php::Int(1), 1), (Php::Array(Rc::new(Vec::new())), 0)] {
        let mut replay = CaptureReplay::new(&case);
        let error = replay.invoke(&[first, string(b"a"), output_token()], strict).unwrap_err();
        assert_eq!(error[0], "TypeError");
        assert_eq!(replay.output.snapshot(), encoded(b"old"));
        assert!(!replay.host.events.contains(&"capture_initialize"));
    }
    let mut replay = CaptureReplay::new(&case);
    let args = [object("pattern", b"a", Action::None), object("subject", b"a", Action::None), output_token()];
    assert_eq!(replay.invoke(&args, 0), Ok(json!(true)));
    assert_eq!(replay.host.trace, vec![json!(["stringify", "pattern"]), json!(["stringify", "subject"])]);
    assert_eq!(replay.output.snapshot(), json!({"array": [[0, encoded(b"a")]]}));
    let mut host = Host::new("");
    let table = host.table();
    let args = [string(b"a"), string(b"a")];
    let pointers: Vec<_> = args.iter().map(|value| (value as *const Php).cast::<c_void>()).collect();
    let mut out = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_capture_v1(0, pointers.as_ptr(), 2, 0, &table, &mut out) }, 0);
    assert_eq!(decode_wire(out), Ok(json!(true)));
    assert!(host.live.is_empty() && host.errors.is_empty());
}

/// Preserves the older capture entry while typed IDs select case sensitivity and output ownership.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_capture_legacy_entry_matches_typed_dispatch() {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    for (op, matched) in [("ereg", false), ("eregi", true)] {
        let case = json!({"op": op, "pattern": hex(b"(?<letter>a)"), "subject": hex(b"A")});
        let mut typed = CaptureReplay::new(&case);
        let mut legacy = CaptureReplay::new(&case);
        elephc_mbstring_reset_v1();
        assert_eq!(typed.invoke(&arguments(&case), 0), Ok(json!(matched)));
        elephc_mbstring_reset_v1();
        assert_eq!(legacy.invoke_via(&arguments(&case), 0, true), Ok(json!(matched)));
        assert_eq!(typed.output.snapshot(), legacy.output.snapshot());
        assert_eq!(typed.host.events, legacy.host.events);
        assert_eq!(typed.warnings, legacy.warnings);
        assert_eq!(typed.stores, if matched { 3 } else { 0 });
    }
}
