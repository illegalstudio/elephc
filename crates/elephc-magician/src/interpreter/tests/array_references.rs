//! Purpose:
//! Verifies native array-reference lookup treats original handles as opaque identity tokens.
//!
//! Called from:
//! - The focused Magician interpreter unit-test harness.
//!
//! Key details:
//! - The original token has no fake runtime cell and must never be dereferenced or inspected.
//! - Repeated reads observe the current referenced scope value and preserve binary keys.

use super::super::*;
use super::support::*;
use crate::interpreter::array_references::read_owned_array_reference;

/// Reads live scope references through an opaque original-array token, including binary keys.
#[test]
fn mbstring_array_reference_identity_and_late_reads() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let token = RuntimeCellHandle::from_raw(0xfeed_usize as *mut _);
    let key = EvalArrayReferenceKey::String(b"encoding\0name".to_vec());
    let first = values.string("ASCII").unwrap();
    let later = values.string("UTF-8").unwrap();
    scope.set("encoding", first, ScopeCellOwnership::Borrowed);
    context.bind_array_element_alias(token, key.clone(), EvalReferenceTarget::Variable {
        scope: &mut scope, name: "encoding".to_string(),
    });
    let mut owners = Vec::new();
    let read = read_owned_array_reference(token, key.clone(), &mut context, &mut values, &mut owners).unwrap().unwrap();
    assert_eq!(values.get(read), FakeValue::String("ASCII".into()));
    values.release(read).unwrap();
    scope.set("encoding", later, ScopeCellOwnership::Borrowed);
    let read = read_owned_array_reference(token, key, &mut context, &mut values, &mut owners).unwrap().unwrap();
    assert_eq!(values.get(read), FakeValue::String("UTF-8".into()));
    values.release(read).unwrap();
    assert!(read_owned_array_reference(token, EvalArrayReferenceKey::Int(0), &mut context, &mut values, &mut owners).unwrap().is_none());
    assert!(owners.is_empty());
}

/// Copies reference bindings to a new box, replaces stale destination metadata, and preserves source reads.
#[test]
fn mbstring_array_reference_value_copy_metadata() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let source = RuntimeCellHandle::from_raw(0xfeed_usize as *mut _);
    let target = RuntimeCellHandle::from_raw(0xbeef_usize as *mut _);
    let key = EvalArrayReferenceKey::String(b"value\0key".to_vec());
    let stale = EvalArrayReferenceKey::Int(7);
    let first = values.string("before").unwrap();
    let later = values.string("after").unwrap();
    scope.set("value", first, ScopeCellOwnership::Borrowed);
    context.bind_array_element_alias(source, key.clone(), EvalReferenceTarget::Variable {
        scope: &mut scope, name: "value".to_string(),
    });
    context.bind_array_element_alias(target, stale.clone(), EvalReferenceTarget::Cell { cell: first });
    context.copy_array_element_aliases(source, target);
    assert!(context.array_element_alias(target, &stale).is_none());
    scope.set("value", later, ScopeCellOwnership::Borrowed);
    let mut owners = Vec::new();
    for token in [source, target] {
        let read = read_owned_array_reference(token, key.clone(), &mut context, &mut values, &mut owners).unwrap().unwrap();
        assert_eq!(values.get(read), FakeValue::String("after".into()));
        values.release(read).unwrap();
    }
    context.copy_array_element_aliases(target, target);
    assert!(context.array_element_alias(target, &key).is_some());
    assert!(owners.is_empty());
}
