//! Purpose:
//! Verifies opaque array-reference lookup and metadata preservation across reference writes.
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

/// Copies array metadata to a new box and clears stale metadata on reused identities.
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
    context.set_array_cursor(source, EvalArrayCursor::Position(1));
    context.set_array_cursor(target, EvalArrayCursor::Invalid);
    context.copy_array_metadata(source, target);
    assert!(context.array_element_alias(target, &stale).is_none());
    assert_eq!(context.array_cursor(target), EvalArrayCursor::Position(1));
    scope.set("value", later, ScopeCellOwnership::Borrowed);
    let mut owners = Vec::new();
    for token in [source, target] {
        let read = read_owned_array_reference(token, key.clone(), &mut context, &mut values, &mut owners).unwrap().unwrap();
        assert_eq!(values.get(read), FakeValue::String("after".into()));
        values.release(read).unwrap();
    }
    context.copy_array_metadata(target, target);
    assert!(context.array_element_alias(target, &key).is_some());
    assert_eq!(context.array_cursor(target), EvalArrayCursor::Position(1));
    context.set_array_cursor(source, EvalArrayCursor::Position(0));
    context.set_array_cursor(target, EvalArrayCursor::Invalid);
    context.copy_array_metadata(source, target);
    assert_eq!(context.array_cursor(target), EvalArrayCursor::Position(0));
    context.set_array_cursor(target, EvalArrayCursor::Invalid);
    context.clear_array_metadata(target);
    assert!(context.array_element_alias(target, &key).is_none());
    assert_eq!(context.array_cursor(target), EvalArrayCursor::Position(0));
    assert!(owners.is_empty());
}

/// Whole-reference replacement updates alias and cursor metadata before releasing the old value.
#[test]
fn array_reference_replacement_refreshes_metadata() {
    for through_cell in [false, true] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let old = values.array_new(1).unwrap();
        let reference = values.reference_new(old).unwrap();
        let stale = EvalArrayReferenceKey::Int(0);
        let key = EvalArrayReferenceKey::String(b"new".to_vec());
        let before = values.string("before").unwrap();
        context.bind_array_element_alias(reference, stale.clone(), EvalReferenceTarget::Cell { cell: before });
        context.set_array_cursor(reference, EvalArrayCursor::Invalid);
        scope.set("array", reference, ScopeCellOwnership::Borrowed);
        let replacement = values.array_new(1).unwrap();
        let after = values.string("after").unwrap();
        context.bind_array_element_alias(replacement, key.clone(), EvalReferenceTarget::Cell { cell: after });
        context.set_array_cursor(replacement, EvalArrayCursor::Position(1));
        let target = EvalReferenceTarget::Cell { cell: reference };
        if through_cell {
            write_back_method_ref_target(&target, replacement, &mut context, &mut values).unwrap();
        } else {
            for old in set_scope_cell(&mut context, &mut scope, "array", replacement, ScopeCellOwnership::Borrowed, &mut values).unwrap() {
                values.release(old).unwrap();
            }
        }
        assert!(context.array_element_alias(reference, &stale).is_none(), "through_cell={through_cell}");
        assert_eq!(context.array_cursor(reference), EvalArrayCursor::Position(1));
        let mut owners = Vec::new();
        let current = read_owned_array_reference(reference, key.clone(), &mut context, &mut values, &mut owners).unwrap().unwrap();
        assert_eq!(values.get(current), FakeValue::String("after".into()));
        values.release(current).unwrap();
        let scalar = values.int(7).unwrap();
        if through_cell {
            write_back_method_ref_target(&target, scalar, &mut context, &mut values).unwrap();
        } else {
            for old in set_scope_cell(&mut context, &mut scope, "array", scalar, ScopeCellOwnership::Borrowed, &mut values).unwrap() {
                values.release(old).unwrap();
            }
        }
        assert!(context.array_element_alias(reference, &key).is_none());
        assert_eq!(context.array_cursor(reference), EvalArrayCursor::Position(0));
        assert!(owners.is_empty());
    }
}

/// Exercises element writeback through either a scope variable or a nested persistent cell.
fn assert_array_element_writeback_preserves_metadata(through_cell: bool) {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let array = values.array_new(2).unwrap();
    let index = values.int(0).unwrap();
    let alias_index = values.int(1).unwrap();
    let before = values.int(1).unwrap();
    values.array_set(array, index, before).unwrap();
    values.array_set(array, alias_index, before).unwrap();
    let reference = values.reference_new(array).unwrap();
    let alias = values.reference_new(before).unwrap();
    let key = EvalArrayReferenceKey::Int(1);
    context.bind_array_element_alias(reference, key.clone(), EvalReferenceTarget::Cell { cell: alias });
    scope.set("array", reference, ScopeCellOwnership::Borrowed);
    let target = if through_cell {
        EvalReferenceTarget::NestedArrayElement {
            array_target: Box::new(EvalReferenceTarget::Cell { cell: reference }), index,
        }
    } else {
        EvalReferenceTarget::ArrayElement { scope: &mut scope, array_name: "array".into(), index }
    };
    for cursor in [EvalArrayCursor::Position(1), EvalArrayCursor::Invalid] {
        context.set_array_cursor(reference, cursor);
        let replacement = values.int(9).unwrap();
        write_back_method_ref_target(&target, replacement, &mut context, &mut values).unwrap();
        let written = values.array_get(reference, index).unwrap();
        assert_eq!(values.get(written), FakeValue::Int(9));
        values.release(written).unwrap();
        assert_eq!(context.array_cursor(reference), cursor);
        let after = values.int(7).unwrap();
        let previous = values.reference_replace(alias, after).unwrap();
        values.release(previous).unwrap();
        let mut owners = Vec::new();
        let read = read_owned_array_reference(reference, key.clone(), &mut context, &mut values, &mut owners).unwrap().unwrap();
        assert_eq!(values.get(read), FakeValue::Int(7));
        values.release(read).unwrap();
        assert!(owners.is_empty());
    }
}

/// Scope-array element writeback preserves sibling references and both valid and invalid cursors.
#[test]
fn array_element_writeback_preserves_scope_metadata() {
    assert_array_element_writeback_preserves_metadata(false);
}

/// Nested element writeback into a persistent Cell preserves sibling references and cursors.
#[test]
fn array_element_writeback_preserves_cell_metadata() {
    assert_array_element_writeback_preserves_metadata(true);
}
