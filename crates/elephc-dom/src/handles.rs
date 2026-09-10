//! Purpose:
//! Provides generation-checked opaque handles for bridge-owned native objects.
//! Rejects forged, stale, and cross-kind values before native pointers are accessed.
//!
//! Called from:
//! - `crate::context` for documents, nodes, collections, XPath values, and wrappers.
//!
//! Key details:
//! - Handles encode an 8-bit kind, 24-bit generation, and one-based 32-bit slot.
//! - Freed generations are advanced before a slot can be reused.

const KIND_SHIFT: u32 = 56;
const GENERATION_SHIFT: u32 = 32;
const GENERATION_MASK: u32 = 0x00ff_ffff;

/// Reason an opaque bridge handle cannot address the requested live entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HandleError {
    Malformed,
    WrongKind,
    Stale,
}

/// One reusable arena slot with a generation guarding its current value.
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Context-owned opaque-handle arena.
pub(crate) struct HandleTable<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    live: usize,
}

impl<T> HandleTable<T> {
    /// Creates an empty handle arena.
    pub(crate) fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    /// Inserts a value under one nonzero handle kind and returns its opaque ID.
    pub(crate) fn insert(&mut self, kind: u8, value: T) -> u64 {
        assert_ne!(kind, 0, "bridge handle kind zero is reserved");
        let slot_index = if let Some(slot_index) = self.free.pop() {
            let slot = &mut self.slots[slot_index as usize];
            debug_assert!(slot.value.is_none());
            slot.value = Some(value);
            slot_index
        } else {
            let slot_index =
                u32::try_from(self.slots.len()).expect("bridge handle slot range exhausted");
            self.slots.push(Slot {
                generation: 1,
                value: Some(value),
            });
            slot_index
        };
        self.live += 1;
        encode(kind, self.slots[slot_index as usize].generation, slot_index)
    }

    /// Borrows a live entry after validating its shape, kind, slot, and generation.
    pub(crate) fn get(&self, handle: u64, expected_kind: u8) -> Result<&T, HandleError> {
        let (kind, generation, slot_index) = decode(handle)?;
        if kind != expected_kind {
            return Err(HandleError::WrongKind);
        }
        let slot = self
            .slots
            .get(slot_index as usize)
            .ok_or(HandleError::Stale)?;
        if slot.generation != generation {
            return Err(HandleError::Stale);
        }
        slot.value.as_ref().ok_or(HandleError::Stale)
    }

    /// Mutably borrows a live entry after complete handle validation.
    pub(crate) fn get_mut(
        &mut self,
        handle: u64,
        expected_kind: u8,
    ) -> Result<&mut T, HandleError> {
        let (kind, generation, slot_index) = decode(handle)?;
        if kind != expected_kind {
            return Err(HandleError::WrongKind);
        }
        let slot = self
            .slots
            .get_mut(slot_index as usize)
            .ok_or(HandleError::Stale)?;
        if slot.generation != generation {
            return Err(HandleError::Stale);
        }
        slot.value.as_mut().ok_or(HandleError::Stale)
    }

    /// Removes a live entry, advances its generation, and makes its slot reusable.
    pub(crate) fn remove(
        &mut self,
        handle: u64,
        expected_kind: u8,
    ) -> Result<T, HandleError> {
        let (kind, generation, slot_index) = decode(handle)?;
        if kind != expected_kind {
            return Err(HandleError::WrongKind);
        }
        let slot = self
            .slots
            .get_mut(slot_index as usize)
            .ok_or(HandleError::Stale)?;
        if slot.generation != generation {
            return Err(HandleError::Stale);
        }
        let value = slot.value.take().ok_or(HandleError::Stale)?;
        slot.generation = next_generation(slot.generation);
        self.free.push(slot_index);
        self.live -= 1;
        Ok(value)
    }

