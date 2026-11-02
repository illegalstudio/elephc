//! Purpose:
//! Models physical reference owners held by live fake containers and activations.
//!
//! Called from:
//! - FakeOps reference sharing and wrapper construction operations.
//!
//! Key details:
//! - Stored edges are derived from the current graph instead of stale allocation history.
//! - Each live container contributes each stored edge once, including duplicate reference slots.

use super::*;
use std::collections::HashSet;

impl FakeOps {
    /// Counts explicit owners plus stored edges reachable from live fake activation owners.
    pub(in crate::interpreter::tests) fn reference_owner_count(&self, reference: RuntimeCellHandle) -> usize {
        let sought = reference.as_ptr() as usize;
        let mut count = self.cell_owners.get(&sought).copied().unwrap_or(0);
        let mut pending = self.cell_owners.iter().filter_map(|(id, count)| (*count > 0).then_some(*id)).collect::<Vec<_>>();
        let mut visited = HashSet::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) { continue; }
            let children = if let Some(value) = self.references.get(&id) {
                vec![*value]
            } else {
                match self.values.get(&id) {
                    Some(FakeValue::Array(elements)) => elements.clone(),
                    Some(FakeValue::Assoc(entries)) => entries.iter().map(|(_, value)| *value).collect(),
                    Some(FakeValue::Object(properties)) => properties.iter().map(|(_, value)| *value).collect(),
                    _ => Vec::new(),
                }
            };
            for child in children {
                let child = child.as_ptr() as usize;
                if child == sought { count += 1; }
                if child != 0 { pending.push(child); }
            }
        }
        count
    }

    /// Transfers a copied value's activation owner into a wrapper-owned graph edge.
    pub(super) fn transfer_reference_value_owner(&mut self, value: RuntimeCellHandle) {
        let owners = self.cell_owners.get_mut(&(value.as_ptr() as usize)).expect("owned fake reference value");
        assert!(*owners > 0, "a reference wrapper consumes one copied value owner");
        *owners -= 1;
    }
}

/// Container stores, replacements and retirement affect sharing without changing explicit owners.
#[test]
fn fake_reference_sharing_tracks_live_container_edges() {
    let mut values = FakeOps::default();
    let original = values.int(1).unwrap();
    let reference = values.reference_new(original).unwrap();
    assert!(!values.reference_is_shared(reference).unwrap());
    let object = values.alloc(FakeValue::Object(vec![("value".into(), reference)]));
    assert!(values.reference_is_shared(reference).unwrap());
    values.release(reference).unwrap();
    assert!(!values.reference_is_shared(reference).unwrap());
    let array = values.array_new(1).unwrap();
    let key = values.int(0).unwrap();
    let array = values.array_set(array, key, reference).unwrap();
    assert!(values.reference_is_shared(reference).unwrap());
    values.release(array).unwrap();
    assert!(!values.reference_is_shared(reference).unwrap());
    values.property_set(object, "value", original).unwrap();
    assert_eq!(values.reference_owner_count(reference), 0);
}

/// A transferred reference payload does not keep a retired container alive as a false owner.
#[test]
fn fake_reference_payload_owner_transfers_without_a_phantom_root() {
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    let reference = values.reference_new(object).unwrap();
    assert_eq!(values.cell_owners[&(object.as_ptr() as usize)], 1);
    let replacement = values.int(2).unwrap();
    let previous = values.reference_replace(reference, replacement).unwrap();
    assert_eq!(previous, object);
    assert_eq!(values.cell_owners[&(object.as_ptr() as usize)], 2);
    values.release(previous).unwrap();
    assert_eq!(values.cell_owners[&(object.as_ptr() as usize)], 1);
}
