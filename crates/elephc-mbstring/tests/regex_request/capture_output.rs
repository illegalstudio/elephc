//! Purpose:
//! Replays PHP reference-initialization callbacks around the shared mb_ereg/mb_eregi engine.
//!
//! Called from:
//! - The regex_request integration test binary with the managed Oniguruma provider.
//!
//! Key details:
//! - The modeled host retains array construction identity across destructor-created aliases.
//! - These tests verify engine ordering and define obligations for future native/eval reference adapters.

use super::*;
use std::rc::Rc;

/// A host-owned output slot distinguishes scalar storage from an array still being constructed.
#[derive(Clone)]
enum Output { Scalar(Value), Array(Rc<RefCell<Vec<Value>>>) }

impl Output {
    /// Publishes a fresh mutable construction array without copying any previous PHP value.
    fn array() -> Self { Self::Array(Rc::new(RefCell::new(Vec::new()))) }

    /// Copies only observable contents; array identity remains independently retained by host aliases.
    fn snapshot(&self) -> Value {
        match self { Self::Scalar(value) => value.clone(), Self::Array(entries) => json!({"array": *entries.borrow()}) }
    }

    /// Appends or replaces one exact-key capture without separating aliases of the active construction array.
    fn insert(&self, entry: Value) {
        let Self::Array(entries) = self else { panic!("capture output must remain an array"); };
        let mut entries = entries.borrow_mut();
        if let Some(previous) = entries.iter_mut().find(|previous| previous[0] == entry[0]) { *previous = entry; }
        else { entries.push(entry); }
    }
}

/// Models the host's typed-reference validation and protected destructor callback at initialization.
fn initialize(session: &Session, case: &Value, output: &RefCell<Output>, copy: &RefCell<Output>,
    error: &RefCell<Value>, initialization: &mut Vec<Value>, actions: &mut Vec<Value>,
    stack: &Cell<i64>, retry: &Cell<i64>) -> bool {
    let storage = case["storage"].as_str().unwrap();
    if matches!(storage, "int" | "object") {
        let (class, ty) = if storage == "int" { ("CaptureIntProperty", "int") }
            else { ("CaptureObjectProperty", "CaptureOutputOwner") };
        let message = format!("Cannot assign array to reference held by property {class}::$value of type {ty}");
        *error.borrow_mut() = json!(["TypeError", hex(message.as_bytes()), null]);
        return false;
    }
    // Typed references publish the new array before releasing the old object; untyped references expose null.
    *output.borrow_mut() = if storage == "local" { Output::Scalar(Value::Null) } else { Output::array() };
    initialization.push(output.borrow().snapshot());
    for step in case["actions"].as_array().unwrap() { actions.push(run(session, step, stack, retry)); }
    if case["mutate"] == true {
        if matches!(*output.borrow(), Output::Scalar(_)) { *output.borrow_mut() = Output::array(); }
        output.borrow().insert(json!([string(b"kept"), string(b"initialization")]));
        *copy.borrow_mut() = output.borrow().clone();
    }
    if case["throw"] == true { *error.borrow_mut() = json!(["RuntimeException", hex(b"output cleanup"), null]); }
    if storage == "local" { *output.borrow_mut() = Output::array(); }
    true
}

/// Runs real shared matching after host actions, retaining pending exceptions alongside later output changes.
fn replay(case: &Value) -> Value {
    let session = Session::default();
    let (stack, retry) = (Cell::new(100_000), Cell::new(1_000_000));
    for step in case["before"].as_array().unwrap() { run(&session, step, &stack, &retry); }
    let output = RefCell::new(Output::Scalar(if case["storage"] == "int" { json!(17) } else { json!({"object": "CaptureOutputOwner"}) }));
    let copy = RefCell::new(Output::Scalar(Value::Null));
    let error = RefCell::new(Value::Null);
    let (mut initialization, mut actions, mut warnings, mut matches_at_warning) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let matched = session.capture(&bytes(&case["pattern"]).unwrap(), &bytes(&case["subject"]).unwrap(), case["op"] == "mb_eregi",
        || initialize(&session, case, &output, &copy, &error, &mut initialization, &mut actions, &stack, &retry),
        || Limits::from_ini(stack.get(), retry.get(), false), &mut |event| match event {
            Event::Exception(raised) => { let previous = error.replace(Value::Null); *error.borrow_mut() = exception(raised, previous); },
            Event::Warning(bytes) if error.borrow().is_null() => {
                warnings.push(hex(&bytes));
                matches_at_warning.push(output.borrow().snapshot());
            },
            Event::Warning(_) => {},
        }).unwrap();
    let found = matched.is_some();
    if let Some(matched) = matched {
        let fields = registers(Some(matched.registers(false)));
        for entry in fields["array"].as_array().unwrap() { output.borrow().insert(entry.clone()); }
    }
    let mut result = json!({"matches": output.borrow().snapshot(), "copy": copy.borrow().snapshot(),
        "initialization": initialization, "actions": actions, "warnings": warnings, "matches_at_warning": matches_at_warning,
        "encoding": session.encoding().name(), "options": session.options().as_string(), "position": session.position(),
        "registers": registers(session.registers())});
    if error.borrow().is_null() { result["value"] = json!(found); }
    else { result["error"] = error.into_inner(); }
    result
}

/// Compares 1,200 PHP traces for typed constraints, initialization visibility, aliases, reentry, and throws.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_capture_output_callbacks_match_php() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let reader = flate2::read::GzDecoder::new(include_bytes!("../fixtures/regex_output.jsonl.gz").as_slice());
    let mut count = 0;
    for line in BufReader::new(reader).lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        assert_eq!(replay(&case), case["trace"], "case {count}: {case}");
        count += 1;
    }
    assert_eq!(count, 1200);
}