    /// Invalidates and drops every live entry while retaining reusable arena storage.
    pub(crate) fn clear(&mut self) {
        self.free.clear();
        for (slot_index, slot) in self.slots.iter_mut().enumerate() {
            if slot.value.take().is_some() {
                slot.generation = next_generation(slot.generation);
            }
            self.free
                .push(u32::try_from(slot_index).expect("existing slot fits u32"));
        }
        self.live = 0;
    }

    /// Returns the number of currently live handles.
    pub(crate) fn len(&self) -> usize {
        self.live
    }

    /// Visits every currently live arena value without changing handle generations.
    pub(crate) fn for_each_mut(&mut self, mut visit: impl FnMut(&mut T)) {
        for slot in &mut self.slots {
            if let Some(value) = slot.value.as_mut() {
                visit(value);
            }
        }
    }
}

/// Returns the encoded nonzero kind without consulting a handle table.
pub(crate) fn handle_kind(handle: u64) -> Result<u8, HandleError> {
    decode(handle).map(|(kind, _, _)| kind)
}

/// Encodes one validated kind, generation, and zero-based slot.
fn encode(kind: u8, generation: u32, slot_index: u32) -> u64 {
    debug_assert_ne!(kind, 0);
    debug_assert!((1..=GENERATION_MASK).contains(&generation));
    (u64::from(kind) << KIND_SHIFT)
        | (u64::from(generation) << GENERATION_SHIFT)
        | u64::from(slot_index + 1)
}

/// Decodes one opaque handle without consulting an arena.
fn decode(handle: u64) -> Result<(u8, u32, u32), HandleError> {
    let kind = (handle >> KIND_SHIFT) as u8;
    let generation = ((handle >> GENERATION_SHIFT) as u32) & GENERATION_MASK;
    let one_based_slot = handle as u32;
    if kind == 0 || generation == 0 || one_based_slot == 0 {
        return Err(HandleError::Malformed);
    }
    Ok((kind, generation, one_based_slot - 1))
}

/// Advances a generation while reserving zero as permanently invalid.
fn next_generation(generation: u32) -> u32 {
    if generation == GENERATION_MASK {
        1
    } else {
        generation + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies reuse changes a slot's generation and rejects the prior handle.
    #[test]
    fn reused_slots_invalidate_stale_handles() {
        let mut handles = HandleTable::new();
        let first = handles.insert(1, "first");
        assert_eq!(handles.remove(first, 1), Ok("first"));
        let second = handles.insert(1, "second");
        assert_ne!(first, second);
        assert_eq!(handles.get(first, 1), Err(HandleError::Stale));
        assert_eq!(handles.get(second, 1), Ok(&"second"));
    }

    /// Verifies kind checks happen before returning an otherwise live slot.
    #[test]
    fn cross_kind_handles_are_rejected() {
        let mut handles = HandleTable::new();
        let handle = handles.insert(7, 10_u64);
        assert_eq!(handles.get(handle, 8), Err(HandleError::WrongKind));
        *handles.get_mut(handle, 7).expect("live typed handle") = 11;
        assert_eq!(handles.get(handle, 7), Ok(&11));
    }

    /// Verifies reset invalidates all handles and retains reusable storage.
    #[test]
    fn clear_invalidates_every_live_generation() {
        let mut handles = HandleTable::new();
        let first = handles.insert(1, 1);
        let second = handles.insert(2, 2);
        handles.clear();
        assert_eq!(handles.len(), 0);
        assert_eq!(handles.get(first, 1), Err(HandleError::Stale));
        assert_eq!(handles.get(second, 2), Err(HandleError::Stale));
        assert_eq!(handles.len(), 0);
    }

    /// Verifies zero and structurally incomplete values never index the arena.
    #[test]
    fn malformed_handles_are_rejected_before_slot_lookup() {
        let handles = HandleTable::<u8>::new();
        assert_eq!(handles.get(0, 1), Err(HandleError::Malformed));
        assert_eq!(handles.get(1_u64 << KIND_SHIFT, 1), Err(HandleError::Malformed));
        assert_eq!(
            handles.get((1_u64 << GENERATION_SHIFT) | 1, 1),
            Err(HandleError::Malformed)
        );
    }
}
