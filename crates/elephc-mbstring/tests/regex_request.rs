//! Purpose:
//! Replays independent PHP request traces against shared regex caching and progressive searches.
//!
//! Called from:
//! - Explicit native-provider tests and the managed-native CI matrix.
//!
//! Key details:
//! - Every fixture starts with a new session; captured errors retain later state mutations.
//! - Exact register keys, empty captures, warnings, and exception chains are compared.

#[path = "support/regex_provider.rs"]
mod regex_provider;
#[path = "regex_request/capture_output.rs"]
mod capture_output;
#[path = "regex_request/replace_callback.rs"]
mod replace_callback;

use std::{cell::{Cell, RefCell}, io::{BufRead, BufReader}};
use elephc_mbstring::{error::MbError, regex::{install_provider, Event, Limits, RegisterKey, Registers, Session}};
use serde_json::{json, Value};
use elephc_mbstring::regex::{Replacement, ReplaceResult};

/// Decodes an optional exact byte input while preserving omission and explicit null.
fn bytes(value: &Value) -> Option<Vec<u8>> {
    value.as_str().map(|value| value.as_bytes().chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect())
}

/// Encodes raw bytes in the oracle's tagged scalar format.
fn string(bytes: &[u8]) -> Value { json!({"bytes": hex(bytes)}) }

/// Encodes exact messages without a UTF-8 replacement policy.
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

/// Preserves numeric/name keys, register order, false groups, and empty matched bytes.
fn registers(registers: Option<Registers>) -> Value {
    match registers {
        None => json!(false),
        Some(registers) => json!({"array": registers.into_iter().map(|(key, value)| {
            let key = match key { RegisterKey::Index(index) => json!(index), RegisterKey::Name(name) => string(&name) };
            json!([key, value.map_or_else(|| json!(false), |bytes| string(&bytes))])
        }).collect::<Vec<_>>()}),
    }
}

/// Translates shared exception events to the exact PHP class, bytes, and previous-exception chain.
fn exception(error: MbError, previous: Value) -> Value {
    let (class, message) = match error {
        MbError::Value(message) => ("ValueError", message.into_bytes()),
        MbError::ValueBytes(message) => ("ValueError", message),
        MbError::Runtime(message) => ("Error", message.into_bytes()),
    };
    json!([class, hex(&message), previous])
}

/// Executes one already-coerced operation and snapshots settings after all observable side effects.
fn run(session: &Session, step: &Value, stack: &Cell<i64>, retry: &Cell<i64>) -> Value {
    let pattern = bytes(&step["pattern"]);
    let options = bytes(&step["options"]);
    let subject = bytes(&step["subject"]);
    let input = bytes(&step["value"]);
    let op = step["op"].as_str().unwrap();
    let capture = matches!(op, "ereg" | "eregi");
    let captured = RefCell::new(step.get("matches").cloned().unwrap_or_else(|| string(b"old")));
    let mut matches_at_warning = Vec::new();
    let mut warnings = Vec::new();
    let mut error = Value::Null;
    let mut callbacks = Vec::new();
    let mut emit = |event| match event {
        Event::Exception(raised) => error = exception(raised, error.take()),
        Event::Warning(bytes) if error.is_null() => {
            warnings.push(hex(&bytes));
            if capture {
                matches_at_warning.push(captured.borrow().clone());
                if let Some(value) = step.get("warning_matches") { *captured.borrow_mut() = value.clone(); }
            }
            if callbacks.is_empty() {
                if let Some(actions) = step["on_warning"].as_array() {
                    callbacks.push(actions.iter().map(|step| run(session, step, stack, retry)).collect::<Vec<_>>());
                    if step["throw"] == true { error = json!(["RuntimeException", hex(b"regex handler failed"), null]); }
                }
            }
        }
        Event::Warning(_) => {},
    };
    let value = match op {
        "encoding" => session.set_encoding(input.as_ref().unwrap()).map(|_| json!(true)),
        "options" => session.set_options(input.as_ref().unwrap()).map(|options| string(options.as_string().as_bytes())),
        "setpos" => session.set_position(step["value"].as_i64().unwrap()).map(|_| json!(true)),
        "init" => Ok(json!(session.initialize(subject.as_ref().unwrap(), pattern.as_deref(), options.as_deref(), &mut emit).unwrap())),
        "match" => Ok(json!(session.is_match(pattern.as_ref().unwrap(), subject.as_ref().unwrap(), options.as_deref(),
            Limits::from_ini(stack.get(), retry.get(), true), &mut emit).unwrap())),
        "split" => Ok(match session.split(pattern.as_ref().unwrap(), subject.as_ref().unwrap(), step["limit"].as_i64().unwrap(),
            Limits::from_ini(stack.get(), retry.get(), false), &mut emit).unwrap() {
                None => json!(false), Some(fields) => json!({"array": fields.iter().enumerate()
                    .map(|(index, bytes)| json!([index, string(bytes)])).collect::<Vec<_>>()}),
            }),
        "replace" | "ireplace" => Ok(match session.replace(Replacement {
            pattern: pattern.as_ref().unwrap(), subject: subject.as_ref().unwrap(),
            replacement: &bytes(&step["replacement"]).unwrap(), options: options.as_deref(),
            encoding: session.encoding(), ignore_case: op == "ireplace",
        }, Limits::from_ini(stack.get(), retry.get(), false), &mut emit).unwrap() {
            ReplaceResult::InvalidSubject => Value::Null,
            ReplaceResult::Failed => json!(false), ReplaceResult::String(bytes) => string(&bytes),
        }),
        "ereg" | "eregi" => {
            let supplied = step["with_matches"] != false;
            let matched = session.capture(pattern.as_ref().unwrap(), subject.as_ref().unwrap(), op == "eregi",
                || { if supplied { *captured.borrow_mut() = json!({"array": []}); } true },
                || Limits::from_ini(stack.get(), retry.get(), false), &mut emit).unwrap();
            let found = matched.is_some();
            if supplied { if let Some(matched) = matched { *captured.borrow_mut() = registers(Some(matched.registers(false))); } }
            Ok(json!(found))
        },
        "search" | "pos" | "regs" => {
            let function = match op { "pos" => "mb_ereg_search_pos", "regs" => "mb_ereg_search_regs", _ => "mb_ereg_search" };
            let matched = session.search(pattern.as_deref(), options.as_deref(), function,
                Limits::from_ini(stack.get(), retry.get(), false), &mut emit).unwrap_or_else(|error| panic!("{step}: {error:?}"));
            Ok(match (op, matched) {
                (_, None) => json!(false),
                ("search", Some(_)) => json!(true),
                ("regs", Some(matched)) => registers(Some(matched.registers(true))),
                (_, Some(matched)) => { let (offset, length) = matched.position(); json!({"array": [[0, offset], [1, length]]}) }
            })
        }
        "getregs" => Ok(registers(session.registers())),
        "stack" | "retry" => {
            let value = step["value"].as_i64().unwrap();
            let old = if op == "stack" { stack } else { retry }.replace(value);
            Ok(string(old.to_string().as_bytes()))
        }
        _ => panic!("unknown step {step}"),
    };
    let mut result = json!({"warnings": warnings, "position": session.position(),
        "encoding": session.encoding().name(), "options": session.options().as_string()});
    if capture { result["matches"] = captured.into_inner(); result["matches_at_warning"] = json!(matches_at_warning); }
    if step.get("on_warning").is_some() { result["callbacks"] = json!(callbacks); }
    match value { Err(raised) => error = exception(raised, error), Ok(value) => if error.is_null() { result["value"] = value; } }
    if !error.is_null() { result["error"] = error; }
    result
}

