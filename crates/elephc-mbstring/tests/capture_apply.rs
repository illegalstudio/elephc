//! Purpose:
//! Verifies ordered capture writes through the real shared C ABI.
//!
//! Called from:
//! - Cargo's focused capture_apply integration test binary.
//!
//! Key details:
//! - The host resolves a mutable reference for each entry and preserves old array aliases.
//! - Pending callback exceptions continue filling; invalid graphs cannot partially mutate output.

use std::{cell::RefCell, ffi::c_void, rc::Rc};
use elephc_builtin_contract::mbstring_abi::{array::{ArrayGraph, Key, Value}, host::*};
use elephc_mbstring::abi::elephc_mbstring_capture_apply_v1;

type Entries = Vec<(Key, Value)>;

/// Owns independent observable arrays while the writer follows a mutable PHP-style reference.
struct Host {
    current: Rc<RefCell<Entries>>,
    old: Rc<RefCell<Entries>>,
    calls: usize,
    statuses: Vec<i32>,
    reassign: bool,
    nested: bool,
}

impl Host {
    /// Starts one reference with an existing key that capture filling must preserve.
    fn new(statuses: &[i32]) -> Self {
        let current = Rc::new(RefCell::new(vec![(Key::String(b"kept".to_vec()), Value::String(b"seed".to_vec()))]));
        Self { old: current.clone(), current, calls: 0, statuses: statuses.to_vec(), reassign: false, nested: false }
    }

    /// Calls the exported bridge with a stable opaque writer token and a borrowed graph.
    fn apply(&mut self, graph: &[u8]) -> i32 {
        let mut token = 0_u64;
        unsafe { elephc_mbstring_capture_apply_v1((self as *mut Self).cast(), (&mut token as *mut u64).cast(),
            graph.as_ptr(), graph.len() as u64, Some(store)) }
    }
}

/// Copies a borrowed descriptor into independently owned observation data.
unsafe fn value(input: &MbHostValueV1) -> Value {
    match input.tag {
        HOST_INT => Value::Int(input.lo as i64),
        HOST_BOOL => Value::Bool(input.lo != 0),
        HOST_STRING => Value::String(unsafe { std::slice::from_raw_parts(input.lo as *const u8, input.hi as usize) }.to_vec()),
        tag => panic!("unexpected capture descriptor {tag}"),
    }
}

/// Resolves one destination, models an old-value callback, then completes that exact write.
unsafe extern "C" fn store(context: *mut c_void, writer: *mut c_void,
    key: *const MbHostValueV1, input: *const MbHostValueV1) -> i32 {
    assert!(!writer.is_null());
    let host = unsafe { &mut *context.cast::<Host>() };
    let key = match unsafe { value(&*key) } {
        Value::Int(value) => Key::Int(value), Value::String(value) => Key::String(value),
        _ => panic!("unexpected capture key"),
    };
    let value = unsafe { value(&*input) };
    let destination = host.current.clone();
    if host.calls == 0 && host.reassign {
        host.current = Rc::new(RefCell::new(vec![(Key::String(b"replacement".to_vec()), Value::String(b"new".to_vec()))]));
    }
    if host.calls == 0 && host.nested {
        let mut nested = Host::new(&[]);
        assert_eq!(nested.apply(&captures()), 0);
        assert_eq!(nested.calls, 3);
    }
    let mut entries = destination.borrow_mut();
    if let Some((_, previous)) = entries.iter_mut().find(|(old, _)| *old == key) { *previous = value; }
    else { entries.push((key, value)); }
    let status = host.statuses.get(host.calls).copied().unwrap_or(0);
    host.calls += 1;
    status
}

/// Produces numeric and binary named captures, including an unmatched group.
fn captures() -> Vec<u8> {
    ArrayGraph::new(0, vec![vec![
        (Key::Int(0), Value::String(b"a\0b".to_vec())),
        (Key::Int(1), Value::Bool(false)),
        (Key::String(b"group\0name".to_vec()), Value::String(b"b".to_vec())),
    ]]).unwrap().encode()
}

