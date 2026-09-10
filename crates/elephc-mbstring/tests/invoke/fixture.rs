//! Purpose:
//! Models PHP value copies, references, COW arrays, and protected callbacks for invocation tests.
//!
//! Called from:
//! - The focused shared-invocation ABI integration tests.
//!
//! Key details:
//! - Callback traces and observable results are compared with independently captured PHP cases.
//! - Every owner is tracked; callbacks reenter the real request ABI to expose borrow violations.
//! - Float formatting is a fixture action, not a claim of production host formatter parity.

#[path = "callbacks.rs"]
mod callbacks;
#[path = "graph_host.rs"]
mod graph_host;

use std::{cell::RefCell, collections::HashSet, ffi::c_void, rc::Rc};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{*, coercion::*, host::*, invoke::*}};
use elephc_mbstring::abi::*;
use serde_json::{json, Value};

/// A retained PHP value whose references and array payloads share identity across value copies.
#[derive(Clone)]
pub enum Php {
    Null, Bool(bool), Int(i64), Float(f64), String(Vec<u8>), Array(Rc<Vec<Php>>),
    Reference(Rc<RefCell<Php>>), Object { label: &'static str, bytes: Vec<u8>, action: Action },
}

/// A PHP-visible side effect performed when a fixture Stringable is converted.
#[derive(Clone)]
pub enum Action { None, Assign(Rc<RefCell<Php>>, Box<Php>), Cow(Rc<RefCell<Php>>), Internal, Language(&'static [u8]), Throw }

/// Injects a callback status or malformed success at one selected invocation.
#[derive(Clone, Copy)]
pub struct Fault { pub callback: &'static str, pub occurrence: usize, pub status: i32, pub malformed: bool }

/// Tracks native ownership separately from PHP-visible callback traces and caller storage.
pub struct Host {
    pub trace: Vec<Value>,
    pub events: Vec<&'static str>,
    pub live: HashSet<usize>,
    pub errors: Vec<String>,
    pub pending: Option<Value>,
    pub fault: Option<Fault>,
    pub handler: String,
    pub precision: u32,
    pub catalog: Option<Rc<Vec<Php>>>,
    pub observer: Option<(&'static str, Rc<RefCell<Php>>)>,
}

impl Host {
    /// Creates a clean callback host without owning or borrowing the shared request state.
    pub fn new(handler: &str) -> Self {
        Self { trace: Vec::new(), events: Vec::new(), live: HashSet::new(), errors: Vec::new(), pending: None,
            fault: None, handler: handler.into(), precision: 14, catalog: None, observer: None }
    }

    /// Publishes an independently releasable fake native box and records its exact pointer identity.
    fn own(&mut self, value: Php) -> *mut c_void {
        let pointer = Box::into_raw(Box::new(value)).cast::<c_void>();
        self.live.insert(pointer as usize);
        pointer
    }

    /// Records a callback, reenters the real request API, and returns its optional injected fault.
    fn enter(&mut self, callback: &'static str) -> Option<Fault> {
        self.events.push(callback);
        let output = call(RuntimeBuiltinId::MbInternalEncoding, &[]);
        if !valid_encoding_result(&output) {
            self.errors.push(format!("request reentry failed in {callback}: {output}"));
        }
        self.fault.filter(|fault| fault.callback == callback
            && self.events.iter().filter(|&&entry| entry == callback).count() == fault.occurrence)
    }

    /// Supplies all C callback entries with stable context storage for the enclosing invocation.
    pub fn table(&mut self) -> MbInvokeHostV1 {
        MbInvokeHostV1 { version: 1, size: std::mem::size_of::<MbInvokeHostV1>() as u32,
            context: (self as *mut Self).cast(), clone_value: Some(callbacks::clone_value),
            describe_value: Some(callbacks::describe), stringable: Some(callbacks::stringable),
            format_float: Some(callbacks::format_float), diagnostic: Some(callbacks::diagnostic),
            release_owner: Some(callbacks::release), array_next: Some(callbacks::next) }
    }

    /// Adds copied array-entry iteration without changing the original V1 callback prefix.
    pub fn table_v2(&mut self) -> MbInvokeHostV2 {
        let mut base = self.table();
        base.version = 2;
        base.size = std::mem::size_of::<MbInvokeHostV2>() as u32;
        MbInvokeHostV2 { base, array_value: Some(callbacks::array_value) }
    }

    /// Adds protected recursive graph reads and separately tracked original identity leases.
    pub fn table_v3(&mut self) -> MbInvokeHostV3 {
        let mut base = self.table_v2();
        base.base.version = 3;
        base.base.size = std::mem::size_of::<MbInvokeHostV3>() as u32;
        MbInvokeHostV3 { base, graph_value: Some(graph_host::graph_value), pin_value: Some(graph_host::pin) }
    }

    /// Encodes caller storage after execution without reading any released argument copies.
    pub fn observed(&self) -> Value {
        match &self.observer {
            Some((name, value)) => json!({*name: observe(&value.borrow())}),
            None => json!([]),
        }
    }
}

/// Copies scalars out of references while retaining array COW payloads and nested references.
fn by_value(value: &Php) -> Php {
    match value { Php::Reference(value) => by_value(&value.borrow()), value => value.clone() }
}

/// Represents PHP observer strings as hex and the fixture's associative array under its name key.
fn observe(value: &Php) -> Value {
    match value {
        Php::Reference(value) => observe(&value.borrow()),
        Php::String(bytes) => json!({"bytes": hex(bytes)}),
        Php::Int(value) => json!(value),
        Php::Array(values) => json!({"name": observe(&values[0])}),
        _ => Value::Null,
    }
}

/// Recognizes a canonical encoding getter result while allowing callbacks to change live settings.
fn valid_encoding_result(output: &Value) -> bool {
    if output[0] != "string" { return false; }
    let Some(hex) = output[1].as_str().filter(|hex| hex.len() % 2 == 0) else { return false; };
    let bytes = (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok()).collect::<Option<Vec<_>>>();
    bytes.is_some_and(|bytes| elephc_mbstring::encoding::Encoding::lookup(&bytes).is_some())
}

/// Encodes arbitrary binary bytes in the independent PHP fixture format.
pub fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

/// Copies and releases one real bridge result, preserving scalar and PHP error distinctions.
pub fn result(mut output: MbResultV1) -> Value {
    let bytes = if output.len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) } };
    let value = match output.kind {
        RESULT_INT => json!(["int", output.value]), RESULT_BOOL => json!(["bool", output.value != 0]),
        RESULT_STRING => json!(["string", hex(bytes)]),
        RESULT_STRING_ARRAY | RESULT_ENCODING_CATALOG => json!(["array", decode_string_array(bytes, output.value as usize)
            .expect("well-formed string list").into_iter().map(hex).collect::<Vec<_>>()]),
        RESULT_TYPE_ERROR => json!(["error", "TypeError", hex(bytes)]),
        RESULT_VALUE_ERROR => json!(["error", "ValueError", hex(bytes)]),
        RESULT_ERROR => json!(["error", "Error", hex(bytes)]),
        PREPARED_ARGUMENT_COUNT_ERROR => json!(["error", "ArgumentCountError", hex(bytes)]),
        _ => json!(["fatal", output.kind]),
    };
    unsafe { elephc_mbstring_release_v1(&mut output); }
    value
}

/// Executes a real request operation from a callback without bypassing the shared state owner.
pub fn call(operation: RuntimeBuiltinId, args: &[MbArgV1]) -> Value {
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(operation.as_u32(), args.as_ptr(), args.len() as u64, &mut output); }
    result(output)
}

/// Builds one binary string fixture value.
pub fn string(bytes: &[u8]) -> Php { Php::String(bytes.to_vec()) }

/// Builds one fixture Stringable whose conversion has a visible label and optional side effect.
pub fn object(label: &'static str, bytes: &[u8], action: Action) -> Php {
    Php::Object { label, bytes: bytes.to_vec(), action }
}

/// Recreates the independently captured PHP arguments and observers for outer parsing scenarios.
pub fn arguments(scenario: &str, host: &mut Host) -> (RuntimeBuiltinId, Vec<Php>) {
    use Php::*;
    let encoding = || object("encoding", b"UTF-8", Action::None);
    match scenario {
        "array_reference_mutation" | "scalar_reference_mutation" => {
            let value = Rc::new(RefCell::new(string(b"ok")));
            host.observer = Some(("bytes", value.clone()));
            let argument = if scenario.starts_with("array") { Array(Rc::new(vec![Reference(value.clone())])) }
                else { Reference(value.clone()) };
            (RuntimeBuiltinId::MbCheckEncoding, vec![argument,
                object("encoding", b"UTF-8", Action::Assign(value, Box::new(string(b"\xff"))))])
        },
        "array_cow_mutation" => {
            let value = Rc::new(RefCell::new(Array(Rc::new(vec![string(b"ok")]))));
            host.observer = Some(("array", value.clone()));
            (RuntimeBuiltinId::MbCheckEncoding, vec![Reference(value.clone()), object("encoding", b"UTF-8", Action::Cow(value))])
        },
        "earlier_stringable_mutates_later_scalar" => {
            let value = Rc::new(RefCell::new(Int(0)));
            host.observer = Some(("offset", value.clone()));
            (RuntimeBuiltinId::MbSubstr, vec![object("source", b"abcdef", Action::Assign(value.clone(), Box::new(Int(2)))), Reference(value), Int(1), string(b"8bit")])
        },
        "stringable_changes_internal" => (RuntimeBuiltinId::MbStrlen, vec![object("source", b"\xff\xff", Action::Internal)]),
        "stringable_throws" => (RuntimeBuiltinId::MbStrlen, vec![object("source", b"abc", Action::Throw), encoding()]),
        "null_before_stringable" => (RuntimeBuiltinId::MbStrlen, vec![Null, encoding()]),
        "lossy_before_stringable" => (RuntimeBuiltinId::MbSubstr, vec![string(b"abcdef"), Float(0.5), Int(1), encoding()]),
        "nan_before_stringable" => (RuntimeBuiltinId::MbStrlen, vec![Float(f64::NAN), encoding()]),
        "all_coercions_before_bad_encoding" => (RuntimeBuiltinId::MbSubstr,
            vec![Null, Float(0.5), Float(1.5), object("encoding", b"not-an-encoding", Action::None)]),
        "float_format_before_later_warning" => (RuntimeBuiltinId::MbSubstr, vec![Float(1.23456789), Float(0.5), Int(20), string(b"8bit")]),
        _ => panic!("unexpected outer argument scenario: {scenario}"),
    }
}

/// Invokes the real coordinator through the complete C callback table and consumes its result.
pub fn run(operation: RuntimeBuiltinId, values: &[Php], strict: bool, host: &mut Host) -> (i32, Value) {
    let pointers: Vec<_> = values.iter().map(|value| (value as *const Php).cast::<c_void>()).collect();
    let original = host.table();
    let extended = matches!(operation, RuntimeBuiltinId::MbDetectOrder | RuntimeBuiltinId::MbConvertEncoding | RuntimeBuiltinId::MbDetectEncoding | RuntimeBuiltinId::MbListEncodings | RuntimeBuiltinId::MbEncodeNumericentity | RuntimeBuiltinId::MbDecodeNumericentity).then(|| host.table_v2());
    let table = extended.as_ref().map_or(&original, |extended| &extended.base);
    let mut output = MbResultV1::default();
    let status = unsafe { elephc_mbstring_invoke_v1(operation.as_u32(), pointers.as_ptr(), pointers.len() as u64,
        strict as u32, table, &mut output) };
    let value = if status == 2 {
        assert_eq!(output.kind, MbResultV1::default().kind);
        assert!(output.bytes.is_null() && output.diagnostics.is_null());
        host.pending.clone().unwrap_or(json!(["pending"]))
    } else { result(output) };
    (status, value)
}