/// Compares cache invalidation, alias changes, partial errors, positions, limits, and retained registers.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_requests_match_php() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let reader = flate2::read::GzDecoder::new(include_bytes!("fixtures/regex_request.jsonl.gz").as_slice());
    let mut count = 0;
    for line in BufReader::new(reader).lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let session = Session::default();
        let (stack, retry) = (Cell::new(100_000), Cell::new(1_000_000));
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let actual = run(&session, step, &stack, &retry);
            assert_eq!(actual, case["trace"][index], "case {count}, step {index}: {case}");
        }
        count += 1;
    }
    assert_eq!(count, 578);
}

/// Compares complete split arrays, limits, empty-match advancement, diagnostics, and shared cache effects.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_split_matches_php() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let reader = flate2::read::GzDecoder::new(include_bytes!("fixtures/regex_split.jsonl.gz").as_slice());
    let mut count = 0;
    for line in BufReader::new(reader).lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let session = Session::default();
        let (stack, retry) = (Cell::new(100_000), Cell::new(1_000_000));
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            assert_eq!(run(&session, step, &stack, &retry), case["trace"][index], "case {count}, step {index}: {case}");
        }
        count += 1;
    }
    assert_eq!(count, 1871);
}

/// Compares replacement bytes, numeric/name references, encoding behavior, and shared cache side effects.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_replace_matches_php() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let reader = flate2::read::GzDecoder::new(include_bytes!("fixtures/regex_replace.jsonl.gz").as_slice());
    let mut count = 0;
    for line in BufReader::new(reader).lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let session = Session::default();
        let (stack, retry) = (Cell::new(100_000), Cell::new(1_000_000));
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            assert_eq!(run(&session, step, &stack, &retry), case["trace"][index], "case {count}, step {index}: {case}");
        }
        count += 1;
    }
    assert_eq!(count, 1431);
}

/// Checks output initialization and exact capture values before references are materialized by backend adapters.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_capture_matches_php() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let reader = flate2::read::GzDecoder::new(include_bytes!("fixtures/regex_capture.jsonl.gz").as_slice());
    let mut count = 0;
    for line in BufReader::new(reader).lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let session = Session::default();
        let (stack, retry) = (Cell::new(100_000), Cell::new(1_000_000));
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            assert_eq!(run(&session, step, &stack, &retry), case["trace"][index], "case {count}, step {index}: {case}");
        }
        count += 1;
    }
    assert_eq!(count, 718);
}