/// Preserves insertion order and aliases while completing writes after pending destructors.
#[test]
fn capture_apply_completes_pending_writes_without_splitting_aliases() {
    let mut host = Host::new(&[2, 2, 0]);
    assert_eq!(host.apply(&captures()), 2);
    assert_eq!(host.calls, 3);
    assert!(Rc::ptr_eq(&host.old, &host.current));
    assert_eq!(host.old.borrow().as_slice(), &[
        (Key::String(b"kept".to_vec()), Value::String(b"seed".to_vec())),
        (Key::Int(0), Value::String(b"a\0b".to_vec())),
        (Key::Int(1), Value::Bool(false)),
        (Key::String(b"group\0name".to_vec()), Value::String(b"b".to_vec())),
    ]);
}

/// Uses the receiver selected for one write, then follows reference reassignment for later writes.
#[test]
fn capture_apply_follows_reassigned_output_between_entries() {
    let mut host = Host::new(&[2]);
    host.reassign = true;
    host.nested = true;
    assert_eq!(host.apply(&captures()), 2);
    assert_eq!(host.calls, 3);
    assert_eq!(host.old.borrow().len(), 2);
    assert_eq!(host.old.borrow()[1], (Key::Int(0), Value::String(b"a\0b".to_vec())));
    assert_eq!(host.current.borrow().as_slice(), &[
        (Key::String(b"replacement".to_vec()), Value::String(b"new".to_vec())),
        (Key::Int(1), Value::Bool(false)),
        (Key::String(b"group\0name".to_vec()), Value::String(b"b".to_vec())),
    ]);
}

/// Stops unknown host failures without losing a throwable reported by an earlier completed write.
#[test]
fn capture_apply_preserves_pending_precedence_on_host_failure() {
    for invalid in [1, 3, 254, -1] {
        let mut host = Host::new(&[2, invalid]);
        assert_eq!(host.apply(&captures()), 2);
        assert_eq!(host.calls, 2);
        let mut host = Host::new(&[invalid, 2]);
        assert_eq!(host.apply(&captures()), 1);
        assert_eq!(host.calls, 1);
    }
}

/// Rejects noncapture values anywhere in a graph before invoking even the first valid entry.
#[test]
fn capture_apply_validates_the_entire_graph_before_mutation() {
    for invalid in [Value::Bool(true), Value::Null, Value::Int(7), Value::Float(0), Value::Unsupported] {
        let graph = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::String(b"valid".to_vec())), (Key::Int(1), invalid)]]).unwrap();
        let mut host = Host::new(&[]);
        assert_eq!(host.apply(&graph.encode()), 1);
        assert_eq!(host.calls, 0);
    }
    let graph = ArrayGraph::new(0, vec![vec![(Key::Int(0), Value::Array(1))], vec![]]).unwrap();
    let mut host = Host::new(&[]);
    assert_eq!(host.apply(&graph.encode()), 1);
    assert_eq!(host.calls, 0);
    assert_eq!(host.apply(b"invalid graph"), 1);
    assert_eq!(host.calls, 0);
}

/// Accepts an empty capture set and rejects missing required callback or writer metadata.
#[test]
fn capture_apply_checks_empty_and_absent_inputs() {
    let graph = ArrayGraph::new(0, vec![vec![]]).unwrap().encode();
    let mut host = Host::new(&[]);
    assert_eq!(host.apply(&graph), 0);
    assert_eq!(host.calls, 0);
    unsafe {
        assert_eq!(elephc_mbstring_capture_apply_v1(std::ptr::null_mut(), std::ptr::null_mut(),
            graph.as_ptr(), graph.len() as u64, Some(store)), 1);
        let mut token = 0_u64;
        assert_eq!(elephc_mbstring_capture_apply_v1(std::ptr::null_mut(), (&mut token as *mut u64).cast(),
            graph.as_ptr(), graph.len() as u64, None), 1);
        assert_eq!(elephc_mbstring_capture_apply_v1(std::ptr::null_mut(), (&mut token as *mut u64).cast(),
            std::ptr::null(), 0, Some(store)), 1);
    }
}