/// Retains active operation owners across cache replacement and reproduces a real two-request PHP worker reset.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_request_ownership_and_reset() {
    use std::rc::Rc;
    assert!(unsafe { install_provider(regex_provider::provider()) });
    assert_eq!(regex_provider::guarded_subjects(), 0);
    let session = Session::default();
    let mut events = Vec::new();
    let mut emit = |event| events.push(event);
    let options = session.options();
    let held = session.compile(b"a", options, "mb_ereg_match", &mut emit).unwrap().unwrap();
    let weak = Rc::downgrade(&held);
    assert!(session.initialize(b"aa", Some(b"a"), None, &mut emit).unwrap());
    let matched = session.search(None, None, "mb_ereg_search", Limits::from_ini(100_000, 1_000_000, false), &mut emit).unwrap().unwrap();
    let subject = Rc::downgrade(&matched.subject);
    let next = session.search(None, None, "mb_ereg_search", Limits::from_ini(100_000, 1_000_000, false), &mut emit).unwrap().unwrap();
    assert!(Rc::ptr_eq(&matched.subject, &next.subject));
    drop(next);
    session.set_options(b"ib").unwrap();
    session.set_encoding(b"SJIS").unwrap();
    let replaced = session.compile(b"a", session.options(), "mb_ereg_match", &mut emit).unwrap().unwrap();
    assert!(!Rc::ptr_eq(&held, &replaced));
    assert!(session.registers().is_none());
    assert!(held.search(b"a", 0, true, Limits::from_ini(100_000, 1_000_000, true)).unwrap().is_some());
    assert_eq!(matched.registers(true), vec![(RegisterKey::Index(0), Some(b"a".to_vec()))]);
    drop(replaced);
    session.reset_request();
    assert_eq!(session.options().as_string(), "ib");
    assert_eq!(session.encoding().name(), "UTF-8");
    assert_eq!(session.position(), 0);
    assert!(session.registers().is_none());
    assert!(session.search(None, None, "mb_ereg_search", Limits::from_ini(100_000, 1_000_000, false), &mut emit).unwrap().is_none());
    assert_eq!(events, [Event::Exception(MbError::Runtime("No pattern was provided".into()))]);
    assert!(weak.upgrade().is_some() && subject.upgrade().is_some());
    drop(held);
    drop(matched);
    assert!(weak.upgrade().is_none() && subject.upgrade().is_none());
    let fresh = session.compile(b"a", options, "mb_ereg_match", &mut |_| {}).unwrap().unwrap();
    let weak = Rc::downgrade(&fresh);
    drop(fresh);
    assert!(weak.upgrade().is_some());
    session.reset_request();
    assert!(weak.upgrade().is_none());
    assert_eq!(Session::default().options().as_string(), "pr");
}

/// Captures the same before/after state as the worker oracle without normalizing its encoding alias.
fn worker_request(session: &Session, first: bool) -> Value {
    let limits = Limits::from_ini(100_000, 1_000_000, false);
    let mut events = Vec::new();
    let mut emit = |event| events.push(event);
    let before = json!([session.options().as_string(), session.encoding().name(),
        session.is_match(b".", b"\xfa\x40", None, Limits::from_ini(100_000, 1_000_000, true), &mut emit).unwrap()]);
    if first {
        session.set_options(b"ib").unwrap();
        session.set_encoding(b"ASCII").unwrap();
        assert!(session.initialize(b"aa", Some(b"a"), None, &mut emit).unwrap());
        assert!(session.search(None, None, "mb_ereg_search", limits, &mut emit).unwrap().is_some());
    }
    let registers = match session.registers() {
        Some(registers) => json!(registers.into_iter().map(|(_, value)| String::from_utf8(value.unwrap()).unwrap()).collect::<Vec<_>>()),
        None => json!(false),
    };
    let mut state = vec![json!(session.options().as_string()), json!(session.encoding().name()), json!(session.position()), registers];
    let matched = session.search(None, None, "mb_ereg_search", limits, &mut emit).unwrap();
    state.push(match events.as_slice() {
        [] => json!(matched.is_some()),
        [Event::Exception(MbError::Runtime(message))] => json!(message),
        _ => panic!("unexpected worker diagnostics: {events:?}"),
    });
    json!({"before": before, "state": state})
}

/// Matches actual consecutive PHP HTTP requests, including configured defaults and canonical alias reset.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_configured_workers_match_php() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/regex_worker.json")).unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let session = Session::default();
        session.configure_encoding(case["input"].as_str().unwrap().as_bytes());
        assert_eq!(worker_request(&session, true), case["first"], "{case}");
        session.reset_request();
        assert_eq!(worker_request(&session, false), case["second"], "{case}");
    }
    let session = Session::default();
    session.set_encoding(b"SJIS-WIN").unwrap();
    session.configure_encoding(b"Windows-1252");
    assert_eq!(session.encoding().name(), "SJIS");
    assert!(session.encoding().is_valid(b"\xfa\x40"));
    session.reset_request();
    assert_eq!(session.encoding().name(), "UTF-8");
}
